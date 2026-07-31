"""Stage 9 (clamp): the deterministic safety net over the reasoning output.

Whatever the LLM (or rule engine) proposes is bounded here, so no unsafe change
can reach the report:

* min-sample gate: CR blocks below ``min_clean_meals`` and CF blocks below
  ``min_isolated_corrections`` are demoted to ``insufficient_data``;
* magnitude cap: every ``proposed_value`` is clamped to within ``max_change_pct``
  of the current value;
* CF confidence is capped at ``medium`` (never ``high``) on a closed loop;
* **evidence-direction guard**: a CR proposal whose direction contradicts what the
  block evidence implies (post-meal hypo -> up; high peak + low in-range + low hypo
  -> down) is vetoed to ``hold``. The old code only made ``direction`` consistent
  with the LLM's own number; it did not check the number against the evidence;
* **rule cross-check**: when the independent rule engine disagrees on direction (or
  abstains), confidence is capped at ``low``;
* ``direction`` is re-derived from the clamped numbers last, so it can never
  contradict the proposed value.

Every adjustment is recorded in a :class:`ClampAudit` for later inspection.
"""
from __future__ import annotations

from typing import Optional

from .contracts import (
    AnalysisSnapshot,
    BlockEvidence,
    ClampAudit,
    Config,
    PipelineState,
    Proposal,
    RecommendationSet,
)

_EPS = 1e-9


def _norm_param(s: str) -> str:
    s = (s or "").strip().upper()
    return "CF" if s.startswith("CF") or "KORR" in s else "CR"


def _norm_conf(s: str) -> str:
    s = (s or "").strip().lower()
    return s if s in ("low", "medium", "high") else "low"


def _norm_dir(s: str) -> str:
    s = (s or "").strip().lower()
    return s if s in ("up", "down", "hold") else "hold"


def _derive_direction(current: Optional[float], proposed: Optional[float], fallback: str) -> str:
    if current is None or proposed is None:
        return fallback
    if proposed > current + _EPS:
        return "up"
    if proposed < current - _EPS:
        return "down"
    return "hold"


def _expected_direction(ev: Optional[BlockEvidence], config: Config) -> Optional[str]:
    """The CR direction the block evidence implies, or None when ambiguous."""
    if ev is None:
        return None
    hypo = ev.pct_post_meal_hypo or 0.0
    peak = ev.median_peak_rise or 0.0
    in_range = ev.pct_in_range
    if hypo >= config.hypo_high_pct:
        return "up"                    # frequent post-meal hypo -> less meal insulin
    if (peak >= config.peak_high_mgdl and in_range is not None
            and in_range < config.inrange_low_pct and hypo < config.hypo_low_pct):
        return "down"                  # high excursion, little in range, no hypo -> more insulin
    return None


def _rule_directions(rule: Optional[RecommendationSet]) -> dict[tuple[str, str], str]:
    if rule is None:
        return {}
    return {(rp.block, _norm_param(rp.parameter)): _norm_dir(rp.direction) for rp in rule.proposals}


def apply(raw: RecommendationSet, snapshot: AnalysisSnapshot, config: Config,
          rule: Optional[RecommendationSet] = None) -> tuple[RecommendationSet, list[ClampAudit]]:
    ev_by_block = {b.block: b for b in snapshot.blocks}
    corr_by_block = {c.block: c for c in snapshot.corrections}
    rule_dir = _rule_directions(rule)
    audit: list[ClampAudit] = []
    demoted: set[str] = set()
    kept: list[Proposal] = []

    for p in raw.proposals:
        param = _norm_param(p.parameter)
        confidence = _norm_conf(p.confidence)
        ev = ev_by_block.get(p.block)

        if param == "CR":
            n = ev.n_clean_meals if ev else 0
            gate = config.min_clean_meals
            base = (ev.configured_cr if ev and ev.configured_cr is not None
                    else (ev.effective_cr if ev else None))
        else:  # CF
            ce = corr_by_block.get(p.block)
            n = ce.n_isolated if ce else 0
            gate = config.min_isolated_corrections
            base = (ce.configured_cf if ce and ce.configured_cf is not None
                    else (ce.observed_drop_per_unit if ce else None))

        if n < gate:
            demoted.add(p.block)
            audit.append(ClampAudit(p.block, param, "dropped", str(p.proposed_value), "None",
                                    f"sample n={n} below gate {gate}"))
            continue

        if param == "CF" and confidence == "high":
            audit.append(ClampAudit(p.block, param, "confidence", "high", "medium",
                                    "CF is low-confidence on a closed loop"))
            confidence = "medium"

        current = p.current_value if p.current_value is not None else base
        proposed = p.proposed_value

        if _norm_dir(p.direction) == "hold":
            proposed = current
        elif current is not None and proposed is not None:
            lo, hi = current * (1 - config.max_change_pct), current * (1 + config.max_change_pct)
            clamped = min(hi, max(lo, proposed))
            if abs(clamped - proposed) > _EPS:
                audit.append(ClampAudit(p.block, param, "proposed_value", str(round(proposed, 3)),
                                        str(round(clamped, 3)),
                                        f"exceeded +/-{int(config.max_change_pct * 100)}%"))
            proposed = clamped

        current = round(current, 2) if current is not None else None
        proposed = round(proposed, 2) if proposed is not None else None
        direction = _derive_direction(current, proposed, _norm_dir(p.direction))

        # (3) Evidence-direction guard (CR only): veto a change that contradicts
        # what the block's glucose pattern implies.
        if param == "CR" and direction in ("up", "down"):
            expected = _expected_direction(ev, config)
            if expected is not None and direction != expected:
                audit.append(ClampAudit(p.block, param, "direction", direction, "hold",
                                        "contradicts evidence (post-meal hypo / peak pattern)"))
                proposed, direction = current, "hold"

        # (4) Rule cross-check: cap confidence when the deterministic engine disagrees.
        rdir = rule_dir.get((p.block, param))
        disagrees = (rdir != direction) if rdir is not None else (direction != "hold")
        if rule is not None and disagrees and confidence != "low":
            audit.append(ClampAudit(p.block, param, "confidence", confidence, "low",
                                    "rule-engine cross-check disagreed"))
            confidence = "low"

        kept.append(Proposal(
            block=p.block, parameter=param, direction=direction,
            current_value=current, proposed_value=proposed, confidence=confidence,
            rationale=p.rationale, caveats=p.caveats,
        ))

    kept_blocks = {p.block for p in kept}
    insufficient = sorted((set(raw.insufficient_data_blocks) | demoted) - kept_blocks)
    clamped = RecommendationSet(proposals=kept, overall_narrative=raw.overall_narrative,
                                insufficient_data_blocks=insufficient)
    return clamped, audit


def run(state: PipelineState, config: Config) -> PipelineState:
    if state.recommendation_raw is None or state.snapshot is None:
        raise ValueError("clamp stage requires state.recommendation_raw and state.snapshot")
    state.recommendation, state.clamp_audit = apply(
        state.recommendation_raw, state.snapshot, config, state.recommendation_rule)
    return state

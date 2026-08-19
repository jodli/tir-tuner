"""Stage 8 (recommend): the judgment step.

Turns the :class:`AnalysisSnapshot` into a :class:`RecommendationSet`. With
``config.use_llm`` (default) it calls the BAML ``RecommendSettings`` function
(model configured in ``baml_src/clients.baml``); otherwise a deterministic rule
engine produces the same shape so the pipeline runs offline and is testable
without a network. Either way the raw output is bounded afterwards by the clamp
stage (:mod:`analysis.clamp`); this stage never enforces numeric limits itself.
"""
from __future__ import annotations

import json

from .contracts import (
    AnalysisSnapshot,
    BlockEvidence,
    Config,
    PipelineState,
    Proposal,
    RecommendationSet,
)


def recommend(snapshot: AnalysisSnapshot, config: Config) -> RecommendationSet:
    return _llm(snapshot) if config.use_llm else _rule_based(snapshot, config)


# --- LLM path ------------------------------------------------------------
def _llm(snapshot: AnalysisSnapshot) -> RecommendationSet:
    from baml_client.sync_client import b  # imported lazily: needs generated client + network

    res = b.RecommendSettings(json.dumps(snapshot.to_json(), ensure_ascii=False))
    return RecommendationSet(
        proposals=[
            Proposal(
                block=p.block,
                parameter=p.parameter,
                direction=p.direction,
                current_value=p.current_value,
                proposed_value=p.proposed_value,
                confidence=p.confidence,
                rationale=p.rationale,
                caveats=p.caveats,
            )
            for p in res.proposals
        ],
        overall_narrative=res.overall_narrative,
        insufficient_data_blocks=list(res.insufficient_data_blocks),
    )


# --- Deterministic fallback ---------------------------------------------
def _rule_based(snapshot: AnalysisSnapshot, config: Config) -> RecommendationSet:
    proposals: list[Proposal] = []
    insufficient: list[str] = []
    for ev in snapshot.blocks:
        base = ev.configured_cr if ev.configured_cr is not None else ev.effective_cr
        if ev.n_clean_meals < config.min_clean_meals or base is None:
            insufficient.append(ev.block)
            continue
        p = _cr_rule(ev, base, config)
        if p is not None:
            proposals.append(p)

    for ce in snapshot.corrections:
        if ce.n_isolated >= config.min_isolated_corrections and ce.configured_cf and ce.observed_drop_per_unit:
            gap = (ce.observed_drop_per_unit - ce.configured_cf) / ce.configured_cf
            if abs(gap) > 0.2:
                step = config.max_change_pct * (1 if gap > 0 else -1)
                proposals.append(Proposal(
                    block=ce.block, parameter="CF", direction="up" if gap > 0 else "down",
                    current_value=ce.configured_cf, proposed_value=round(ce.configured_cf * (1 + step), 2),
                    confidence="low",
                    rationale=(f"Beobachteter Abfall pro Einheit ({ce.observed_drop_per_unit}) weicht vom "
                               f"konfigurierten CF ({ce.configured_cf}) ab."),
                    caveats="Sehr unsicher: Korrekturen werden im Closed Loop stark von der Basalrate überlagert.",
                ))
    narrative = (f"Gesamt-TIR {snapshot.overall_tir} % (Mittel {snapshot.overall_mean} mg/dl, "
                 f"CV {snapshot.overall_cv} %). Vorschläge nur für Blöcke mit klarer, ausreichender Evidenz.")
    return RecommendationSet(proposals=proposals, overall_narrative=narrative, insufficient_data_blocks=insufficient)


def _cr_rule(ev: BlockEvidence, base: float, config: Config) -> Proposal | None:
    hypo = ev.pct_post_meal_hypo or 0.0
    peak = ev.median_peak_rise or 0.0
    in_range = ev.pct_in_range
    step = config.max_change_pct
    if hypo >= config.hypo_high_pct:
        # Too much meal insulin -> raise the g/U number (less insulin per carb).
        return Proposal(
            block=ev.block, parameter="CR", direction="up",
            current_value=base, proposed_value=round(base * (1 + step), 2), confidence="medium",
            rationale=(f"Im Block {ev.block} enden {hypo} % der Mahlzeiten in einer Unterzuckerung "
                       f"(1,5-4 h). Das CR wird leicht angehoben (weniger Insulin pro KH)."),
            caveats="Effektives CR aus Bolus/KH geschätzt; mögliche Loop-Korrektur nicht herausgerechnet.",
        )
    if (peak >= config.peak_high_mgdl and (in_range is not None and in_range < config.inrange_low_pct)
            and hypo < config.hypo_low_pct):
        # Too little meal insulin -> lower the g/U number (more insulin per carb).
        return Proposal(
            block=ev.block, parameter="CR", direction="down",
            current_value=base, proposed_value=round(base * (1 - step), 2), confidence="medium",
            rationale=(f"Im Block {ev.block} hoher Anstieg nach dem Essen (Median {peak} mg/dl) und nur "
                       f"{in_range} % im Zielbereich, kaum Unterzuckerungen. CR wird leicht gesenkt."),
            caveats="Effektives CR aus Bolus/KH geschätzt; mögliche Loop-Korrektur nicht herausgerechnet.",
        )
    return None


def run(state: PipelineState, config: Config) -> PipelineState:
    if state.snapshot is None:
        raise ValueError("recommend stage requires state.snapshot")
    state.recommendation_raw = recommend(state.snapshot, config)
    # Always compute the deterministic rule result too: the clamp uses it as an
    # independent cross-check to damp confidence when the two disagree.
    state.recommendation_rule = _rule_based(state.snapshot, config)
    return state

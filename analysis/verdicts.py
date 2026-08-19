"""Stage (verdicts): attribute the TIR loss to blocks and judge every block.

Two problems this stage fixes in the output:

* The per-block table listed six TIR values with equal weight, although the
  blocks differ in length. Weighting each block's out-of-range share by its time
  share turns them into pp of the *overall* TIR shortfall, which is what says
  where the problem actually is: on the last real run, 68% of the 27.3 pp lost
  sat in the afternoon and evening, and almost all of it was time above 180.
* Blocks without a proposal simply vanished from the recommendations, so the
  second-worst block (TIR 55%) produced no output at all while the best block got
  the only change. Every block now gets a verdict, and a problem block with no CR
  lever says so and names the likely one instead of staying silent.

Runs after the clamp, so it judges the *clamped* proposals, and it only reads
already-computed numbers.
"""
from __future__ import annotations

from typing import Optional

from .contracts import (
    AnalysisSnapshot,
    BlockEvidence,
    BlockVerdict,
    Config,
    GlycemicMetrics,
    PipelineState,
    Proposal,
    RecommendationSet,
)
from .strings import BLOCK_DE


def _label(config: Config, key: str) -> str:
    for b in config.blocks:
        if b.key == key:
            return BLOCK_DE.get(b.name, key)
    return key


def _dominant(ev: BlockEvidence) -> Optional[str]:
    """Is the block losing time mostly above 180, mostly below 70, or both?"""
    low, high = ev.tbr_70, ev.tar_180
    if low is None or high is None:
        return None
    if low <= 0 and high <= 0:
        return None
    if high >= 3 * max(low, 0.1):
        return "high"
    if low >= 3 * max(high, 0.1):
        return "low"
    return "mixed"


def _note(ev: BlockEvidence, verdict: str, dominant: Optional[str],
          proposal: Optional[Proposal], config: Config) -> str:
    """One German line saying what this block means and what to look at."""
    if verdict == "change" and proposal is not None:
        return (f"Änderung vorgeschlagen: {proposal.parameter} "
                f"{proposal.current_value} → {proposal.proposed_value}.")

    reasons: list[str] = []
    if ev.n_suspected_no_delivery:
        reasons.append(f"{ev.n_suspected_no_delivery}× Verdacht auf Abgabestörung "
                       f"(Pumpe/Katheter prüfen, kein CR-Signal)")
    if ev.n_clean_meals < config.min_clean_meals:
        reasons.append(f"nur {ev.n_clean_meals} saubere Mahlzeiten "
                       f"(Gate {config.min_clean_meals})")
    if ev.dawn_rise:
        reasons.append("Dawn-Anstieg erklärt Hochwerte ohne CR-Bezug")
    if ev.coverage_pct is not None and ev.coverage_pct < config.min_coverage_pct:
        reasons.append(f"Abdeckung nur {ev.coverage_pct} %")
    if ev.ci_low is not None and ev.ci_high is not None and ev.effective_cr:
        span = ev.ci_high - ev.ci_low
        if span > 0.2 * ev.effective_cr:
            reasons.append(f"breites Konfidenzintervall ({ev.ci_low}–{ev.ci_high})")

    if verdict == "ok":
        # A block at target needs no excuses; only data gaps are still worth saying.
        gaps = [r for r in reasons if r.startswith("Abdeckung")]
        return "Im Ziel, kein Handlungsbedarf." if not gaps else \
            "Im Ziel; " + ", ".join(gaps) + "."

    if verdict == "no_cr_lever":
        where = {"high": "Der Verlust ist fast nur Zeit über 180",
                 "low": "Der Verlust ist fast nur Zeit unter 70",
                 "mixed": "Der Verlust geht in beide Richtungen"}.get(
                     dominant, "Der Verlust ist nicht eindeutig zuzuordnen")
        lever = {"high": "Basalrate/Bolus-Timing bzw. Mahlzeiten außerhalb der Boli",
                 "low": "Basalrate bzw. Korrekturverhalten",
                 "mixed": "Timing und Basalrate"}.get(dominant, "Basalrate und Timing")
        head = f"Problemblock ohne CR-Hebel: {', '.join(reasons)}. " if reasons else \
               "Problemblock ohne belastbaren CR-Hebel. "
        return f"{head}{where} → {lever} mit dem Behandlungsteam prüfen."

    # watch
    if proposal is not None and proposal.direction == "hold":
        base = "Beobachten, keine Änderung: "
        return base + (", ".join(reasons) + "." if reasons else
                       "Evidenz reicht für eine Änderung nicht aus.")
    return "Beobachten: " + (", ".join(reasons) + "." if reasons else
                             "unauffällig, aber unter Ziel.")


def build(glycemic: GlycemicMetrics, snapshot: AnalysisSnapshot,
          recommendation: Optional[RecommendationSet], config: Config) -> list[BlockVerdict]:
    ev_by_block = {b.block: b for b in snapshot.blocks}
    # A CR proposal wins over a CF one when both exist: it is the stronger lever.
    props: dict[str, Proposal] = {}
    for p in (recommendation.proposals if recommendation else []):
        cur = props.get(p.block)
        if cur is None or (cur.parameter != "CR" and p.parameter == "CR"):
            props[p.block] = p
    insufficient = set(recommendation.insufficient_data_blocks if recommendation else [])

    total_readings = sum(b.n_readings for b in glycemic.per_block.values())
    loss: dict[str, Optional[float]] = {}
    for key, band in glycemic.per_block.items():
        if not total_readings or band.tir is None:
            loss[key] = None
            continue
        loss[key] = round(band.n_readings / total_readings * (100.0 - band.tir), 2)
    total_loss = sum(v for v in loss.values() if v) or 0.0

    out: list[BlockVerdict] = []
    for b in config.blocks:
        ev = ev_by_block.get(b.key)
        band = glycemic.per_block.get(b.key)
        if ev is None or band is None:
            continue
        share = round(100.0 * band.n_readings / total_readings, 1) if total_readings else None
        loss_pp = loss.get(b.key)
        loss_share = round(100.0 * loss_pp / total_loss, 1) if (loss_pp and total_loss) else None
        dominant = _dominant(ev)
        proposal = props.get(b.key)

        below_target = ev.tir is not None and ev.tir < config.tir_target_pct
        if proposal is not None and proposal.direction in ("up", "down"):
            verdict = "change"
        elif below_target and (b.key in insufficient or proposal is None):
            # Losing time and nothing to turn: say which lever to look at instead
            # of dropping the block from the output.
            verdict = "no_cr_lever"
        elif proposal is not None or below_target:
            verdict = "watch"
        else:
            verdict = "ok"

        out.append(BlockVerdict(
            block=b.key,
            label=_label(config, b.key),
            share_pct=share,
            tir=ev.tir,
            tbr_70=ev.tbr_70,
            tar_180=ev.tar_180,
            loss_pp=loss_pp,
            loss_share_pct=loss_share,
            dominant=dominant,
            verdict=verdict,
            note=_note(ev, verdict, dominant, proposal, config),
            parameter=proposal.parameter if proposal else None,
            direction=proposal.direction if proposal else None,
            current_value=proposal.current_value if proposal else None,
            proposed_value=proposal.proposed_value if proposal else None,
        ))
    return out


def run(state: PipelineState, config: Config) -> PipelineState:
    for name in ("glycemic", "snapshot"):
        if getattr(state, name) is None:
            raise ValueError(f"verdicts stage requires state.{name}")
    state.verdicts = build(state.glycemic, state.snapshot, state.recommendation, config)
    return state

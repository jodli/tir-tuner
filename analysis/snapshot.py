"""Stage (snapshot): assemble the typed evidence bundle for the reasoning step.

Pure assembly: every measurable thing is already computed by the upstream stages
(glycemic, meals, corrections, iob, confounders, trends, stats, backtest) and this
stage only joins them per block into the single :class:`AnalysisSnapshot` the
reasoning step consumes. History I/O lives in :mod:`analysis.trends`; this stage
does no I/O.
"""
from __future__ import annotations

import datetime as dt
from typing import Optional

from . import trends
from .contracts import (
    AnalysisSnapshot,
    BlockEvidence,
    Config,
    CorrectionEvidence,
    PipelineState,
    RunRef,
    block_for_hour,
)
from .settings import value_for_block

# Signals the export simply does not carry; surfaced so the reasoning step can
# state its blind spots every run.
UNAVAILABLE_SIGNALS = [
    "exercise", "illness", "meal_fat_protein", "announced_vs_actual_carbs",
    "correction_trigger_bg", "sleep", "stress",
]


def _gap_pct(effective: Optional[float], configured: Optional[float]) -> Optional[float]:
    if effective is None or configured in (None, 0):
        return None
    return round(100.0 * (effective - configured) / configured, 1)


def _delta(cur: Optional[float], prev: Optional[float]) -> Optional[float]:
    if cur is None or prev is None:
        return None
    return round(cur - prev, 2)


def _corr_no_delivery_by_block(state: PipelineState, config: Config) -> dict[str, int]:
    counts: dict[str, int] = {}
    if state.iob is None:
        return counts
    for e in state.iob.per_event:
        if e.kind == "correction" and e.suspected_no_delivery:
            hour = dt.datetime.fromisoformat(e.time).hour
            key = block_for_hour(hour, config.blocks).key
            counts[key] = counts.get(key, 0) + 1
    return counts


def build_snapshot(state: PipelineState, config: Config, prior) -> AnalysisSnapshot:
    gly = state.glycemic
    meals = state.meals
    corr = state.corrections
    settings = state.settings

    stats_by_block = state.stats.per_block if state.stats else {}
    changes_by_block = {c.block: c for c in (settings.cr_changes if settings else [])}
    backtest_by_block = state.backtest.per_block if state.backtest else {}
    corr_nodel = _corr_no_delivery_by_block(state, config)
    dawn = state.confounders.dawn_rise if state.confounders else None

    blocks: list[BlockEvidence] = []
    corrections: list[CorrectionEvidence] = []
    for b in config.blocks:
        mb = meals.per_block[b.key]
        gb = gly.per_block[b.key]
        rs = stats_by_block.get(b.key)
        configured_cr = value_for_block(settings.carb_ratio, b) if settings.available else None
        prior_cr = prior.per_block_effective_cr.get(b.key) if prior else None
        prior_tir = prior.per_block_tir.get(b.key) if prior else None
        bt = backtest_by_block.get(b.key)
        # Dawn only sensibly applies to overnight / early-morning blocks.
        dawn_here = dawn if (dawn is not None and b.start_hour < 11) else None

        blocks.append(BlockEvidence(
            block=b.key,
            label=b.name,
            configured_cr=configured_cr,
            effective_cr=mb.median_effective_cr,
            cr_gap_pct=_gap_pct(mb.median_effective_cr, configured_cr),
            n_clean_meals=mb.n_clean,
            median_peak_rise=mb.median_peak_rise,
            pct_in_range=mb.pct_in_range,
            pct_post_meal_hypo=mb.pct_post_meal_hypo,
            tir=gb.tir,
            tbr_70=gb.tbr_70,
            tar_180=gb.tar_180,
            trend_effective_cr_delta=_delta(mb.median_effective_cr, prior_cr),
            trend_tir_delta=_delta(gb.tir, prior_tir),
            cv=gb.cv,
            tbr_54=gb.tbr_54,
            titr=gb.titr,
            coverage_pct=gb.coverage_pct,
            effective_cr_q25=mb.effective_cr_q25,
            effective_cr_q75=mb.effective_cr_q75,
            median_time_to_peak_min=mb.median_time_to_peak_min,
            median_auc_over_baseline=mb.median_auc_over_baseline,
            median_undershoot_depth=mb.median_undershoot_depth,
            median_effective_cr_inrange_start=mb.median_effective_cr_inrange_start,
            n_inrange_start=mb.n_inrange_start,
            ci_low=rs.ci_low if rs else None,
            ci_high=rs.ci_high if rs else None,
            delta_significant=rs.delta_significant if rs else None,
            cr_iob_filtered=rs.cr_iob_filtered if rs else None,
            n_high_iob=rs.n_high_iob if rs else 0,
            n_suspected_no_delivery=rs.n_suspected_no_delivery if rs else 0,
            dawn_rise=dawn_here,
            config_last_change=changes_by_block.get(b.key),
            backtest_outcome=bt.outcome if bt else None,
        ))

        cb = corr.per_block[b.key]
        block_confounds = sorted({c for f in corr.corrections if f.block == b.key for c in f.confounds})
        configured_cf = value_for_block(settings.correction_factor, b) if settings.available else None
        corrections.append(CorrectionEvidence(
            block=b.key,
            configured_cf=configured_cf,
            observed_drop_per_unit=cb.median_drop_per_unit,
            n_isolated=cb.n_isolated,
            confounds=block_confounds,
            n_suspected_no_delivery=corr_nodel.get(b.key, 0),
        ))

    o = gly.overall
    return AnalysisSnapshot(
        as_of=state.window.as_of,
        window_weeks=state.window.weeks,
        overall_tir=o.tir,
        overall_tbr_70=o.tbr_70,
        overall_tar_180=o.tar_180,
        overall_mean=o.mean,
        overall_cv=o.cv,
        overall_gmi=o.gmi,
        settings_available=settings.available,
        blocks=blocks,
        corrections=corrections,
        prior_run_date=prior.as_of if prior else None,
        prior_runs=_prior_series(state, config),
        unavailable_signals=list(UNAVAILABLE_SIGNALS),
        active_flags=_active_flags(state, stats_by_block, dawn),
    )


def _prior_series(state: PipelineState, config: Config) -> list[RunRef]:
    if state.trends is None or state.window is None:
        return []
    cur = state.window.as_of
    earlier = [r for r in state.trends.prior if r.as_of < cur]
    return earlier[-config.history_series_len:]


def _active_flags(state: PipelineState, stats_by_block: dict, dawn: Optional[bool]) -> list[str]:
    flags: list[str] = []
    if state.iob is not None and state.iob.available:
        flags.append("iob_available")
    if dawn:
        flags.append("dawn_rise")
    for key, rs in stats_by_block.items():
        if rs.n_suspected_no_delivery:
            flags.append(f"no_delivery:{key}={rs.n_suspected_no_delivery}")
    return flags


def run(state: PipelineState, config: Config) -> PipelineState:
    for name in ("glycemic", "meals", "corrections", "settings", "window", "trends"):
        if getattr(state, name) is None:
            raise ValueError(f"snapshot stage requires state.{name}")
    prior = trends.most_recent_prior(state.trends)
    state.snapshot = build_snapshot(state, config, prior)
    return state

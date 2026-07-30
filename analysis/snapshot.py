"""Stage 7 (snapshot): assemble the typed evidence bundle for the reasoning step.

Combines glycemic metrics, meal-derived effective CR, correction-derived CF,
resolved settings and cross-run trends into a single :class:`AnalysisSnapshot`.
This is the LLM's only input: everything measurable is computed here so the
reasoning step is pure judgment over already-computed numbers.
"""
from __future__ import annotations

from typing import Optional

from . import history
from .contracts import (
    AnalysisSnapshot,
    BlockEvidence,
    Config,
    CorrectionEvidence,
    PipelineState,
    ResolvedSettings,
    RunRef,
)
from .settings import value_for_block


def _gap_pct(effective: Optional[float], configured: Optional[float]) -> Optional[float]:
    if effective is None or configured in (None, 0):
        return None
    return round(100.0 * (effective - configured) / configured, 1)


def _delta(cur: Optional[float], prev: Optional[float]) -> Optional[float]:
    if cur is None or prev is None:
        return None
    return round(cur - prev, 2)


def build_snapshot(state: PipelineState, config: Config, prior: Optional[RunRef]) -> AnalysisSnapshot:
    gly = state.glycemic
    meals = state.meals
    corr = state.corrections
    settings: ResolvedSettings = state.settings

    blocks: list[BlockEvidence] = []
    corrections: list[CorrectionEvidence] = []
    for b in config.blocks:
        mb = meals.per_block[b.key]
        gb = gly.per_block[b.key]
        configured_cr = value_for_block(settings.carb_ratio, b) if settings.available else None
        prior_cr = prior.per_block_effective_cr.get(b.key) if prior else None
        prior_tir = prior.per_block_tir.get(b.key) if prior else None
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
    )


def run(state: PipelineState, config: Config) -> PipelineState:
    for name in ("glycemic", "meals", "corrections", "settings", "window"):
        if getattr(state, name) is None:
            raise ValueError(f"snapshot stage requires state.{name}")
    current = history.build_current_ref(state.window.as_of, state.glycemic, state.meals)
    state.trends = history.compute_trends(history.load_refs(config), current)
    prior = history.most_recent_prior(state.trends)
    state.snapshot = build_snapshot(state, config, prior)
    return state

"""Stage (stats): robust effective-CR estimation.

Turns the raw per-meal effective-CR sample into estimates the reasoning step can
trust: a bootstrap confidence interval on the median (so week-to-week noise on a
thin sample is visible as a wide CI), a significance flag versus the most recent
prior run, and an IOB/no-delivery-filtered median that drops meals whose CR
estimate is contaminated by residual insulin or a suspected delivery failure.

Bootstrap uses a fixed seed (``Config.bootstrap_seed``) so results are
deterministic and testable. Reuses the meal stage's fence + median helpers.
"""
from __future__ import annotations

from typing import Optional

import numpy as np

from .contracts import (
    BlockRobustStats,
    Config,
    IobAnalysis,
    MealAnalysis,
    PipelineState,
    RobustStats,
    RunRef,
)
from .meals import _in_fences, _median
from .trends import most_recent_prior


def _bootstrap_ci(values: list[float], config: Config, rng) -> tuple[Optional[float], Optional[float]]:
    if len(values) < config.min_ci_samples:
        return None, None
    arr = np.asarray(values, dtype=float)
    idx = rng.integers(0, len(arr), size=(config.n_boot, len(arr)))
    meds = np.median(arr[idx], axis=1)
    tail = (100.0 - config.ci_pct) / 2.0
    return round(float(np.percentile(meds, tail)), 2), round(float(np.percentile(meds, 100.0 - tail)), 2)


def analyze(meals: MealAnalysis, iob: Optional[IobAnalysis], prior: Optional[RunRef], config: Config) -> RobustStats:
    iob_by_time = {e.time: e for e in (iob.per_event if iob else []) if e.kind == "meal"}
    rng = np.random.default_rng(config.bootstrap_seed)

    per_block: dict[str, BlockRobustStats] = {}
    for b in config.blocks:
        clean = [f for f in meals.meals if f.block == b.key and f.clean]
        cr = [f.effective_cr for f in clean if _in_fences(f.effective_cr, config)]
        median = _median(cr)
        ci_low, ci_high = _bootstrap_ci(cr, config, rng)

        kept: list[float] = []
        n_high_iob = n_nodel = 0
        for f in clean:
            if not _in_fences(f.effective_cr, config):
                continue
            e = iob_by_time.get(f.time)
            if e is not None and e.suspected_no_delivery:
                n_nodel += 1
                continue
            if e is not None and e.iob_before is not None and e.iob_before > config.iob_contamination_units:
                n_high_iob += 1
                continue
            kept.append(f.effective_cr)

        prior_cr = prior.per_block_effective_cr.get(b.key) if prior else None
        delta = round(median - prior_cr, 2) if (median is not None and prior_cr is not None) else None
        sig: Optional[bool] = None
        if prior_cr is not None and ci_low is not None and ci_high is not None:
            sig = not (ci_low <= prior_cr <= ci_high)

        per_block[b.key] = BlockRobustStats(
            block=b.key,
            n=len(cr),
            median_effective_cr=median,
            ci_low=ci_low,
            ci_high=ci_high,
            delta_vs_prior=delta,
            delta_significant=sig,
            cr_iob_filtered=_median(kept),
            n_high_iob=n_high_iob,
            n_suspected_no_delivery=n_nodel,
        )
    return RobustStats(per_block=per_block)


def run(state: PipelineState, config: Config) -> PipelineState:
    for name in ("meals", "trends"):
        if getattr(state, name) is None:
            raise ValueError(f"stats stage requires state.{name}")
    prior = most_recent_prior(state.trends)
    state.stats = analyze(state.meals, state.iob, prior, config)
    return state

"""Stage 4 (meals): effective carb ratio and post-meal excursions per block.

For each meal bolus (carbs > 0, units > 0) the *effective* carb ratio the pump
used is ``carbs / units`` (the export does not log the configured ratio). Only
"clean" meals (no other meal bolus within a gap) are aggregated, so an
excursion can be attributed to a single meal. Post-meal CGM features let later
stages judge whether the ratio in that block is too weak or too strong.

Assumption: with BG-entry logged as 0 on this closed loop, meal-bolus units are
treated as a pure carb dose. Any loop-added correction inflates the units and
therefore lowers the apparent effective CR; this is surfaced downstream.
"""
from __future__ import annotations

import statistics
from typing import Optional

import numpy as np
import pandas as pd

from ._cgm import CgmLookup
from .contracts import (
    Config,
    MealAnalysis,
    MealBlockStats,
    MealFeature,
    PipelineState,
    block_for_hour,
)


_trapz = np.trapezoid if hasattr(np, "trapezoid") else np.trapz   # renamed in numpy 2.0


def _minutes(a, b) -> float:
    """Signed minutes between two timestamps (numpy datetime64 or pandas Timestamp)."""
    return float((np.datetime64(pd.Timestamp(a)) - np.datetime64(pd.Timestamp(b))) / np.timedelta64(1, "m"))


def _median(xs: list[Optional[float]]) -> Optional[float]:
    vals = [x for x in xs if x is not None and not (isinstance(x, float) and np.isnan(x))]
    return round(statistics.median(vals), 2) if vals else None


def _pct_true(flags: list[Optional[bool]]) -> Optional[float]:
    known = [f for f in flags if f is not None]
    return round(100.0 * sum(known) / len(known), 2) if known else None


def _clean_flags(times: np.ndarray, gap_min: int) -> list[bool]:
    """A meal is clean if no *other* meal bolus falls within +/- gap."""
    gap = np.timedelta64(gap_min, "m")
    flags = []
    for i in range(len(times)):
        others = np.delete(times, i)
        flags.append(bool(len(others) == 0 or not np.any(np.abs(others - times[i]) <= gap)))
    return flags


def analyze(cgm: pd.DataFrame, bolus: pd.DataFrame, config: Config) -> MealAnalysis:
    meals = bolus[(bolus["kind"] == "meal") & (bolus["carbs"] > 0) & (bolus["total_units"] > 0)]
    meals = meals.sort_values("time").reset_index(drop=True)
    lookup = CgmLookup(cgm)
    times = meals["time"].to_numpy(dtype="datetime64[ns]")
    clean_flags = _clean_flags(times, config.meal_clean_gap_min)

    features: list[MealFeature] = []
    for i, row in meals.iterrows():
        t = row["time"]
        carbs = float(row["carbs"])
        units = float(row["total_units"])
        block = block_for_hour(t.hour, config.blocks).key
        baseline = lookup.nearest(t, tol_min=15)
        peak_t, peak_v = lookup.window_tv(t, 0.0, config.excursion_peak_h)
        tail_t, tail_v = lookup.window_tv(t, 0.0, config.excursion_tail_h)
        val_3h = lookup.at_offset(t, 3.0)
        val_4h = lookup.at_offset(t, 4.0)

        # Peak rise + time-to-peak: a fast early spike that still lands in range is
        # a timing/pre-bolus issue, not a CR magnitude issue.
        peak_rise = time_to_peak_min = None
        if len(peak_v) and baseline is not None:
            pidx = int(np.argmax(peak_v))
            peak_rise = float(peak_v[pidx]) - baseline
            time_to_peak_min = _minutes(peak_t[pidx], t)

        # Positive area over baseline over the tail window (overall exposure).
        auc_over_baseline = None
        if len(tail_v) >= 2 and baseline is not None:
            mins = np.array([_minutes(x, t) for x in tail_t], dtype=float)
            auc_over_baseline = round(float(_trapz(np.clip(tail_v - baseline, 0.0, None), mins)), 1)

        min_0_4h = float(tail_v.min()) if len(tail_v) else None
        delta_3h = (val_3h - baseline) if (val_3h is not None and baseline is not None) else None
        delta_4h = (val_4h - baseline) if (val_4h is not None and baseline is not None) else None
        ended_in_range = (config.tir_low <= val_3h <= config.tir_high) if val_3h is not None else None

        # Post-meal hypo + undershoot are judged from the insulin-tail window only,
        # so an early transient dip (or pre-meal low) is not mistaken for an
        # over-bolus.
        hypo_t, hypo_v = lookup.window_tv(t, config.hypo_start_h, config.excursion_tail_h)
        post_meal_hypo = (float(hypo_v.min()) < config.tir_low) if len(hypo_v) else None
        undershoot_depth = undershoot_dur_min = rebound = None
        if len(hypo_v):
            below = hypo_v < config.tir_low
            if below.any():
                nadir_val = float(hypo_v.min())
                nadir_idx = int(np.argmin(hypo_v))
                below_t = hypo_t[below]
                undershoot_depth = round(config.tir_low - nadir_val, 1)
                undershoot_dur_min = _minutes(below_t.max(), below_t.min())
                rebound = round(float(hypo_v[nadir_idx:].max()) - nadir_val, 1)

        features.append(MealFeature(
            time=t.isoformat(),
            block=block,
            carbs=carbs,
            units=round(units, 3),
            effective_cr=round(carbs / units, 2) if units else None,
            baseline_mgdl=baseline,
            peak_rise=round(peak_rise, 1) if peak_rise is not None else None,
            delta_3h=round(delta_3h, 1) if delta_3h is not None else None,
            delta_4h=round(delta_4h, 1) if delta_4h is not None else None,
            min_0_4h=min_0_4h,
            ended_in_range=ended_in_range,
            post_meal_hypo=post_meal_hypo,
            clean=clean_flags[i],
            time_to_peak_min=round(time_to_peak_min, 1) if time_to_peak_min is not None else None,
            auc_over_baseline=auc_over_baseline,
            undershoot_depth=undershoot_depth,
            undershoot_dur_min=undershoot_dur_min,
            rebound=rebound,
        ))

    per_block = _aggregate(features, config)
    return MealAnalysis(meals=features, per_block=per_block)


def _in_fences(cr: Optional[float], config: Config) -> bool:
    return cr is not None and config.cr_plausible_min <= cr <= config.cr_plausible_max


def _aggregate(features: list[MealFeature], config: Config) -> dict[str, MealBlockStats]:
    per_block: dict[str, MealBlockStats] = {}
    for b in config.blocks:
        clean = [f for f in features if f.block == b.key and f.clean]

        # Trim implausible effective-CR (mis-logged carbs/units) before aggregating.
        cr_all = [f.effective_cr for f in clean if f.effective_cr is not None]
        cr_kept = [c for c in cr_all if _in_fences(c, config)]
        n_trimmed = len(cr_all) - len(cr_kept)
        q25 = round(float(np.percentile(cr_kept, 25)), 2) if cr_kept else None
        q75 = round(float(np.percentile(cr_kept, 75)), 2) if cr_kept else None

        # Segment the (trimmed) effective CR by starting glucose: a high start
        # likely folded a correction into the bolus, deflating the apparent CR.
        inrange = [f.effective_cr for f in clean if _in_fences(f.effective_cr, config)
                   and f.baseline_mgdl is not None and config.tir_low <= f.baseline_mgdl <= config.tir_high]
        high = [f.effective_cr for f in clean if _in_fences(f.effective_cr, config)
                and f.baseline_mgdl is not None and f.baseline_mgdl > config.tir_high]

        per_block[b.key] = MealBlockStats(
            block=b.key,
            n_clean=len(clean),
            median_effective_cr=_median(cr_kept),
            median_peak_rise=_median([f.peak_rise for f in clean]),
            pct_in_range=_pct_true([f.ended_in_range for f in clean]),
            pct_post_meal_hypo=_pct_true([f.post_meal_hypo for f in clean]),
            effective_cr_q25=q25,
            effective_cr_q75=q75,
            effective_cr_min=round(min(cr_kept), 2) if cr_kept else None,
            effective_cr_max=round(max(cr_kept), 2) if cr_kept else None,
            n_trimmed=n_trimmed,
            median_time_to_peak_min=_median([f.time_to_peak_min for f in clean]),
            median_auc_over_baseline=_median([f.auc_over_baseline for f in clean]),
            median_undershoot_depth=_median([f.undershoot_depth for f in clean]),
            median_effective_cr_inrange_start=_median(inrange),
            n_inrange_start=len(inrange),
            median_effective_cr_high_start=_median(high),
            n_high_start=len(high),
        )
    return per_block


def run(state: PipelineState, config: Config) -> PipelineState:
    if state.dataset is None:
        raise ValueError("meals stage requires state.dataset")
    state.meals = analyze(state.dataset.cgm, state.dataset.bolus, config)
    return state

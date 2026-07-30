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
        peak_window = lookup.window(t, config.excursion_peak_h)
        tail_window = lookup.window(t, config.excursion_tail_h)
        val_3h = lookup.at_offset(t, 3.0)
        val_4h = lookup.at_offset(t, 4.0)

        # Post-meal hypo is judged from the insulin-tail window only, so an
        # early transient dip (or pre-meal low) is not mistaken for an over-bolus.
        hypo_window = lookup.window_between(t, config.hypo_start_h, config.excursion_tail_h)
        peak_rise = (float(peak_window.max()) - baseline) if (len(peak_window) and baseline is not None) else None
        min_0_4h = float(tail_window.min()) if len(tail_window) else None
        delta_3h = (val_3h - baseline) if (val_3h is not None and baseline is not None) else None
        delta_4h = (val_4h - baseline) if (val_4h is not None and baseline is not None) else None
        ended_in_range = (config.tir_low <= val_3h <= config.tir_high) if val_3h is not None else None
        post_meal_hypo = (float(hypo_window.min()) < config.tir_low) if len(hypo_window) else None

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
        ))

    per_block = _aggregate(features, config)
    return MealAnalysis(meals=features, per_block=per_block)


def _aggregate(features: list[MealFeature], config: Config) -> dict[str, MealBlockStats]:
    per_block: dict[str, MealBlockStats] = {}
    for b in config.blocks:
        clean = [f for f in features if f.block == b.key and f.clean]
        per_block[b.key] = MealBlockStats(
            block=b.key,
            n_clean=len(clean),
            median_effective_cr=_median([f.effective_cr for f in clean]),
            median_peak_rise=_median([f.peak_rise for f in clean]),
            pct_in_range=_pct_true([f.ended_in_range for f in clean]),
            pct_post_meal_hypo=_pct_true([f.post_meal_hypo for f in clean]),
        )
    return per_block


def run(state: PipelineState, config: Config) -> PipelineState:
    if state.dataset is None:
        raise ValueError("meals stage requires state.dataset")
    state.meals = analyze(state.dataset.cgm, state.dataset.bolus, config)
    return state

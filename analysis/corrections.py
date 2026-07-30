"""Stage 5 (corrections): observed glucose drop per correction unit.

Correction boluses (carbs == 0, units > 0) let us estimate an *effective*
correction factor as the observed CGM drop per unit over the following hours.
On a hybrid closed loop this is inherently low-confidence: basal is modulated
continuously and there is no logged trigger BG, so confounds are flagged and
only "isolated" corrections (no meal nearby) feed the aggregate.
"""
from __future__ import annotations

import statistics
from typing import Optional

import numpy as np
import pandas as pd

from ._cgm import CgmLookup
from .contracts import (
    Config,
    CorrectionAnalysis,
    CorrectionBlockStats,
    CorrectionFeature,
    PipelineState,
    block_for_hour,
)

_DROP_WINDOW_H = 3.0


def _median(xs: list[Optional[float]]) -> Optional[float]:
    vals = [x for x in xs if x is not None]
    return round(statistics.median(vals), 2) if vals else None


def analyze(cgm: pd.DataFrame, bolus: pd.DataFrame, config: Config) -> CorrectionAnalysis:
    corrections = bolus[(bolus["kind"] == "correction") & (bolus["total_units"] > 0)]
    corrections = corrections.sort_values("time").reset_index(drop=True)
    lookup = CgmLookup(cgm)

    meal_times = bolus.loc[bolus["kind"] == "meal", "time"].to_numpy(dtype="datetime64[ns]")
    corr_times = corrections["time"].to_numpy(dtype="datetime64[ns]")
    gap = np.timedelta64(config.meal_clean_gap_min, "m")
    stack_gap = np.timedelta64(120, "m")

    features: list[CorrectionFeature] = []
    for i, row in corrections.iterrows():
        t = row["time"]
        tt = corr_times[i]
        units = float(row["total_units"])
        block = block_for_hour(t.hour, config.blocks).key
        start = lookup.nearest(t, tol_min=15)
        drop_window = lookup.window(t, _DROP_WINDOW_H)
        nadir = float(drop_window.min()) if len(drop_window) else None
        observed_drop = (start - nadir) if (start is not None and nadir is not None) else None
        drop_per_unit = (observed_drop / units) if (observed_drop is not None and units) else None

        confounds: list[str] = []
        if len(meal_times) and np.any(np.abs(meal_times - tt) <= gap):
            confounds.append("meal_nearby")
        others = np.delete(corr_times, i)
        if len(others) and np.any(np.abs(others - tt) <= stack_gap):
            confounds.append("stacked_correction")
        isolated = not confounds

        features.append(CorrectionFeature(
            time=t.isoformat(),
            block=block,
            units=round(units, 3),
            start_mgdl=start,
            nadir_mgdl=nadir,
            observed_drop=round(observed_drop, 1) if observed_drop is not None else None,
            drop_per_unit=round(drop_per_unit, 1) if drop_per_unit is not None else None,
            isolated=isolated,
            confounds=confounds,
        ))

    per_block: dict[str, CorrectionBlockStats] = {}
    for b in config.blocks:
        iso = [f for f in features if f.block == b.key and f.isolated]
        per_block[b.key] = CorrectionBlockStats(
            block=b.key,
            n_isolated=len(iso),
            median_drop_per_unit=_median([f.drop_per_unit for f in iso]),
        )
    overall = _median([f.drop_per_unit for f in features if f.isolated])
    return CorrectionAnalysis(corrections=features, per_block=per_block, overall_median_drop_per_unit=overall)


def run(state: PipelineState, config: Config) -> PipelineState:
    if state.dataset is None:
        raise ValueError("corrections stage requires state.dataset")
    state.corrections = analyze(state.dataset.cgm, state.dataset.bolus, config)
    return state

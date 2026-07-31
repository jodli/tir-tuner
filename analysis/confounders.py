"""Stage (confounders): non-CR patterns that would otherwise mislead the reasoning.

Currently detects the dawn phenomenon: a consistent pre-wake glucose rise that is
not driven by food/correction and is not a rebound from an overnight low. Flagging
it stops a dawn-driven breakfast high from being blamed on the breakfast carb
ratio. Derived only from overnight CGM + bolus timing. (This is where future
proxy detectors, e.g. probable-exercise, would also live.)
"""
from __future__ import annotations

import statistics
from typing import Optional

import pandas as pd

from .contracts import Config, ConfounderFlags, PipelineState


def _median(xs: list[float]) -> Optional[float]:
    return round(statistics.median(xs), 1) if xs else None


def analyze(cgm: pd.DataFrame, bolus: pd.DataFrame, config: Config) -> ConfounderFlags:
    if len(cgm) == 0:
        return ConfounderFlags(dawn_rise=False, dawn_magnitude=None, dawn_nights=0, dawn_nights_rising=0)

    bol_times = bolus.loc[bolus["total_units"] > 0, "time"] if len(bolus) else pd.Series([], dtype="datetime64[ns]")
    rises: list[float] = []          # rise on every qualifying night
    for date, _ in cgm.groupby(cgm["time"].dt.date):
        day = pd.Timestamp(date)
        dawn_start = day + pd.Timedelta(hours=config.dawn_start_h)
        dawn_end = day + pd.Timedelta(hours=config.dawn_end_h)

        # A bolus anywhere in the overnight-to-dawn span confounds the rise.
        if len(bol_times) and ((bol_times >= day) & (bol_times <= dawn_end)).any():
            continue
        # A preceding low means any rise is a rebound, not dawn.
        pre = cgm.loc[(cgm["time"] >= day) & (cgm["time"] < dawn_start), "mg_dl"]
        if len(pre) and float(pre.min()) < config.tir_low:
            continue

        window = cgm.loc[(cgm["time"] >= dawn_start) & (cgm["time"] <= dawn_end)].sort_values("time")
        if len(window) < 2:
            continue
        rises.append(float(window["mg_dl"].iloc[-1]) - float(window["mg_dl"].iloc[0]))

    n_nights = len(rises)
    rising = [r for r in rises if r >= config.dawn_min_rise_mgdl]
    dawn_rise = (n_nights >= config.dawn_min_nights
                 and len(rising) / n_nights >= config.dawn_min_fraction)
    return ConfounderFlags(
        dawn_rise=dawn_rise,
        dawn_magnitude=_median(rising),
        dawn_nights=n_nights,
        dawn_nights_rising=len(rising),
    )


def run(state: PipelineState, config: Config) -> PipelineState:
    if state.dataset is None:
        raise ValueError("confounders stage requires state.dataset")
    state.confounders = analyze(state.dataset.cgm, state.dataset.bolus, config)
    return state

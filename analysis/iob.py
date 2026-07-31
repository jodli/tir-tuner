"""Stage (iob): insulin-on-board and a delivery-robustness guard.

Two orthogonal signals, both derived only from pump logs + CGM:

* **iob_before** - residual *bolus* insulin still active when the next bolus is
  given, via a transparent linear-decay model ``u * max(0, 1 - dt/DIA)`` summed
  over earlier boluses. A meal started with substantial residual insulin has a
  contaminated effective-CR estimate; downstream stages use this to down-weight
  it. Basal is deliberately excluded: on a hybrid closed loop it is auto-modulated
  and hard to attribute.

* **suspected_no_delivery** - pump logs record *commanded* insulin, but
  pump-to-body connection issues (occlusion, cannula/site failure, disconnect)
  mean logged insulin sometimes never acts. A substantial bolus whose glucose
  rises and never descends from its peak is flagged as logged-but-not-absorbed,
  so a runaway meal is not mistaken for a too-weak carb ratio. This targets the
  runaway-rise pattern (the case that would otherwise drive an unsafe CR cut);
  ambiguous flat-high traces are left unflagged on purpose.

DIA comes from ``settings.insulin_action_hours`` (configured or its default), so
the model rests on user-supplied data, not a guess.
"""
from __future__ import annotations

from typing import Optional

import numpy as np
import pandas as pd

from ._cgm import CgmLookup
from .contracts import Config, EventIob, IobAnalysis, PipelineState


def _suspect_no_delivery(lookup: CgmLookup, t, units: float, config: Config) -> bool:
    if units < config.no_delivery_min_units:
        return False
    start = lookup.nearest(t, tol_min=15)
    _, series = lookup.window_tv(t, 0.0, config.excursion_tail_h)
    if start is None or len(series) == 0:
        return False
    end = float(series[-1])
    peak = float(series.max())
    net_rise = end - start
    descent_from_peak = peak - end
    # Ended well above the start AND never came down from the peak: consistent
    # with carbs/glucose acting with no insulin on board.
    return net_rise >= config.no_delivery_expected_drop and descent_from_peak < config.no_delivery_expected_drop


def analyze(bolus: pd.DataFrame, cgm: pd.DataFrame, dia_hours: Optional[float], config: Config) -> IobAnalysis:
    if dia_hours is None or dia_hours <= 0:
        return IobAnalysis(available=False, dia_hours=dia_hours, per_event=[])

    events = bolus[bolus["total_units"] > 0].sort_values("time").reset_index(drop=True)
    lookup = CgmLookup(cgm)
    times = events["time"].to_numpy(dtype="datetime64[ns]")
    units = events["total_units"].to_numpy(dtype=float)
    dia_min = dia_hours * 60.0

    per_event: list[EventIob] = []
    for i, row in events.iterrows():
        iob = 0.0
        for j in range(i):
            dt_min = float((times[i] - times[j]) / np.timedelta64(1, "m"))
            if 0.0 < dt_min < dia_min:
                iob += units[j] * (1.0 - dt_min / dia_min)
        per_event.append(EventIob(
            time=row["time"].isoformat(),
            kind=str(row["kind"]),
            units=round(float(row["total_units"]), 3),
            iob_before=round(iob, 2),
            suspected_no_delivery=_suspect_no_delivery(lookup, row["time"], float(row["total_units"]), config),
        ))
    return IobAnalysis(available=True, dia_hours=float(dia_hours), per_event=per_event)


def run(state: PipelineState, config: Config) -> PipelineState:
    if state.dataset is None:
        raise ValueError("iob stage requires state.dataset")
    dia = state.settings.insulin_action_hours if state.settings is not None else None
    if dia is None:
        dia = config.insulin_action_hours_default
    state.iob = analyze(state.dataset.bolus, state.dataset.cgm, dia, config)
    return state

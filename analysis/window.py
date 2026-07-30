"""Stage 2 (window): restrict every frame to a rolling N-week window.

The window ends on ``config.as_of`` (inclusive of that whole day) and spans
``config.weeks`` weeks back. ``as_of`` defaults to the date of the last CGM
reading, so a weekly run naturally analyses the most recent 4 weeks; it can be
overridden to re-run a historical window.
"""
from __future__ import annotations

import datetime as dt

import pandas as pd

from .contracts import Config, Dataset, PipelineState, WindowInfo


def _infer_as_of(ds: Dataset) -> dt.date:
    times = [f["time"].max() for f in (ds.cgm, ds.bolus, ds.basal, ds.manual_bg) if len(f)]
    times = [t for t in times if pd.notna(t)]
    if not times:
        raise ValueError("Cannot infer as_of: dataset has no timestamped rows. Pass config.as_of.")
    return max(times).date()


def _filter(df: pd.DataFrame, start: pd.Timestamp, end: pd.Timestamp) -> pd.DataFrame:
    if "time" not in df.columns or len(df) == 0:
        return df
    mask = (df["time"] >= start) & (df["time"] < end)
    return df[mask].reset_index(drop=True)


def apply_window(ds: Dataset, config: Config) -> tuple[Dataset, WindowInfo]:
    as_of = dt.date.fromisoformat(config.as_of) if config.as_of else _infer_as_of(ds)
    end = pd.Timestamp(as_of) + pd.Timedelta(days=1)          # exclusive; includes all of as_of
    start = end - pd.Timedelta(weeks=config.weeks)            # inclusive lower bound
    windowed = Dataset(
        cgm=_filter(ds.cgm, start, end),
        bolus=_filter(ds.bolus, start, end),
        basal=_filter(ds.basal, start, end),
        daily_totals=_filter(ds.daily_totals, start, end),
        manual_bg=_filter(ds.manual_bg, start, end),
        source_range=ds.source_range,
    )
    info = WindowInfo(
        as_of=as_of.isoformat(),
        weeks=config.weeks,
        start=start.isoformat(),
        end=end.isoformat(),
    )
    return windowed, info


def run(state: PipelineState, config: Config) -> PipelineState:
    if state.dataset is None:
        raise ValueError("window stage requires state.dataset (run load first)")
    state.dataset, state.window = apply_window(state.dataset, config)
    return state

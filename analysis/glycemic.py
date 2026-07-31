"""Stage 3 (glycemic): time-in-range and related CGM metrics.

Computes standard glycemic statistics over the windowed CGM series: overall,
per time block, and per day. All thresholds come from :class:`Config` so the
in-range definition is explicit and testable.
"""
from __future__ import annotations

import pandas as pd

from .contracts import (
    Config,
    DayStat,
    GlycemicBand,
    GlycemicMetrics,
    PipelineState,
    TimeBlock,
    block_for_hour,
)


def _pct(mask: pd.Series) -> float:
    return round(100.0 * float(mask.mean()), 2)


def _band(values: pd.Series, config: Config) -> GlycemicBand:
    values = values.dropna()
    n = int(len(values))
    if n == 0:
        return GlycemicBand(None, None, None, None, None, None, None, None, 0)
    mean = float(values.mean())
    # CV uses sample SD (ddof=1); undefined for a single reading.
    cv = round(100.0 * float(values.std(ddof=1)) / mean, 2) if n > 1 and mean else None
    gmi = round(3.31 + 0.02392 * mean, 2)   # Bergenstal GMI for mg/dl
    return GlycemicBand(
        tir=_pct((values >= config.tir_low) & (values <= config.tir_high)),
        tbr_70=_pct(values < config.tir_low),
        tbr_54=_pct(values < config.vlow),
        tar_180=_pct(values > config.tir_high),
        tar_250=_pct(values > config.high),
        mean=round(mean, 1),
        cv=cv,
        gmi=gmi,
        n_readings=n,
        titr=_pct((values >= config.tir_low) & (values <= config.tir_tight_high)),
    )


def _coverage_pct(n_readings: int, span_days: int, hours: float) -> Optional[float]:
    """Actual readings vs. the ~1/min a Libre 3 would produce over the span.

    ``span_days`` is the calendar span present in the window, so whole missing
    days lower coverage. Capped at 100; ``None`` when the span is empty.
    """
    expected = span_days * hours * 60.0
    if expected <= 0:
        return None
    return round(min(100.0, 100.0 * n_readings / expected), 1)


def _hour_to_block_key(blocks: list[TimeBlock]) -> dict[int, str]:
    return {h: block_for_hour(h, blocks).key for h in range(24)}


def compute(cgm: pd.DataFrame, config: Config) -> GlycemicMetrics:
    overall = _band(cgm["mg_dl"] if len(cgm) else pd.Series(dtype="float64"), config)

    span_days = 0
    if len(cgm):
        dates = cgm["time"].dt.date
        span_days = (dates.max() - dates.min()).days + 1
    overall.coverage_pct = _coverage_pct(overall.n_readings, span_days, 24)

    per_block: dict[str, GlycemicBand] = {}
    hour_key = _hour_to_block_key(config.blocks)
    if len(cgm):
        keys = cgm["time"].dt.hour.map(hour_key)
        for b in config.blocks:
            per_block[b.key] = _band(cgm.loc[keys == b.key, "mg_dl"], config)
    else:
        for b in config.blocks:
            per_block[b.key] = _band(pd.Series(dtype="float64"), config)
    for b in config.blocks:
        per_block[b.key].coverage_pct = _coverage_pct(
            per_block[b.key].n_readings, span_days, b.end_hour - b.start_hour)

    per_day: list[DayStat] = []
    if len(cgm):
        for day, grp in cgm.groupby(cgm["time"].dt.date):
            band = _band(grp["mg_dl"], config)
            per_day.append(DayStat(date=day.isoformat(), tir=band.tir, mean=band.mean, n_readings=band.n_readings))
    per_day.sort(key=lambda d: d.date)

    return GlycemicMetrics(overall=overall, per_block=per_block, per_day=per_day)


def run(state: PipelineState, config: Config) -> PipelineState:
    if state.dataset is None:
        raise ValueError("glycemic stage requires state.dataset")
    state.glycemic = compute(state.dataset.cgm, config)
    return state

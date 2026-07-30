"""Stage 1 (load): parse the de-identified Glooko CSVs into a typed Dataset.

Glooko CSVs are German-locale: a banner row (``Name:...,Datumsbereich:...``),
then a header row, then data. Values use decimal commas and are quoted
(``"77,0"``); timestamps are ``dd.mm.YYYY HH:MM``. Everything is read as text and
converted explicitly here so locale handling is in one place.

This is an *edge* stage: it reads files. Downstream stages operate purely on the
returned :class:`~analysis.contracts.Dataset`.
"""
from __future__ import annotations

import os

import pandas as pd

from .contracts import Config, Dataset, PipelineState

# German header names (Glooko export is stable across exports).
C_TIME = "Zeitstempel"
C_CGM = "CGM-Glukosewert (mg/dl)"
C_BOLUS_TYPE = "Insulin-Typ"
C_BG_ENTRY = "Blutzuckereingabe (mg/dl)"
C_CARBS = "Kohlenhydrataufnahme (g)"
C_DELIVERED = "Abgegebenes Insulin (E)"
C_INITIAL = "Anfängliche Abgabe (E)"
C_DELAYED = "Verzögerte Abgabe (E)"
C_DURATION = "Dauer (Minuten)"
C_RATE = "Rate"
C_BOLUS_TOTAL = "Bolus gesamt (U)"
C_INSULIN_TOTAL = "Insulin gesamt (U)"
C_BASAL_TOTAL = "Basal gesamt (U)"
C_BG_VALUE = "Glukosewert (mg/dl)"

_TS_FORMAT = "%d.%m.%Y %H:%M"


def _to_float(series: pd.Series) -> pd.Series:
    """German decimal text -> float. Empty and ``-`` become NaN."""
    cleaned = (
        series.astype(str)
        .str.strip()
        .replace({"": None, "-": None})
        .str.replace(",", ".", regex=False)
    )
    return pd.to_numeric(cleaned, errors="coerce")


def _to_time(series: pd.Series) -> pd.Series:
    return pd.to_datetime(series.astype(str).str.strip(), format=_TS_FORMAT, errors="coerce")


def _read(path: str) -> tuple[str, pd.DataFrame | None]:
    """Return (banner line, dataframe). Missing file -> ("", None)."""
    if not os.path.exists(path):
        return "", None
    with open(path, encoding="utf-8-sig") as f:
        banner = f.readline().rstrip("\r\n")
    df = pd.read_csv(path, skiprows=1, dtype=str, keep_default_na=False, encoding="utf-8-sig")
    return banner, df


def _source_range(banner: str) -> str:
    marker = "Datumsbereich:"
    if marker in banner:
        return banner.split(marker, 1)[1].strip()
    return ""


def _empty(cols: dict[str, str]) -> pd.DataFrame:
    return pd.DataFrame({c: pd.Series(dtype=t) for c, t in cols.items()})


def _finish(df: pd.DataFrame) -> pd.DataFrame:
    """Drop rows without a valid timestamp; sort ascending by time."""
    return df.dropna(subset=["time"]).sort_values("time").reset_index(drop=True)


def _load_cgm(data_dir: str) -> pd.DataFrame:
    cols = {"time": "datetime64[ns]", "mg_dl": "float64"}
    _, df = _read(os.path.join(data_dir, "cgm_data_1.csv"))
    if df is None or df.empty:
        return _empty(cols)
    out = pd.DataFrame({"time": _to_time(df[C_TIME]), "mg_dl": _to_float(df[C_CGM])})
    return _finish(out)


def _load_bolus(data_dir: str) -> pd.DataFrame:
    cols = {
        "time": "datetime64[ns]", "kind": "object", "carbs": "float64",
        "total_units": "float64", "delivered_u": "float64",
        "initial_u": "float64", "delayed_u": "float64", "bg_entry": "float64",
    }
    _, df = _read(os.path.join(data_dir, "Insulin data", "bolus_data_1.csv"))
    if df is None or df.empty:
        return _empty(cols)
    carbs = _to_float(df[C_CARBS]).fillna(0.0)
    delivered = _to_float(df[C_DELIVERED])
    initial = _to_float(df[C_INITIAL])
    delayed = _to_float(df[C_DELAYED])
    # `delivered` is the TOTAL for the bolus; initial/delayed are its split for
    # extended boluses. Never sum all three (double count). Fall back to the
    # split only when the total is missing.
    total_units = delivered.where(delivered.notna(), initial.fillna(0.0) + delayed.fillna(0.0))
    out = pd.DataFrame({
        "time": _to_time(df[C_TIME]),
        "kind": pd.Series(["meal" if c > 0 else "correction" for c in carbs], dtype="object"),
        "carbs": carbs,
        "total_units": total_units,
        "delivered_u": delivered,
        "initial_u": initial,
        "delayed_u": delayed,
        "bg_entry": _to_float(df[C_BG_ENTRY]),
    })
    return _finish(out)


def _load_basal(data_dir: str) -> pd.DataFrame:
    cols = {
        "time": "datetime64[ns]", "duration_min": "float64",
        "rate": "float64", "delivered_u": "float64",
    }
    _, df = _read(os.path.join(data_dir, "Insulin data", "basal_data_1.csv"))
    if df is None or df.empty:
        return _empty(cols)
    duration = _to_float(df[C_DURATION])
    rate = _to_float(df[C_RATE])
    reported = _to_float(df[C_DELIVERED]) if C_DELIVERED in df.columns else pd.Series([None] * len(df))
    computed = rate * duration / 60.0
    out = pd.DataFrame({
        "time": _to_time(df[C_TIME]),
        "duration_min": duration,
        "rate": rate,
        "delivered_u": reported.where(reported.notna(), computed),
    })
    return _finish(out)


def _load_daily_totals(data_dir: str) -> pd.DataFrame:
    cols = {
        "time": "datetime64[ns]", "bolus_total": "float64",
        "insulin_total": "float64", "basal_total": "float64",
    }
    _, df = _read(os.path.join(data_dir, "Insulin data", "insulin_data_1.csv"))
    if df is None or df.empty:
        return _empty(cols)
    out = pd.DataFrame({
        "time": _to_time(df[C_TIME]),
        "bolus_total": _to_float(df[C_BOLUS_TOTAL]),
        "insulin_total": _to_float(df[C_INSULIN_TOTAL]),
        "basal_total": _to_float(df[C_BASAL_TOTAL]),
    })
    return _finish(out)


def _load_manual_bg(data_dir: str) -> pd.DataFrame:
    cols = {"time": "datetime64[ns]", "mg_dl": "float64"}
    _, df = _read(os.path.join(data_dir, "bg_data_1.csv"))
    if df is None or df.empty:
        return _empty(cols)
    out = pd.DataFrame({"time": _to_time(df[C_TIME]), "mg_dl": _to_float(df[C_BG_VALUE])})
    return _finish(out)


def load_dataset(config: Config) -> Dataset:
    """Read every relevant CSV under ``config.data_dir`` into a Dataset."""
    data_dir = config.data_dir
    banner, _ = _read(os.path.join(data_dir, "cgm_data_1.csv"))
    return Dataset(
        cgm=_load_cgm(data_dir),
        bolus=_load_bolus(data_dir),
        basal=_load_basal(data_dir),
        daily_totals=_load_daily_totals(data_dir),
        manual_bg=_load_manual_bg(data_dir),
        source_range=_source_range(banner),
    )


def run(state: PipelineState, config: Config) -> PipelineState:
    """Stage entry point: populate ``state.dataset`` from the CSV export."""
    state.dataset = load_dataset(config)
    return state

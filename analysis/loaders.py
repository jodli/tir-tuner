"""Stage 1 (load): parse the de-identified Glooko CSVs into a typed Dataset.

Glooko CSVs are German-locale: a banner row (``Name:...,Datumsbereich:...``),
then a header row, then data. Values use decimal commas and are quoted
(``"77,0"``); timestamps are ``dd.mm.YYYY HH:MM``. Everything is read as text and
converted explicitly here so locale handling is in one place.

Two properties of the export drive the file discovery below:

* **Glooko splits large tables.** A table is capped at 20000 data rows and spills
  into ``<name>_2.csv``, ``_3.csv``, ... A 14-day CGM export is 20160 minutes, so
  *every* two-week export spills. Reading only ``cgm_data_1.csv`` silently drops
  the oldest hours of the window.
* **The intended cadence is a weekly two-week export**, so consecutive exports
  overlap by a week. All of them are read and merged, and rows carried by more
  than one export are de-duplicated, which is what makes a 4-week window possible
  from two-week exports.

Discovery is therefore a recursive glob over ``config.data_dir``: both a single
export directory and a directory of per-export subdirectories work.

This is an *edge* stage: it reads files. Downstream stages operate purely on the
returned :class:`~analysis.contracts.Dataset`.
"""
from __future__ import annotations

import datetime as dt
import glob
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

# Filename patterns per table. The trailing ``_*`` catches Glooko's row-cap
# split files (``cgm_data_1.csv``, ``cgm_data_2.csv``, ...).
PATTERNS = {
    "cgm": "cgm_data_*.csv",
    "bolus": "bolus_data_*.csv",
    "basal": "basal_data_*.csv",
    "daily_totals": "insulin_data_*.csv",
    "manual_bg": "bg_data_*.csv",
}


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


def discover(data_dir: str, pattern: str) -> list[str]:
    """Every file matching ``pattern`` at any depth under ``data_dir``, sorted.

    Sorted order matters: de-duplication keeps the *last* occurrence, so with
    date-named export directories the newest export wins on conflicts.
    """
    hits = glob.glob(os.path.join(data_dir, "**", pattern), recursive=True)
    return sorted(p for p in hits if os.path.isfile(p))


def _read_group(paths: list[str]) -> tuple[list[str], pd.DataFrame | None]:
    """Read and concatenate every file of one table. Returns (banners, frame)."""
    banners: list[str] = []
    frames: list[pd.DataFrame] = []
    for p in paths:
        banner, df = _read(p)
        if banner:
            banners.append(banner)
        if df is not None and not df.empty:
            frames.append(df)
    if not frames:
        return banners, None
    return banners, pd.concat(frames, ignore_index=True)


def _source_range(banners: list[str]) -> str:
    """Combined ``Datumsbereich`` over all banners.

    A single export keeps its banner range verbatim; several exports collapse to
    the spanned range so the report can state what was actually merged.
    """
    marker = "Datumsbereich:"
    ranges = []
    for banner in banners:
        if marker in banner:
            value = banner.split(marker, 1)[1].strip()
            if value:
                ranges.append(value)
    unique = sorted(set(ranges))
    if not unique:
        return ""
    if len(unique) == 1:
        return unique[0]

    def parse(text: str) -> list[dt.date]:
        out = []
        for part in text.split("-"):
            try:
                out.append(dt.datetime.strptime(part.strip(), "%d.%m.%Y").date())
            except ValueError:
                continue
        return out

    dates = [d for text in unique for d in parse(text)]
    if not dates:
        return "; ".join(unique)
    return (f"{min(dates).strftime('%d.%m.%Y')} - {max(dates).strftime('%d.%m.%Y')} "
            f"({len(unique)} Exporte)")


def _empty(cols: dict[str, str]) -> pd.DataFrame:
    return pd.DataFrame({c: pd.Series(dtype=t) for c, t in cols.items()})


def _finish(df: pd.DataFrame, dedupe_on: list[str] | None = None) -> tuple[pd.DataFrame, int]:
    """Drop rows without a timestamp, de-duplicate, sort ascending by time.

    ``dedupe_on`` defaults to ``["time"]``: one row per timestamp, the last file
    (newest export) winning. Bolus data passes the full row instead, because two
    distinct boluses can legitimately share a minute.
    """
    df = df.dropna(subset=["time"])
    before = len(df)
    # mergesort is stable, so equal timestamps keep file order and keep="last"
    # really means "from the newest export".
    df = df.sort_values("time", kind="mergesort")
    df = df.drop_duplicates(subset=dedupe_on or ["time"], keep="last")
    return df.reset_index(drop=True), before - len(df)


def _load_cgm(df: pd.DataFrame | None) -> tuple[pd.DataFrame, int]:
    cols = {"time": "datetime64[ns]", "mg_dl": "float64"}
    if df is None or df.empty:
        return _empty(cols), 0
    out = pd.DataFrame({"time": _to_time(df[C_TIME]), "mg_dl": _to_float(df[C_CGM])})
    return _finish(out)


def _load_bolus(df: pd.DataFrame | None) -> tuple[pd.DataFrame, int]:
    cols = {
        "time": "datetime64[ns]", "kind": "object", "carbs": "float64",
        "total_units": "float64", "delivered_u": "float64",
        "initial_u": "float64", "delayed_u": "float64", "bg_entry": "float64",
    }
    if df is None or df.empty:
        return _empty(cols), 0
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
    # Two boluses can share a minute (meal + correction), so identity is the whole
    # row, not just the timestamp.
    return _finish(out, dedupe_on=list(out.columns))


def _load_basal(df: pd.DataFrame | None) -> tuple[pd.DataFrame, int]:
    cols = {
        "time": "datetime64[ns]", "duration_min": "float64",
        "rate": "float64", "delivered_u": "float64",
    }
    if df is None or df.empty:
        return _empty(cols), 0
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


def _load_daily_totals(df: pd.DataFrame | None) -> tuple[pd.DataFrame, int]:
    cols = {
        "time": "datetime64[ns]", "bolus_total": "float64",
        "insulin_total": "float64", "basal_total": "float64",
    }
    if df is None or df.empty:
        return _empty(cols), 0
    out = pd.DataFrame({
        "time": _to_time(df[C_TIME]),
        "bolus_total": _to_float(df[C_BOLUS_TOTAL]),
        "insulin_total": _to_float(df[C_INSULIN_TOTAL]),
        "basal_total": _to_float(df[C_BASAL_TOTAL]),
    })
    return _finish(out)


def _load_manual_bg(df: pd.DataFrame | None) -> tuple[pd.DataFrame, int]:
    cols = {"time": "datetime64[ns]", "mg_dl": "float64"}
    if df is None or df.empty:
        return _empty(cols), 0
    out = pd.DataFrame({"time": _to_time(df[C_TIME]), "mg_dl": _to_float(df[C_BG_VALUE])})
    return _finish(out)


def load_dataset(config: Config) -> Dataset:
    """Read every relevant CSV under ``config.data_dir`` into one merged Dataset."""
    data_dir = config.data_dir
    paths = {kind: discover(data_dir, pattern) for kind, pattern in PATTERNS.items()}
    groups = {kind: _read_group(p) for kind, p in paths.items()}

    cgm, dup_cgm = _load_cgm(groups["cgm"][1])
    bolus, dup_bolus = _load_bolus(groups["bolus"][1])
    basal, dup_basal = _load_basal(groups["basal"][1])
    totals, dup_totals = _load_daily_totals(groups["daily_totals"][1])
    manual_bg, dup_bg = _load_manual_bg(groups["manual_bg"][1])

    all_files = sorted(os.path.relpath(p, data_dir) for group in paths.values() for p in group)
    return Dataset(
        cgm=cgm,
        bolus=bolus,
        basal=basal,
        daily_totals=totals,
        manual_bg=manual_bg,
        source_range=_source_range(groups["cgm"][0] or groups["bolus"][0]),
        source_files=all_files,
        n_duplicate_rows=dup_cgm + dup_bolus + dup_basal + dup_totals + dup_bg,
    )


def run(state: PipelineState, config: Config) -> PipelineState:
    """Stage entry point: populate ``state.dataset`` from the CSV export(s)."""
    state.dataset = load_dataset(config)
    return state

import math

import pandas as pd

from analysis import loaders
from analysis.contracts import Config


def test_source_range_from_banner(sample_config):
    ds = loaders.load_dataset(sample_config)
    assert ds.source_range == "29.07.2026 - 30.07.2026"


def test_cgm_parsed_and_sorted(sample_config):
    ds = loaders.load_dataset(sample_config)
    assert list(ds.cgm.columns) == ["time", "mg_dl"]
    assert len(ds.cgm) == 3
    # German decimals parsed; rows sorted ascending despite fixture order.
    assert list(ds.cgm["mg_dl"]) == [118.0, 120.0, 122.0]
    assert ds.cgm["time"].is_monotonic_increasing
    assert ds.cgm["time"].iloc[0] == pd.Timestamp("2026-07-30 11:59")


def test_bolus_classification_and_units(sample_config):
    ds = loaders.load_dataset(sample_config)
    assert list(ds.bolus["kind"]) == ["meal", "meal", "correction"]
    assert list(ds.bolus["carbs"]) == [45.0, 30.0, 0.0]

    normal, extended, correction = (ds.bolus.iloc[i] for i in range(3))
    # Normal meal: delivered is the total.
    assert normal["total_units"] == 4.1
    # Extended bolus: delivered blank -> initial + delayed (NOT summed with delivered).
    assert math.isnan(extended["delivered_u"])
    assert extended["total_units"] == 3.0
    # Correction: carbs 0.
    assert correction["total_units"] == 1.6


def test_basal_delivered_computed_from_rate_and_duration(sample_config):
    ds = loaders.load_dataset(sample_config)
    # 3.0 U/h * 10 min = 0.5 U ; 0.6 U/h * 20 min = 0.2 U
    delivered = sorted(round(x, 4) for x in ds.basal["delivered_u"])
    assert delivered == [0.2, 0.5]


def test_daily_totals_and_manual_bg(sample_config):
    ds = loaders.load_dataset(sample_config)
    assert ds.daily_totals["insulin_total"].iloc[0] == 36.11
    assert ds.daily_totals["basal_total"].iloc[0] == 21.11
    assert ds.manual_bg["mg_dl"].iloc[0] == 244.0


def test_empty_and_missing_files_yield_typed_empty_frames(empty_dir):
    # CGM file has header only; all other files are absent.
    ds = loaders.load_dataset(Config(data_dir=empty_dir))
    for frame in (ds.cgm, ds.bolus, ds.basal, ds.daily_totals, ds.manual_bg):
        assert len(frame) == 0
    # Columns still present so downstream code is safe.
    assert list(ds.cgm.columns) == ["time", "mg_dl"]
    assert "total_units" in ds.bolus.columns
    assert "delivered_u" in ds.basal.columns


def test_dataset_round_trips_through_json(sample_config):
    ds = loaders.load_dataset(sample_config)
    from analysis.contracts import Dataset

    back = Dataset.from_json(ds.to_json())
    assert len(back.cgm) == len(ds.cgm)
    assert pd.api.types.is_datetime64_any_dtype(back.bolus["time"])
    assert list(back.bolus["kind"]) == ["meal", "meal", "correction"]

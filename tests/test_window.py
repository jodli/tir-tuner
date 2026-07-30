import pandas as pd

from analysis.contracts import Config
from analysis.window import apply_window
from conftest import cgm_df, make_dataset


def test_window_boundaries_inclusive_start_exclusive_end():
    ds = make_dataset(cgm_df([
        ("2026-07-02 12:00", 100),   # before window -> excluded
        ("2026-07-03 00:00", 101),   # == start -> included
        ("2026-07-15 12:00", 102),   # inside -> included
        ("2026-07-30 23:00", 103),   # last day (as_of) -> included
        ("2026-07-31 00:00", 104),   # == end -> excluded
    ]))
    windowed, info = apply_window(ds, Config(as_of="2026-07-30", weeks=4))
    assert list(windowed.cgm["mg_dl"]) == [101.0, 102.0, 103.0]
    assert info.as_of == "2026-07-30"
    assert info.start == pd.Timestamp("2026-07-03 00:00").isoformat()
    assert info.end == pd.Timestamp("2026-07-31 00:00").isoformat()


def test_as_of_defaults_to_last_cgm_day():
    ds = make_dataset(cgm_df([
        ("2026-07-20 08:00", 100),
        ("2026-07-30 21:37", 110),
    ]))
    _, info = apply_window(ds, Config(as_of=None, weeks=4))
    assert info.as_of == "2026-07-30"


def test_empty_dataset_without_as_of_raises():
    import pytest

    ds = make_dataset()
    with pytest.raises(ValueError):
        apply_window(ds, Config(as_of=None))

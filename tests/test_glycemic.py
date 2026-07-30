import numpy as np
import pytest

from analysis.contracts import Config
from analysis.glycemic import compute
from conftest import cgm_df


def test_overall_band_hand_computed():
    # 7 readings spanning low/in-range/high buckets, all in the breakfast block.
    values = [50, 60, 70, 100, 180, 200, 260]
    pairs = [(f"2026-07-30 08:0{i}", v) for i, v in enumerate(values)]
    m = compute(cgm_df(pairs), Config())
    o = m.overall
    assert o.n_readings == 7
    assert o.tir == pytest.approx(100 * 3 / 7, abs=0.01)      # 70,100,180
    assert o.tbr_70 == pytest.approx(100 * 2 / 7, abs=0.01)   # 50,60
    assert o.tbr_54 == pytest.approx(100 * 1 / 7, abs=0.01)   # 50
    assert o.tar_180 == pytest.approx(100 * 2 / 7, abs=0.01)  # 200,260
    assert o.tar_250 == pytest.approx(100 * 1 / 7, abs=0.01)  # 260
    assert o.mean == pytest.approx(np.mean(values), abs=0.05)
    assert o.gmi == pytest.approx(3.31 + 0.02392 * np.mean(values), abs=0.01)
    expected_cv = 100 * np.std(values, ddof=1) / np.mean(values)
    assert o.cv == pytest.approx(expected_cv, abs=0.05)


def test_per_block_assignment():
    pairs = [
        ("2026-07-30 02:00", 90),    # night
        ("2026-07-30 08:00", 100),   # breakfast
        ("2026-07-30 08:30", 200),   # breakfast
    ]
    m = compute(cgm_df(pairs), Config())
    assert m.per_block["00-06"].n_readings == 1
    assert m.per_block["06-11"].n_readings == 2
    assert m.per_block["11-15"].n_readings == 0
    assert m.per_block["06-11"].mean == pytest.approx(150.0)


def test_per_day_grouping():
    pairs = [
        ("2026-07-29 08:00", 100),
        ("2026-07-29 09:00", 120),
        ("2026-07-30 08:00", 200),
    ]
    m = compute(cgm_df(pairs), Config())
    days = {d.date: d for d in m.per_day}
    assert set(days) == {"2026-07-29", "2026-07-30"}
    assert days["2026-07-29"].n_readings == 2
    assert days["2026-07-30"].mean == pytest.approx(200.0)


def test_empty_series_is_safe():
    m = compute(cgm_df([]), Config())
    assert m.overall.n_readings == 0
    assert m.overall.tir is None
    assert all(b.n_readings == 0 for b in m.per_block.values())
    assert m.per_day == []

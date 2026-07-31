import pytest

from analysis import meals
from analysis.contracts import Config
from conftest import cgm_df, bolus_df


def _meal(time, carbs, units):
    return {"time": time, "kind": "meal", "carbs": carbs, "total_units": units,
            "delivered_u": units, "initial_u": None, "delayed_u": None, "bg_entry": 0.0}


def test_meal_excursion_features():
    cgm = cgm_df([
        ("2026-07-30 12:00", 120),   # baseline
        ("2026-07-30 12:30", 170),
        ("2026-07-30 13:00", 220),   # peak
        ("2026-07-30 13:30", 200),
        ("2026-07-30 14:00", 170),
        ("2026-07-30 14:30", 150),
        ("2026-07-30 15:00", 140),   # +3h
        ("2026-07-30 15:30", 135),
        ("2026-07-30 16:00", 130),   # +4h
    ])
    bolus = bolus_df([_meal("2026-07-30 12:00", 60, 6)])
    res = meals.analyze(cgm, bolus, Config())
    assert len(res.meals) == 1
    f = res.meals[0]
    assert f.block == "11-15"
    assert f.effective_cr == 10.0
    assert f.baseline_mgdl == 120.0
    assert f.peak_rise == 100.0
    assert f.delta_3h == 20.0
    assert f.ended_in_range is True
    assert f.post_meal_hypo is False
    assert f.clean is True


def test_post_meal_hypo_detected():
    cgm = cgm_df([
        ("2026-07-30 12:00", 120),
        ("2026-07-30 13:00", 90),
        ("2026-07-30 14:00", 62),    # dips below 70
        ("2026-07-30 15:00", 80),
    ])
    bolus = bolus_df([_meal("2026-07-30 12:00", 60, 9)])
    f = meals.analyze(cgm, bolus, Config()).meals[0]
    assert f.post_meal_hypo is True


def test_clean_detection_by_gap():
    # 30 min apart -> both dirty
    close = meals.analyze(cgm_df([]), bolus_df([
        _meal("2026-07-30 12:00", 60, 6), _meal("2026-07-30 12:30", 40, 4)]), Config())
    assert [m.clean for m in close.meals] == [False, False]
    # >3h apart -> both clean
    far = meals.analyze(cgm_df([]), bolus_df([
        _meal("2026-07-30 08:00", 60, 6), _meal("2026-07-30 13:00", 40, 4)]), Config())
    assert [m.clean for m in far.meals] == [True, True]


def test_per_block_median_effective_cr():
    bolus = bolus_df([
        _meal("2026-07-30 11:15", 60, 6),    # cr 10, lunch
        _meal("2026-07-30 14:45", 60, 5),    # cr 12, lunch, 3.5h later -> clean
    ])
    res = meals.analyze(cgm_df([]), bolus, Config())
    lunch = res.per_block["11-15"]
    assert lunch.n_clean == 2
    assert lunch.median_effective_cr == 11.0


def test_time_to_peak_and_auc():
    cgm = cgm_df([
        ("2026-07-30 12:00", 120),   # baseline
        ("2026-07-30 12:30", 170),
        ("2026-07-30 13:00", 220),   # peak, +60 min
        ("2026-07-30 13:30", 200),
        ("2026-07-30 14:00", 170),
        ("2026-07-30 15:00", 140),
        ("2026-07-30 16:00", 130),
    ])
    f = meals.analyze(cgm, bolus_df([_meal("2026-07-30 12:00", 60, 6)]), Config()).meals[0]
    assert f.time_to_peak_min == 60.0
    assert f.auc_over_baseline is not None and f.auc_over_baseline > 0
    assert f.undershoot_depth is None        # no post-meal hypo
    assert f.rebound is None


def test_undershoot_depth_and_rebound():
    cgm = cgm_df([
        ("2026-07-30 12:00", 120),
        ("2026-07-30 13:00", 90),
        ("2026-07-30 14:00", 62),    # below 70, inside the 1.5-4h hypo window
        ("2026-07-30 15:00", 80),    # recovery
    ])
    f = meals.analyze(cgm, bolus_df([_meal("2026-07-30 12:00", 60, 9)]), Config()).meals[0]
    assert f.post_meal_hypo is True
    assert f.undershoot_depth == 8.0     # 70 - 62
    assert f.rebound == 18.0             # 80 - 62


def test_effective_cr_trim_and_distribution():
    # One clean meal per day (>3h from any other) all land in the lunch block.
    bolus = bolus_df([
        _meal("2026-07-27 12:00", 60, 8),      # cr 7.5
        _meal("2026-07-28 12:00", 60, 6),      # cr 10
        _meal("2026-07-29 12:00", 60, 5),      # cr 12
        _meal("2026-07-30 12:00", 60, 0.6),    # cr 100 -> implausible, trimmed
    ])
    lunch = meals.analyze(cgm_df([]), bolus, Config()).per_block["11-15"]
    assert lunch.n_clean == 4
    assert lunch.n_trimmed == 1
    assert lunch.median_effective_cr == 10.0          # median of [7.5, 10, 12]
    assert lunch.effective_cr_min == 7.5
    assert lunch.effective_cr_max == 12.0
    assert lunch.effective_cr_q25 is not None and lunch.effective_cr_q75 is not None


def test_effective_cr_segmented_by_start_glucose():
    cgm = cgm_df([("2026-07-29 12:00", 100), ("2026-07-30 12:00", 250)])
    bolus = bolus_df([
        _meal("2026-07-29 12:00", 60, 6),      # cr 10, in-range start
        _meal("2026-07-30 12:00", 60, 5),      # cr 12, high start (folded correction)
    ])
    lunch = meals.analyze(cgm, bolus, Config()).per_block["11-15"]
    assert lunch.n_inrange_start == 1
    assert lunch.median_effective_cr_inrange_start == 10.0
    assert lunch.n_high_start == 1
    assert lunch.median_effective_cr_high_start == 12.0

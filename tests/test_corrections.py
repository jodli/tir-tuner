from analysis import corrections
from analysis.contracts import Config
from conftest import cgm_df, bolus_df


def _corr(time, units):
    return {"time": time, "kind": "correction", "carbs": 0.0, "total_units": units,
            "delivered_u": units, "initial_u": None, "delayed_u": None, "bg_entry": 0.0}


def _meal(time, carbs, units):
    return {"time": time, "kind": "meal", "carbs": carbs, "total_units": units,
            "delivered_u": units, "initial_u": None, "delayed_u": None, "bg_entry": 0.0}


def test_isolated_correction_drop_per_unit():
    cgm = cgm_df([
        ("2026-07-30 20:00", 200),   # start
        ("2026-07-30 20:30", 170),
        ("2026-07-30 21:00", 140),
        ("2026-07-30 21:30", 120),   # nadir within 3h
    ])
    res = corrections.analyze(cgm, bolus_df([_corr("2026-07-30 20:00", 2)]), Config())
    f = res.corrections[0]
    assert f.start_mgdl == 200.0
    assert f.nadir_mgdl == 120.0
    assert f.observed_drop == 80.0
    assert f.drop_per_unit == 40.0
    assert f.isolated is True
    assert f.confounds == []
    assert res.overall_median_drop_per_unit == 40.0


def test_meal_nearby_marks_confound_and_excludes_from_aggregate():
    cgm = cgm_df([
        ("2026-07-30 20:00", 200),
        ("2026-07-30 21:00", 150),
    ])
    bolus = bolus_df([
        _corr("2026-07-30 20:00", 2),
        _meal("2026-07-30 20:30", 40, 4),   # within 3h gap -> confounds the correction
    ])
    res = corrections.analyze(cgm, bolus, Config())
    corr = res.corrections[0]
    assert "meal_nearby" in corr.confounds
    assert corr.isolated is False
    # No isolated corrections -> aggregate is None.
    assert res.overall_median_drop_per_unit is None

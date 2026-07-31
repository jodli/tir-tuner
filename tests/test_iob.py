from analysis import iob
from analysis.contracts import Config
from conftest import bolus_df, cgm_df


def _meal(time, carbs, units):
    return {"time": time, "kind": "meal", "carbs": carbs, "total_units": units,
            "delivered_u": units, "initial_u": None, "delayed_u": None, "bg_entry": 0.0}


def _corr(time, units):
    return {"time": time, "kind": "correction", "carbs": 0.0, "total_units": units,
            "delivered_u": units, "initial_u": None, "delayed_u": None, "bg_entry": 0.0}


def _by_time(res):
    return {e.time: e for e in res.per_event}


def test_iob_linear_decay_and_stacking():
    # DIA 2h. Bolus of 4u at 12:00; a second bolus 1h later sees half of it (2u).
    bolus = bolus_df([_meal("2026-07-30 12:00", 40, 4), _corr("2026-07-30 13:00", 2)])
    res = iob.analyze(bolus, cgm_df([]), dia_hours=2.0, config=Config())
    ev = _by_time(res)
    assert res.available is True and res.dia_hours == 2.0
    assert ev["2026-07-30T12:00:00"].iob_before == 0.0          # nothing before it
    assert ev["2026-07-30T13:00:00"].iob_before == 2.0          # 4u * (1 - 60/120)


def test_iob_unavailable_without_dia():
    res = iob.analyze(bolus_df([_meal("2026-07-30 12:00", 40, 4)]), cgm_df([]),
                      dia_hours=None, config=Config())
    assert res.available is False
    assert res.per_event == []


def test_iob_run_falls_back_to_default_dia():
    # No settings on state -> stage uses Config.insulin_action_hours_default (2.0).
    from analysis.contracts import PipelineState
    from conftest import make_dataset
    st = PipelineState(dataset=make_dataset(bolus=bolus_df([_meal("2026-07-30 12:00", 40, 4)])))
    st = iob.run(st, Config())
    assert st.iob.available is True
    assert st.iob.dia_hours == 2.0


def test_suspected_no_delivery_fires_on_runaway_rise():
    # 6u meal but glucose climbs and never comes back down: logged-but-not-absorbed.
    cgm = cgm_df([
        ("2026-07-30 12:00", 120), ("2026-07-30 13:00", 190),
        ("2026-07-30 14:00", 250), ("2026-07-30 15:00", 300),
    ])
    res = iob.analyze(bolus_df([_meal("2026-07-30 12:00", 60, 6)]), cgm, 2.0, Config())
    assert res.per_event[0].suspected_no_delivery is True


def test_suspected_no_delivery_stays_off_for_controlled_meal():
    # Normal excursion that peaks then returns toward baseline: not flagged.
    cgm = cgm_df([
        ("2026-07-30 12:00", 120), ("2026-07-30 13:00", 180),
        ("2026-07-30 14:00", 150), ("2026-07-30 15:00", 125),
    ])
    res = iob.analyze(bolus_df([_meal("2026-07-30 12:00", 60, 6)]), cgm, 2.0, Config())
    assert res.per_event[0].suspected_no_delivery is False


def test_suspected_no_delivery_ignores_tiny_dose():
    cgm = cgm_df([
        ("2026-07-30 12:00", 120), ("2026-07-30 13:00", 190),
        ("2026-07-30 14:00", 250), ("2026-07-30 15:00", 300),
    ])
    # 0.5u is below no_delivery_min_units -> not worth flagging even if it ran away.
    res = iob.analyze(bolus_df([_corr("2026-07-30 12:00", 0.5)]), cgm, 2.0, Config())
    assert res.per_event[0].suspected_no_delivery is False

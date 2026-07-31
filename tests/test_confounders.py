from analysis import confounders
from analysis.contracts import Config
from conftest import bolus_df, cgm_df


def _nights(rise_pairs, dates):
    """CGM with a 03:00->06:00 leg on each date. rise_pairs is (start, end) mg/dl."""
    pairs = []
    for d, (start, end) in zip(dates, rise_pairs):
        pairs.append((f"{d} 03:00", start))
        pairs.append((f"{d} 06:00", end))
    return cgm_df(pairs)


DATES = ["2026-07-27", "2026-07-28", "2026-07-29", "2026-07-30"]


def test_dawn_rise_detected_across_nights():
    cgm = _nights([(100, 150), (110, 160), (105, 140), (100, 155)], DATES)
    f = confounders.analyze(cgm, bolus_df([]), Config())
    assert f.dawn_nights == 4
    assert f.dawn_nights_rising == 4          # all rose >= 30 mg/dl
    assert f.dawn_rise is True
    assert f.dawn_magnitude == 50.0           # median of sorted [35,50,50,55]


def test_no_dawn_when_flat():
    cgm = _nights([(100, 105), (110, 108), (105, 110), (100, 112)], DATES)
    f = confounders.analyze(cgm, bolus_df([]), Config())
    assert f.dawn_nights == 4
    assert f.dawn_nights_rising == 0
    assert f.dawn_rise is False


def test_overnight_bolus_night_excluded():
    cgm = _nights([(100, 150)], ["2026-07-30"])
    # a correction at 04:00 confounds the rise -> night not counted
    bolus = bolus_df([{"time": "2026-07-30 04:00", "kind": "correction", "carbs": 0.0,
                       "total_units": 1.0, "delivered_u": 1.0, "initial_u": None,
                       "delayed_u": None, "bg_entry": 0.0}])
    f = confounders.analyze(cgm, bolus, Config())
    assert f.dawn_nights == 0


def test_rebound_from_low_excluded():
    # a pre-dawn low at 02:00 makes the 03:00->06:00 rise a rebound, not dawn
    pairs = [("2026-07-30 02:00", 60), ("2026-07-30 03:00", 100), ("2026-07-30 06:00", 150)]
    f = confounders.analyze(cgm_df(pairs), bolus_df([]), Config())
    assert f.dawn_nights == 0

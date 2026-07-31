from analysis import iob, meals, stats
from analysis.contracts import Config, RunRef
from conftest import bolus_df, cgm_df


def _meal(time, carbs, units):
    return {"time": time, "kind": "meal", "carbs": carbs, "total_units": units,
            "delivered_u": units, "initial_u": None, "delayed_u": None, "bg_entry": 0.0}


def _corr(time, units):
    return {"time": time, "kind": "correction", "carbs": 0.0, "total_units": units,
            "delivered_u": units, "initial_u": None, "delayed_u": None, "bg_entry": 0.0}


def _lunch(bolus, cgm=None):
    cgm = cgm if cgm is not None else cgm_df([])
    m = meals.analyze(cgm, bolus, Config())
    ib = iob.analyze(bolus, cgm, 2.0, Config())
    return m, ib


def test_bootstrap_ci_deterministic_and_brackets_median():
    # one clean meal per day in the lunch block; CRs 10,11,9,10,12
    bolus = bolus_df([_meal(f"2026-07-2{d} 12:00", c, 6)
                      for d, c in [(5, 60), (6, 66), (7, 54), (8, 60), (9, 72)]])
    m, ib = _lunch(bolus)
    rs = stats.analyze(m, ib, None, Config()).per_block["11-15"]
    assert rs.n == 5
    assert rs.median_effective_cr == 10.0
    assert rs.ci_low is not None and rs.ci_low <= 10.0 <= rs.ci_high
    # fixed seed -> identical CI on a second run
    rs2 = stats.analyze(m, ib, None, Config()).per_block["11-15"]
    assert (rs2.ci_low, rs2.ci_high) == (rs.ci_low, rs.ci_high)


def test_ci_none_below_min_samples():
    bolus = bolus_df([_meal("2026-07-25 12:00", 60, 6), _meal("2026-07-26 12:00", 66, 6)])
    m, ib = _lunch(bolus)
    rs = stats.analyze(m, ib, None, Config()).per_block["11-15"]
    assert rs.n == 2
    assert rs.ci_low is None and rs.ci_high is None


def test_filtered_cr_excludes_contaminated_and_no_delivery():
    runaway = cgm_df([("2026-07-28 12:00", 120), ("2026-07-28 13:00", 190),
                      ("2026-07-28 14:00", 250), ("2026-07-28 15:00", 300)])
    bolus = bolus_df([
        _corr("2026-07-25 11:30", 2),      # 30 min before -> meal starts with IOB 1.5u
        _meal("2026-07-25 12:00", 60, 10),  # cr 6, contaminated (high IOB) -> excluded
        _meal("2026-07-26 12:00", 66, 6),   # cr 11, kept
        _meal("2026-07-27 12:00", 54, 6),   # cr 9, kept
        _meal("2026-07-28 12:00", 60, 10),  # cr 6, runaway CGM -> no-delivery, excluded
    ])
    m, ib = _lunch(bolus, runaway)
    rs = stats.analyze(m, ib, None, Config()).per_block["11-15"]
    assert rs.n == 4
    assert rs.median_effective_cr == 7.5          # median of [6, 11, 9, 6]
    assert rs.n_high_iob == 1
    assert rs.n_suspected_no_delivery == 1
    assert rs.cr_iob_filtered == 10.0             # median of kept [11, 9]


def test_delta_significance_vs_prior():
    bolus = bolus_df([_meal(f"2026-07-2{d} 12:00", 60, 6) for d in [5, 6, 7, 8, 9]])  # all cr 10
    m, ib = _lunch(bolus)
    same = stats.analyze(m, ib, RunRef("2026-07-18", 70.0, {"11-15": 10.0}, {}), Config()).per_block["11-15"]
    assert same.delta_vs_prior == 0.0
    assert same.delta_significant is False        # prior inside the (tight) CI
    far = stats.analyze(m, ib, RunRef("2026-07-18", 70.0, {"11-15": 20.0}, {}), Config()).per_block["11-15"]
    assert far.delta_vs_prior == -10.0
    assert far.delta_significant is True          # prior well outside the CI

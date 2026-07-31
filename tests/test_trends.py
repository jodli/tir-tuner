from analysis import trends
from analysis.contracts import (
    DEFAULT_BLOCKS,
    Config,
    GlycemicBand,
    GlycemicMetrics,
    MealAnalysis,
    MealBlockStats,
    ResolvedSettings,
    RunRef,
)


def _band(tir):
    return GlycemicBand(tir, 3.0, 0.0, 100 - tir, 3.0, 140.0, 35.0, 6.6, 500)


def _glycemic():
    keys = [b.key for b in DEFAULT_BLOCKS]
    return GlycemicMetrics(overall=_band(76.0),
                           per_block={k: _band(80.0 if k == "06-11" else 70.0) for k in keys},
                           per_day=[])


def _meals():
    keys = [b.key for b in DEFAULT_BLOCKS]
    return MealAnalysis(meals=[], per_block={
        k: MealBlockStats(k, n_clean=8 if k == "06-11" else 0,
                          median_effective_cr=9.8 if k == "06-11" else None,
                          median_peak_rise=None, pct_in_range=None, pct_post_meal_hypo=None)
        for k in keys})


def test_build_current_ref_records_configured_cr():
    settings = ResolvedSettings(as_of="2026-07-30", available=True,
                                carb_ratio={"06-11": 10.0}, correction_factor={},
                                change_dates=[], insulin_action_hours=2.0)
    ref = trends.build_current_ref("2026-07-30", _glycemic(), _meals(), settings, Config())
    assert ref.overall_tir == 76.0
    assert ref.per_block_effective_cr["06-11"] == 9.8
    assert ref.per_block_configured_cr["06-11"] == 10.0


def test_build_current_ref_without_settings_has_no_configured_cr():
    settings = ResolvedSettings(as_of="2026-07-30", available=False, carb_ratio={},
                                correction_factor={}, change_dates=[], insulin_action_hours=2.0)
    ref = trends.build_current_ref("2026-07-30", _glycemic(), _meals(), settings, Config())
    assert ref.per_block_configured_cr == {}


def test_compute_trends_orders_prior_and_excludes_current():
    cur = RunRef("2026-07-30", 76.0, {}, {})
    priors = [RunRef("2026-07-16", 70.0, {}, {}), RunRef("2026-07-23", 72.0, {}, {}),
              RunRef("2026-07-30", 99.0, {}, {})]  # same-date entry is excluded
    tr = trends.compute_trends(priors, cur)
    assert [r.as_of for r in tr.prior] == ["2026-07-16", "2026-07-23"]
    assert trends.most_recent_prior(tr).as_of == "2026-07-23"


def test_most_recent_prior_none_when_no_earlier_run():
    cur = RunRef("2026-07-30", 76.0, {}, {})
    tr = trends.compute_trends([], cur)
    assert trends.most_recent_prior(tr) is None


def test_runref_from_legacy_history_defaults_configured_cr():
    # history.json written before per_block_configured_cr existed must load as {}, not None.
    legacy = {"as_of": "2026-07-16", "overall_tir": 70.0,
              "per_block_effective_cr": {"06-11": 10.5}, "per_block_tir": {"06-11": 78.0}}
    ref = RunRef.from_json(legacy)
    assert ref.per_block_configured_cr == {}
    # and a full round-trip preserves a populated map
    ref2 = RunRef.from_json(RunRef("2026-07-30", 76.0, {}, {}, {"06-11": 10.0}).to_json())
    assert ref2.per_block_configured_cr == {"06-11": 10.0}

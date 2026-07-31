from analysis.contracts import (
    DEFAULT_BLOCKS,
    BacktestAnalysis,
    BlockBacktest,
    BlockRobustStats,
    Config,
    ConfounderFlags,
    CorrectionAnalysis,
    CorrectionBlockStats,
    CrChange,
    GlycemicBand,
    GlycemicMetrics,
    IobAnalysis,
    MealAnalysis,
    MealBlockStats,
    PipelineState,
    ResolvedSettings,
    RobustStats,
    RunRef,
    Trends,
    WindowInfo,
)
from analysis.snapshot import build_snapshot


def _band(tir):
    return GlycemicBand(tir, 3.0, 0.0, 100 - tir, 3.0, 140.0, 35.0, 6.6, 500, titr=90.0, coverage_pct=95.0)


def _state():
    keys = [b.key for b in DEFAULT_BLOCKS]
    gly = GlycemicMetrics(
        overall=_band(76.0),
        per_block={k: _band(80.0 if k == "06-11" else 70.0) for k in keys},
        per_day=[],
    )
    meals = MealAnalysis(meals=[], per_block={
        k: MealBlockStats(k, n_clean=8 if k == "06-11" else 0,
                          median_effective_cr=9.8 if k == "06-11" else None,
                          median_peak_rise=40.0 if k == "06-11" else None,
                          pct_in_range=88.0 if k == "06-11" else None,
                          pct_post_meal_hypo=10.0 if k == "06-11" else None)
        for k in keys
    })
    corr = CorrectionAnalysis(corrections=[], per_block={
        k: CorrectionBlockStats(k, n_isolated=4 if k == "18-22" else 0,
                                median_drop_per_unit=45.0 if k == "18-22" else None)
        for k in keys
    }, overall_median_drop_per_unit=45.0)
    settings = ResolvedSettings(as_of="2026-07-30", available=True,
                                carb_ratio={"06-11": 10.0}, correction_factor={"00-24": 40.0},
                                change_dates=["2026-06-01"])
    st = PipelineState(glycemic=gly, meals=meals, corrections=corr, settings=settings,
                       window=WindowInfo("2026-07-30", 4, "2026-07-03T00:00:00", "2026-07-31T00:00:00"))
    return st


def test_snapshot_maps_configured_vs_effective_and_trends():
    st = _state()
    prior = RunRef(as_of="2026-07-23", overall_tir=70.0,
                   per_block_effective_cr={"06-11": 10.5}, per_block_tir={"06-11": 78.0})
    snap = build_snapshot(st, Config(), prior)

    bf = next(b for b in snap.blocks if b.block == "06-11")
    assert bf.configured_cr == 10.0
    assert bf.effective_cr == 9.8
    assert bf.cr_gap_pct == -2.0                      # (9.8-10)/10
    assert bf.trend_effective_cr_delta == -0.7        # 9.8 - 10.5
    assert bf.trend_tir_delta == 2.0                  # 80 - 78
    assert snap.prior_run_date == "2026-07-23"

    # Coarse CF schedule ("00-24") maps onto the dinner block.
    dinner = next(c for c in snap.corrections if c.block == "18-22")
    assert dinner.configured_cf == 40.0
    assert dinner.observed_drop_per_unit == 45.0
    assert dinner.n_isolated == 4


def test_snapshot_without_prior_has_no_trend_deltas():
    snap = build_snapshot(_state(), Config(), prior=None)
    bf = next(b for b in snap.blocks if b.block == "06-11")
    assert bf.trend_effective_cr_delta is None
    assert bf.trend_tir_delta is None
    assert snap.prior_run_date is None


def test_snapshot_joins_robust_confounder_and_backtest_evidence():
    st = _state()
    st.iob = IobAnalysis(available=True, dia_hours=2.0, per_event=[])
    st.confounders = ConfounderFlags(dawn_rise=True, dawn_magnitude=40.0, dawn_nights=5, dawn_nights_rising=4)
    st.stats = RobustStats(per_block={"06-11": BlockRobustStats(
        block="06-11", n=8, median_effective_cr=9.8, ci_low=9.0, ci_high=10.6,
        delta_vs_prior=-0.7, delta_significant=True, cr_iob_filtered=9.9,
        n_high_iob=1, n_suspected_no_delivery=2)})
    st.settings.cr_changes = [CrChange("06-11", "2026-07-15", 10.0, 9.0)]
    st.backtest = BacktestAnalysis(prior_run_date="2026-07-23", per_block={
        "06-11": BlockBacktest(block="06-11", had_reco=True, parameter="CR", direction="down",
                               applied=True, tir_before=78.0, tir_after=80.0, outcome="improved")})
    st.trends = Trends(prior=[RunRef("2026-07-16", 68.0, {}, {}), RunRef("2026-07-23", 78.0, {}, {})],
                       current=RunRef("2026-07-30", 80.0, {}, {}))

    snap = build_snapshot(st, Config(), prior=None)
    bf = next(b for b in snap.blocks if b.block == "06-11")
    assert (bf.ci_low, bf.ci_high) == (9.0, 10.6)
    assert bf.delta_significant is True
    assert bf.cr_iob_filtered == 9.9
    assert (bf.n_high_iob, bf.n_suspected_no_delivery) == (1, 2)
    assert bf.cv == 35.0 and bf.tbr_54 == 0.0 and bf.titr == 90.0 and bf.coverage_pct == 95.0
    assert bf.dawn_rise is True                                   # breakfast is a morning block
    assert bf.config_last_change is not None and bf.config_last_change.to_value == 9.0
    assert bf.backtest_outcome == "improved"

    dinner = next(b for b in snap.blocks if b.block == "18-22")
    assert dinner.dawn_rise is None                              # not a morning block

    assert "dawn_rise" in snap.active_flags
    assert "iob_available" in snap.active_flags
    assert any(f.startswith("no_delivery:06-11=") for f in snap.active_flags)
    assert "exercise" in snap.unavailable_signals
    assert [r.as_of for r in snap.prior_runs] == ["2026-07-16", "2026-07-23"]

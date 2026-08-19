"""TIR-loss attribution and the per-block verdicts."""
from analysis import verdicts
from analysis.contracts import (
    AnalysisSnapshot,
    BlockEvidence,
    Config,
    GlycemicBand,
    GlycemicMetrics,
    Proposal,
    RecommendationSet,
)


def _band(tir, n_readings):
    return GlycemicBand(tir=tir, tbr_70=1.0, tbr_54=0.0, tar_180=100.0 - tir - 1.0,
                        tar_250=0.0, mean=150.0, cv=35.0, gmi=6.5, n_readings=n_readings)


def _ev(block, tir, tar, tbr=1.0, n_clean_meals=10, **kw):
    base = dict(configured_cr=10.0, effective_cr=10.0, cr_gap_pct=0.0,
                n_clean_meals=n_clean_meals, median_peak_rise=40.0, pct_in_range=80.0,
                pct_post_meal_hypo=5.0, tir=tir, tbr_70=tbr, tar_180=tar,
                trend_effective_cr_delta=None, trend_tir_delta=None)
    base.update(kw)
    return BlockEvidence(block=block, label=block, **base)


def _snapshot(blocks):
    return AnalysisSnapshot(
        as_of="2026-08-19", window_weeks=4, overall_tir=70.0, overall_tbr_70=2.0,
        overall_tar_180=28.0, overall_mean=150.0, overall_cv=35.0, overall_gmi=6.5,
        settings_available=True, blocks=blocks, corrections=[], prior_run_date=None)


def _config(blocks):
    """Config restricted to the block keys used by a test."""
    cfg = Config()
    cfg.blocks = [b for b in cfg.blocks if b.key in blocks]
    return cfg


def test_loss_is_weighted_by_time_share():
    """A 6h block at TIR 60 outranks a 2h block at the same TIR."""
    cfg = _config({"00-06", "22-24"})
    gly = GlycemicMetrics(overall=_band(70.0, 8000),
                          per_block={"00-06": _band(60.0, 6000), "22-24": _band(60.0, 2000)},
                          per_day=[])
    snap = _snapshot([_ev("00-06", 60.0, 39.0), _ev("22-24", 60.0, 39.0)])
    out = {v.block: v for v in verdicts.build(gly, snap, None, cfg)}
    assert out["00-06"].share_pct == 75.0
    assert out["00-06"].loss_pp == 30.0        # 0.75 * 40
    assert out["22-24"].loss_pp == 10.0        # 0.25 * 40
    assert out["00-06"].loss_share_pct == 75.0


def test_problem_block_without_a_proposal_says_which_lever_to_check():
    cfg = _config({"15-18"})
    gly = GlycemicMetrics(overall=_band(55.0, 2000),
                          per_block={"15-18": _band(55.0, 2000)}, per_day=[])
    # Second-worst block on the real runs: plenty of time above 180, only 2 meals.
    snap = _snapshot([_ev("15-18", 55.0, 41.0, tbr=3.5, n_clean_meals=2)])
    rec = RecommendationSet(proposals=[], overall_narrative="", insufficient_data_blocks=["15-18"])
    v = verdicts.build(gly, snap, rec, cfg)[0]
    assert v.verdict == "no_cr_lever"
    assert v.dominant == "high"
    assert "Basalrate" in v.note and "2 saubere Mahlzeiten" in v.note


def test_block_with_a_clamped_change_is_marked_change():
    cfg = _config({"06-11"})
    gly = GlycemicMetrics(overall=_band(65.0, 2000),
                          per_block={"06-11": _band(65.0, 2000)}, per_day=[])
    snap = _snapshot([_ev("06-11", 65.0, 34.0)])
    rec = RecommendationSet(
        proposals=[Proposal(block="06-11", parameter="CR", direction="down",
                            current_value=10.0, proposed_value=9.0, confidence="medium",
                            rationale="", caveats="")],
        overall_narrative="", insufficient_data_blocks=[])
    v = verdicts.build(gly, snap, rec, cfg)[0]
    assert v.verdict == "change"
    assert (v.parameter, v.direction, v.proposed_value) == ("CR", "down", 9.0)
    assert "10.0 → 9.0" in v.note


def test_hold_proposal_is_watch_not_change():
    cfg = _config({"11-15"})
    gly = GlycemicMetrics(overall=_band(66.0, 2000),
                          per_block={"11-15": _band(66.0, 2000)}, per_day=[])
    snap = _snapshot([_ev("11-15", 66.0, 33.0, ci_low=9.0, ci_high=13.0)])
    rec = RecommendationSet(
        proposals=[Proposal(block="11-15", parameter="CR", direction="hold",
                            current_value=13.0, proposed_value=13.0, confidence="low",
                            rationale="", caveats="")],
        overall_narrative="", insufficient_data_blocks=[])
    v = verdicts.build(gly, snap, rec, cfg)[0]
    assert v.verdict == "watch"
    assert "Konfidenzintervall" in v.note


def test_block_at_target_is_ok():
    cfg = _config({"00-06"})
    gly = GlycemicMetrics(overall=_band(88.0, 2000),
                          per_block={"00-06": _band(88.0, 2000)}, per_day=[])
    snap = _snapshot([_ev("00-06", 88.0, 11.0)])
    v = verdicts.build(gly, snap, None, cfg)[0]
    assert v.verdict == "ok"

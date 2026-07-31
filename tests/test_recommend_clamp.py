"""Rule-based recommend + the deterministic safety clamp.

The LLM path is not called here (no network); instead a hand-built
RecommendationSet stands in for the model output, asserting the clamp always
governs the persisted result.
"""
from analysis import clamp, recommend
from analysis.contracts import (
    AnalysisSnapshot,
    BlockEvidence,
    Config,
    CorrectionEvidence,
    Proposal,
    RecommendationSet,
)


def _block(block="06-11", **kw):
    defaults = dict(
        label="breakfast", configured_cr=10.0, effective_cr=10.0, cr_gap_pct=0.0,
        n_clean_meals=10, median_peak_rise=40.0, pct_in_range=85.0, pct_post_meal_hypo=5.0,
        tir=80.0, tbr_70=3.0, tar_180=17.0, trend_effective_cr_delta=None, trend_tir_delta=None,
    )
    defaults.update(kw)
    return BlockEvidence(block=block, **defaults)


def _snapshot(blocks, corrections=None):
    return AnalysisSnapshot(
        as_of="2026-07-30", window_weeks=4, overall_tir=76.0, overall_tbr_70=4.0,
        overall_tar_180=20.0, overall_mean=138.0, overall_cv=37.0, overall_gmi=6.6,
        settings_available=True, blocks=blocks, corrections=corrections or [], prior_run_date=None,
    )


# --- rule-based fallback -------------------------------------------------
def test_rule_raises_cr_on_frequent_post_meal_hypo():
    snap = _snapshot([_block(pct_post_meal_hypo=60.0)])
    rec = recommend.recommend(snap, Config(use_llm=False))
    p = rec.proposals[0]
    assert p.parameter == "CR" and p.direction == "up"
    assert p.proposed_value > p.current_value


def test_rule_lowers_cr_on_high_peak_low_range_no_hypo():
    snap = _snapshot([_block(median_peak_rise=90.0, pct_in_range=55.0, pct_post_meal_hypo=5.0)])
    rec = recommend.recommend(snap, Config(use_llm=False))
    p = rec.proposals[0]
    assert p.parameter == "CR" and p.direction == "down"
    assert p.proposed_value < p.current_value


def test_rule_marks_low_sample_block_insufficient():
    snap = _snapshot([_block(n_clean_meals=2, pct_post_meal_hypo=60.0)])
    rec = recommend.recommend(snap, Config(use_llm=False))
    assert rec.proposals == []
    assert "06-11" in rec.insufficient_data_blocks


# --- clamp: the safety net -----------------------------------------------
def test_clamp_bounds_overaggressive_change_and_rederives_direction():
    snap = _snapshot([_block(configured_cr=10.0, n_clean_meals=10)])
    # LLM-style output: slash CR from 10 to 3 (a 70% cut), labelled "up".
    raw = RecommendationSet(
        proposals=[Proposal("06-11", "CR", "up", None, 3.0, "high", "…", "…")],
        overall_narrative="…", insufficient_data_blocks=[],
    )
    clamped, audit = clamp.apply(raw, snap, Config())
    p = clamped.proposals[0]
    assert p.proposed_value == 9.0          # bounded to -10% of 10
    assert p.current_value == 10.0
    assert p.direction == "down"            # re-derived from the numbers, not the label
    assert any(a.field == "proposed_value" for a in audit)


def test_clamp_demotes_low_sample_block():
    snap = _snapshot([_block(block="15-18", n_clean_meals=1)])
    raw = RecommendationSet(
        proposals=[Proposal("15-18", "CR", "down", 8.0, 6.0, "high", "…", "…")],
        overall_narrative="…", insufficient_data_blocks=[],
    )
    clamped, audit = clamp.apply(raw, snap, Config())
    assert clamped.proposals == []
    assert "15-18" in clamped.insufficient_data_blocks
    assert any(a.field == "dropped" for a in audit)


def test_clamp_caps_cf_confidence():
    corr = [CorrectionEvidence(block="18-22", configured_cf=40.0, observed_drop_per_unit=44.0,
                               n_isolated=5, confounds=[])]
    snap = _snapshot([_block(block="18-22")], corrections=corr)
    raw = RecommendationSet(
        proposals=[Proposal("18-22", "CF", "up", 40.0, 44.0, "high", "…", "…")],
        overall_narrative="…", insufficient_data_blocks=[],
    )
    clamped, audit = clamp.apply(raw, snap, Config())
    p = clamped.proposals[0]
    assert p.confidence == "medium"
    assert p.proposed_value == 44.0         # within +10% of 40, unchanged
    assert any(a.field == "confidence" for a in audit)


# --- clamp: evidence-direction guard (#3) --------------------------------
def test_clamp_vetoes_direction_against_evidence():
    # frequent post-meal hypos imply CR up; a "down" proposal (more insulin) is unsafe
    snap = _snapshot([_block(configured_cr=10.0, n_clean_meals=10, pct_post_meal_hypo=60.0)])
    raw = RecommendationSet(
        proposals=[Proposal("06-11", "CR", "down", 10.0, 9.0, "high", "…", "…")],
        overall_narrative="…", insufficient_data_blocks=[],
    )
    clamped, audit = clamp.apply(raw, snap, Config())
    p = clamped.proposals[0]
    assert p.direction == "hold"
    assert p.proposed_value == p.current_value == 10.0
    assert any(a.field == "direction" for a in audit)


def test_clamp_allows_direction_matching_evidence():
    snap = _snapshot([_block(configured_cr=10.0, n_clean_meals=10, pct_post_meal_hypo=60.0)])
    raw = RecommendationSet(
        proposals=[Proposal("06-11", "CR", "up", 10.0, 11.0, "medium", "…", "…")],
        overall_narrative="…", insufficient_data_blocks=[],
    )
    clamped, audit = clamp.apply(raw, snap, Config())
    p = clamped.proposals[0]
    assert p.direction == "up" and p.proposed_value == 11.0
    assert not any(a.field == "direction" for a in audit)


# --- clamp: rule-engine cross-check confidence gate (#4) -----------------
def test_clamp_caps_confidence_when_rule_disagrees():
    snap = _snapshot([_block(configured_cr=10.0, n_clean_meals=10)])   # ambiguous evidence
    raw = RecommendationSet(
        proposals=[Proposal("06-11", "CR", "up", 10.0, 11.0, "high", "…", "…")],
        overall_narrative="…", insufficient_data_blocks=[],
    )
    rule = RecommendationSet(proposals=[], overall_narrative="", insufficient_data_blocks=["06-11"])
    clamped, audit = clamp.apply(raw, snap, Config(), rule)
    p = clamped.proposals[0]
    assert p.confidence == "low"
    assert any(a.field == "confidence" and a.reason.startswith("rule-engine") for a in audit)


def test_clamp_keeps_confidence_when_rule_agrees():
    snap = _snapshot([_block(configured_cr=10.0, n_clean_meals=10)])
    raw = RecommendationSet(
        proposals=[Proposal("06-11", "CR", "up", 10.0, 11.0, "medium", "…", "…")],
        overall_narrative="…", insufficient_data_blocks=[],
    )
    rule = RecommendationSet(
        proposals=[Proposal("06-11", "CR", "up", 10.0, 11.0, "medium", "…", "…")],
        overall_narrative="", insufficient_data_blocks=[],
    )
    clamped, audit = clamp.apply(raw, snap, Config(), rule)
    assert clamped.proposals[0].confidence == "medium"
    assert not any(a.field == "confidence" for a in audit)

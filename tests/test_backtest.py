import json

from analysis import backtest
from analysis.contracts import (
    Config,
    GlycemicBand,
    GlycemicMetrics,
    PipelineState,
    ResolvedSettings,
    RunRef,
    Trends,
    WindowInfo,
)


def _gly(tir_by_block):
    def band(tir):
        return GlycemicBand(tir, 0.0, 0.0, 0.0, 0.0, 140.0, 35.0, 6.6, 100)
    return GlycemicMetrics(overall=band(75.0),
                           per_block={k: band(v) for k, v in tir_by_block.items()}, per_day=[])


def _settings(carb_ratio):
    return ResolvedSettings(as_of="2026-07-30", available=True, carb_ratio=carb_ratio,
                            correction_factor={}, change_dates=[])


PRIOR_RESULT = {
    "recommendation": {"proposals": [
        {"block": "06-11", "parameter": "CR", "direction": "down"},   # lower CR (more insulin)
        {"block": "18-22", "parameter": "CR", "direction": "up"},     # raise CR (less insulin)
    ]},
    "snapshot": {"blocks": [{"block": "06-11", "tir": 70.0}, {"block": "18-22", "tir": 80.0}]},
    "settings": {"available": True, "carb_ratio": {"06-11": 10.0, "18-22": 10.0}},
}


def test_backtest_scores_applied_and_outcome():
    gly = _gly({"06-11": 76.0, "18-22": 79.0})           # breakfast +6 (improved), dinner -1 (unchanged)
    settings = _settings({"06-11": 9.0, "18-22": 10.0})  # breakfast lowered (applied), dinner unchanged
    res = backtest.analyze(PRIOR_RESULT, "2026-07-23", gly, settings, Config())
    assert res.prior_run_date == "2026-07-23"
    b1 = res.per_block["06-11"]
    assert b1.applied is True and b1.outcome == "improved" and b1.tir_before == 70.0
    b2 = res.per_block["18-22"]
    assert b2.applied is False and b2.outcome == "unchanged"


def test_backtest_no_prior_is_empty():
    res = backtest.analyze(None, None, _gly({"06-11": 76.0}), _settings({}), Config())
    assert res.prior_run_date is None
    assert res.per_block == {}


def test_backtest_run_reads_prior_result_from_disk(tmp_path):
    out = tmp_path / "runs"
    prior_dir = out / "2026-07-23"
    prior_dir.mkdir(parents=True)
    (prior_dir / "result.json").write_text(json.dumps(PRIOR_RESULT), encoding="utf-8")

    st = PipelineState(
        glycemic=_gly({"06-11": 76.0, "18-22": 79.0}),
        settings=_settings({"06-11": 9.0, "18-22": 10.0}),
        window=WindowInfo("2026-07-30", 4, "2026-07-03T00:00:00", "2026-07-31T00:00:00"),
        trends=Trends(prior=[RunRef("2026-07-23", 70.0, {}, {})],
                      current=RunRef("2026-07-30", 76.0, {}, {})),
    )
    st = backtest.run(st, Config(out_dir=str(out)))
    assert st.backtest.prior_run_date == "2026-07-23"
    assert st.backtest.per_block["06-11"].outcome == "improved"

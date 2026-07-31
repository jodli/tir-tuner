"""End-to-end pipeline, CLI, report rendering and stage isolation."""
import json

from analysis import (
    clamp, corrections, glycemic, iob, loaders, meals, recommend, settings, snapshot, trends, window,
)
from analysis.__main__ import main
from analysis.contracts import Config, PipelineState
from analysis.report import format_summary

MISSING_SETTINGS = "/nonexistent/settings.json"


def _build_state(sample_dir):
    cfg = Config(data_dir=sample_dir, use_llm=False, settings_path=MISSING_SETTINGS, weeks=4)
    st = PipelineState()
    for fn in (loaders.run, window.run, glycemic.run, meals.run, corrections.run,
               settings.run, iob.run, trends.run, snapshot.run, recommend.run, clamp.run):
        st = fn(st, cfg)
    return st, cfg


def test_run_writes_result_and_history(tmp_path, sample_dir):
    out = tmp_path / "runs"
    rc = main(["run", "--data", sample_dir, "--out", str(out),
               "--settings", MISSING_SETTINGS, "--no-llm", "--no-charts"])
    assert rc == 0
    assert (out / "2026-07-30" / "result.json").exists()
    for stage in ("load", "window", "glycemic", "meals", "corrections",
                  "settings", "iob", "trends", "snapshot", "recommend", "clamp", "history"):
        assert (out / "2026-07-30" / "stages" / f"{stage}.json").exists()
    hist = json.loads((out / "history.json").read_text())
    assert any(r["as_of"] == "2026-07-30" for r in hist)


def test_rerun_is_idempotent_in_history(tmp_path, sample_dir):
    out = tmp_path / "runs"
    args = ["run", "--data", sample_dir, "--out", str(out),
            "--settings", MISSING_SETTINGS, "--no-llm", "--no-charts"]
    main(args)
    main(args)
    hist = json.loads((out / "history.json").read_text())
    assert [r["as_of"] for r in hist].count("2026-07-30") == 1


def test_stage_isolation_reproduces_glycemic(tmp_path, sample_dir):
    out = tmp_path / "runs"
    main(["run", "--data", sample_dir, "--out", str(out),
          "--settings", MISSING_SETTINGS, "--no-llm", "--no-charts"])
    win = out / "2026-07-30" / "stages" / "window.json"
    dest = tmp_path / "gly.json"
    rc = main(["stage", "glycemic", "--in", str(win), "--out", str(dest), "--run-dir", str(out)])
    assert rc == 0
    full = PipelineState.from_json(json.loads((out / "2026-07-30" / "stages" / "glycemic.json").read_text()))
    iso = PipelineState.from_json(json.loads(dest.read_text()))
    assert full.glycemic.overall.n_readings == iso.glycemic.overall.n_readings


def test_report_is_german_and_flags_inference_mode(sample_dir):
    st, cfg = _build_state(sample_dir)
    text = format_summary(st, cfg)
    assert "Zeit im Zielbereich" in text
    assert "Empfehlungen" in text
    assert "Hinweise" in text
    assert "Inferenzmodus" in text        # no settings.json -> inference-only banner
    assert "ärztliche Anweisung" in text   # standing caveat present


def test_full_state_round_trips_through_json(sample_dir):
    st, _ = _build_state(sample_dir)
    back = PipelineState.from_json(st.to_json())
    assert back.recommendation is not None
    assert back.snapshot.as_of == st.snapshot.as_of
    assert len(back.glycemic.per_block) == len(st.glycemic.per_block)
    assert [c.time for c in back.corrections.corrections] == [c.time for c in st.corrections.corrections]

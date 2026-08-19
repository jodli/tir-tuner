"""The HTML report builder, driven by a real (offline) run."""
import json
import os
import sys

import pytest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

import make_html_report as mhr  # noqa: E402
from analysis.__main__ import main  # noqa: E402

MISSING_SETTINGS = "/nonexistent/settings.json"


@pytest.fixture
def built_run(tmp_path, sample_dir):
    out = tmp_path / "runs"
    main(["run", "--data", sample_dir, "--out", str(out),
          "--settings", MISSING_SETTINGS, "--no-llm", "--no-charts"])
    return out


def test_report_is_one_self_contained_file(built_run):
    assert mhr.main([str(built_run)]) == 0
    text = (built_run / "report.html").read_text(encoding="utf-8")
    assert text.startswith("<!DOCTYPE html>")
    assert 'lang="de"' in text
    # No external assets: no link/script src, images inline only.
    assert "<link" not in text and "src=\"http" not in text
    for heading in ("Status im aktuellen Fenster", "Empfehlung des Sprachmodells",
                    "Wo die TIR verloren geht", "Kennzahlen nach Tagesblock",
                    "Verlauf über die Läufe"):
        assert heading in text


def test_charts_are_embedded_when_present(tmp_path, sample_dir):
    out = tmp_path / "runs"
    main(["run", "--data", sample_dir, "--out", str(out),
          "--settings", MISSING_SETTINGS, "--no-llm"])          # charts on
    mhr.main([str(out)])
    text = (out / "report.html").read_text(encoding="utf-8")
    assert "data:image/png;base64," in text


def test_narrative_of_every_run_is_shown(built_run):
    mhr.main([str(built_run)])
    text = (built_run / "report.html").read_text(encoding="utf-8")
    result = json.loads((built_run / "2026-07-30" / "result.json").read_text(encoding="utf-8"))
    narrative = result["recommendation"]["overall_narrative"]
    assert narrative[:40] in text


def test_missing_runs_dir_is_an_error(tmp_path, capsys):
    assert mhr.main([str(tmp_path / "nope")]) == 1
    assert "no result.json" in capsys.readouterr().err


def test_german_number_format(built_run):
    assert mhr.num(75.34, " %") == "75,3 %"
    assert mhr.num(None) == "–"
    assert mhr.num(12, "") == "12"

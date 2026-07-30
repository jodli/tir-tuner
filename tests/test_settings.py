import json

from analysis import settings


def _write(tmp_path, data):
    p = tmp_path / "settings.json"
    p.write_text(json.dumps(data), encoding="utf-8")
    return str(p)


HISTORY = {
    "carb_ratio": [
        {"effective_from": "2026-06-01", "blocks": {"06-11": 10, "11-15": 12}},
        {"effective_from": "2026-07-15", "blocks": {"06-11": 9, "11-15": 11}},
    ],
    "correction_factor": [
        {"effective_from": "2026-06-01", "blocks": {"00-24": 40}},
    ],
}


def test_resolve_picks_latest_applicable(tmp_path):
    hist = settings.load_history(_write(tmp_path, HISTORY))
    r = settings.resolve(hist, "2026-07-20")
    assert r.available is True
    assert r.carb_ratio == {"06-11": 9.0, "11-15": 11.0}
    assert r.correction_factor == {"00-24": 40.0}
    assert r.change_dates == ["2026-06-01", "2026-07-15"]


def test_resolve_before_change(tmp_path):
    hist = settings.load_history(_write(tmp_path, HISTORY))
    r = settings.resolve(hist, "2026-06-10")
    assert r.carb_ratio == {"06-11": 10.0, "11-15": 12.0}


def test_resolve_before_any_schedule_is_empty(tmp_path):
    hist = settings.load_history(_write(tmp_path, HISTORY))
    r = settings.resolve(hist, "2026-05-01")
    assert r.carb_ratio == {}
    assert r.available is True


def test_missing_file_is_inference_only():
    r = settings.resolve(settings.load_history("/no/such/settings.json"), "2026-07-20")
    assert r.available is False
    assert r.carb_ratio == {}
    assert r.change_dates == []

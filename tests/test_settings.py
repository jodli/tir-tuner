import json

from analysis import settings
from analysis.contracts import TimeBlock


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


def test_dia_falls_back_to_default_when_absent(tmp_path):
    hist = settings.load_history(_write(tmp_path, HISTORY))
    assert hist.insulin_action_hours is None
    # inference mode still exposes the default so IOB stays available
    assert settings.resolve(None, "2026-07-20", dia_default=2.0).insulin_action_hours == 2.0
    assert settings.resolve(hist, "2026-07-20", dia_default=2.0).insulin_action_hours == 2.0


def test_dia_configured_value_wins(tmp_path):
    data = {**HISTORY, "insulin_action_hours": 3.5}
    r = settings.resolve(settings.load_history(_write(tmp_path, data)), "2026-07-20", dia_default=2.0)
    assert r.insulin_action_hours == 3.5


def test_last_change_for_block_reports_from_to(tmp_path):
    hist = settings.load_history(_write(tmp_path, HISTORY))
    b = TimeBlock("breakfast", 6, 11)  # 06-11: 10 -> 9 on 2026-07-15
    assert settings.last_change_for_block(hist.carb_ratio, b, "2026-07-20") == ("2026-07-15", 10.0, 9.0)


def test_last_change_none_before_second_schedule(tmp_path):
    hist = settings.load_history(_write(tmp_path, HISTORY))
    b = TimeBlock("breakfast", 6, 11)
    # only the first schedule applies -> no change yet
    assert settings.last_change_for_block(hist.carb_ratio, b, "2026-07-01") is None


# --- analysis grid vs. the configured pump schedule -------------------------
REAL_SCHEDULE = {"00-06": 17.0, "06-11": 11.0, "11-17": 13.0, "17-24": 11.0}


def test_coarse_key_covers_a_block():
    assert settings.value_for_block({"00-24": 40.0}, TimeBlock("dinner", 18, 22)) == 40.0


def test_block_inside_one_schedule_entry_resolves():
    assert settings.value_for_block(REAL_SCHEDULE, TimeBlock("lunch", 11, 15)) == 13.0
    assert settings.schedule_block_for(REAL_SCHEDULE, TimeBlock("lunch", 11, 15)) == "11-17"


def test_block_straddling_two_schedule_values_has_no_configured_value():
    """15-18 sits in both 11-17 (13) and 17-24 (11): no single configured CR."""
    afternoon = TimeBlock("afternoon", 15, 18)
    assert settings.overlapping_keys(REAL_SCHEDULE, afternoon) == ["11-17", "17-24"]
    assert settings.value_for_block(REAL_SCHEDULE, afternoon) is None
    # A change would still have to be entered in the entry holding its start hour.
    assert settings.schedule_block_for(REAL_SCHEDULE, afternoon) == "11-17"


def test_map_to_schedule_reports_shared_and_ambiguous_blocks(tmp_path):
    from analysis.contracts import Config

    hist = settings.load_history(_write(tmp_path, {
        "carb_ratio": [{"effective_from": "2026-01-01", "blocks": REAL_SCHEDULE}]}))
    r = settings.resolve(hist, "2026-08-19")
    settings.map_to_schedule(r, Config())
    assert r.schedule_block == {"00-06": "00-06", "06-11": "06-11", "11-15": "11-17",
                                "15-18": "11-17", "18-22": "17-24", "22-24": "17-24"}
    # Changing the evening also changes the late block, and vice versa.
    assert r.schedule_shared_with == {"11-17": ["11-15", "15-18"], "17-24": ["18-22", "22-24"]}
    assert r.ambiguous_blocks == ["15-18"]

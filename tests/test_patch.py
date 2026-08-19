"""The settings.json patch built from the clamped proposals."""
from analysis import patch
from analysis.contracts import (
    Config,
    PipelineState,
    Proposal,
    RecommendationSet,
    ResolvedSettings,
    WindowInfo,
)

SCHEDULE = {"00-06": 17.0, "06-11": 11.0, "11-17": 13.0, "17-24": 11.0}


def _state(proposals, available=True):
    st = PipelineState()
    st.window = WindowInfo(as_of="2026-08-19", weeks=4,
                           start="2026-07-23T00:00:00", end="2026-08-20T00:00:00")
    st.settings = ResolvedSettings(
        as_of="2026-08-19", available=available, carb_ratio=dict(SCHEDULE),
        correction_factor={"00-24": 75.0}, change_dates=[])
    st.recommendation = RecommendationSet(proposals=proposals, overall_narrative="",
                                          insufficient_data_blocks=[])
    return st


def _p(block, param, direction, current, proposed, schedule_block):
    return Proposal(block=block, parameter=param, direction=direction,
                    current_value=current, proposed_value=proposed, confidence="medium",
                    rationale="", caveats="", schedule_block=schedule_block)


def test_patch_expresses_changes_in_pump_blocks():
    st = _state([_p("18-22", "CR", "down", 11.0, 10.0, "17-24"),
                 _p("00-06", "CF", "up", 75.0, 82.5, "00-24")])
    result, conflicts = patch.build(st, Config())
    assert conflicts == []
    # A full schedule, so it can be pasted as the next dated entry.
    assert result["carb_ratio"] == [{
        "effective_from": "2026-08-20",
        "blocks": {"00-06": 17.0, "06-11": 11.0, "11-17": 13.0, "17-24": 10.0}}]
    assert result["correction_factor"][0]["blocks"] == {"00-24": 82.5}


def test_holds_produce_no_patch():
    st = _state([_p("11-15", "CR", "hold", 13.0, 13.0, "11-17")])
    assert patch.build(st, Config()) == ({}, [])


def test_two_blocks_sharing_a_pump_block_conflict_instead_of_guessing():
    """18-22 and 22-24 both live in 17-24: contradicting values are not applied."""
    st = _state([_p("18-22", "CR", "down", 11.0, 10.0, "17-24"),
                 _p("22-24", "CR", "up", 11.0, 12.0, "17-24")])
    result, conflicts = patch.build(st, Config())
    assert result == {}
    assert len(conflicts) == 1 and "17-24" in conflicts[0]


def test_agreeing_proposals_in_one_pump_block_are_applied_once():
    st = _state([_p("18-22", "CR", "down", 11.0, 10.0, "17-24"),
                 _p("22-24", "CR", "down", 11.0, 10.0, "17-24")])
    result, conflicts = patch.build(st, Config())
    assert conflicts == []
    assert result["carb_ratio"][0]["blocks"]["17-24"] == 10.0


def test_inference_mode_has_nothing_to_patch():
    st = _state([_p("18-22", "CR", "down", 11.0, 10.0, "17-24")], available=False)
    assert patch.build(st, Config()) == ({}, [])


def test_write_creates_the_file(tmp_path):
    st = _state([_p("18-22", "CR", "down", 11.0, 10.0, "17-24")])
    path = patch.write(st, Config(out_dir=str(tmp_path)))
    assert path.endswith("2026-08-19/settings_patch.json")
    assert '"17-24": 10.0' in open(path, encoding="utf-8").read()

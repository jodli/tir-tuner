"""Stage 6 (settings): resolve the configured CR/CF schedule at a date.

The export never contains the configured carb ratio / correction factor, so the
user maintains a hand-edited ``settings.json`` holding the dated history of
their schedules. This stage picks the schedule in effect on the analysis date
and exposes the change dates (for chart markers). If the file is absent the
pipeline still runs in inference-only mode (``available == False``).

settings.json shape::

    {
      "carb_ratio":        [{"effective_from": "2026-07-01", "blocks": {"06-11": 10, ...}}],
      "correction_factor": [{"effective_from": "2026-07-01", "blocks": {"00-24": 40}}]
    }
"""
from __future__ import annotations

import json
import os
from typing import Optional

from .contracts import (
    Config,
    DatedSchedule,
    PipelineState,
    ResolvedSettings,
    SettingsHistory,
    TimeBlock,
)


def value_for_block(schedule: dict[str, float], block: TimeBlock) -> Optional[float]:
    """Look up a configured value for a config block from a settings schedule.

    Handles both aligned keys (``"06-11"``) and coarse ones (``"00-24"``): the
    value whose ``HH-HH`` hour range contains the block's start hour wins, with
    an exact key match preferred.
    """
    if block.key in schedule:
        return schedule[block.key]
    for key, value in schedule.items():
        try:
            start, end = (int(x) for x in key.split("-"))
        except ValueError:
            continue
        if start <= block.start_hour < end:
            return value
    return None


def load_history(path: str) -> Optional[SettingsHistory]:
    if not os.path.exists(path):
        return None
    with open(path, encoding="utf-8") as f:
        data = json.load(f)

    def parse(key: str) -> list[DatedSchedule]:
        out = []
        for entry in data.get(key, []):
            try:
                blocks = {str(k): float(v) for k, v in entry["blocks"].items()}
                out.append(DatedSchedule(effective_from=str(entry["effective_from"]), blocks=blocks))
            except (KeyError, TypeError, ValueError) as exc:
                raise ValueError(f"Invalid {key} entry in {path}: {entry!r} ({exc})") from exc
        out.sort(key=lambda s: s.effective_from)
        return out

    return SettingsHistory(carb_ratio=parse("carb_ratio"), correction_factor=parse("correction_factor"))


def _pick(schedules: list[DatedSchedule], as_of: str) -> dict[str, float]:
    applicable = [s for s in schedules if s.effective_from <= as_of]
    return dict(max(applicable, key=lambda s: s.effective_from).blocks) if applicable else {}


def resolve(history: Optional[SettingsHistory], as_of: str) -> ResolvedSettings:
    if history is None:
        return ResolvedSettings(as_of=as_of, available=False, carb_ratio={}, correction_factor={}, change_dates=[])
    change_dates = sorted({s.effective_from for s in history.carb_ratio + history.correction_factor})
    return ResolvedSettings(
        as_of=as_of,
        available=True,
        carb_ratio=_pick(history.carb_ratio, as_of),
        correction_factor=_pick(history.correction_factor, as_of),
        change_dates=change_dates,
    )


def run(state: PipelineState, config: Config) -> PipelineState:
    as_of = state.window.as_of if state.window else (config.as_of or "")
    state.settings = resolve(load_history(config.settings_path), as_of)
    return state

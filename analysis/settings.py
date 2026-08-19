"""Stage 6 (settings): resolve the configured CR/CF schedule at a date.

The export never contains the configured carb ratio / correction factor, so the
user maintains a hand-edited ``settings.json`` holding the dated history of
their schedules. This stage picks the schedule in effect on the analysis date
and exposes the change dates (for chart markers). If the file is absent the
pipeline still runs in inference-only mode (``available == False``).

settings.json shape::

    {
      "insulin_action_hours": 2,
      "carb_ratio":        [{"effective_from": "2026-07-01", "blocks": {"06-11": 10, ...}}],
      "correction_factor": [{"effective_from": "2026-07-01", "blocks": {"00-24": 40}}]
    }

``insulin_action_hours`` is optional; when absent the IOB stage falls back to
``Config.insulin_action_hours_default``.
"""
from __future__ import annotations

import json
import os
from typing import Optional

from .contracts import (
    Config,
    CrChange,
    DatedSchedule,
    PipelineState,
    ResolvedSettings,
    SettingsHistory,
    TimeBlock,
)


def _key_hours(key: str) -> Optional[tuple[int, int]]:
    try:
        start, end = (int(x) for x in key.split("-"))
    except ValueError:
        return None
    return start, end


def overlapping_keys(schedule: dict[str, float], block: TimeBlock) -> list[str]:
    """Schedule keys whose hour range overlaps the analysis block, in time order.

    An analysis block can straddle a schedule boundary: with a pump programmed
    ``11-17`` / ``17-24``, the analysis block ``15-18`` sits in both. Callers need
    to know that, because a proposal for such a block cannot be entered anywhere
    without also changing hours it was not measured over.
    """
    hits = []
    for key in schedule:
        hours = _key_hours(key)
        if hours is None:
            continue
        start, end = hours
        if start < block.end_hour and block.start_hour < end:
            hits.append((start, key))
    return [key for _, key in sorted(hits)]


def schedule_block_for(schedule: dict[str, float], block: TimeBlock) -> Optional[str]:
    """The schedule key a change for this block would have to be entered in."""
    if block.key in schedule:
        return block.key
    keys = overlapping_keys(schedule, block)
    if not keys:
        return None
    # The key containing the block's start hour is the one that governs it.
    for key in keys:
        hours = _key_hours(key)
        if hours and hours[0] <= block.start_hour < hours[1]:
            return key
    return keys[0]


def value_for_block(schedule: dict[str, float], block: TimeBlock) -> Optional[float]:
    """Configured value for an analysis block, or ``None`` when it is ambiguous.

    Handles aligned keys (``"06-11"``) and coarse ones (``"00-24"``). A block that
    overlaps several schedule entries with *different* values has no single
    configured value: reporting the one at its start hour silently compared meals
    after 17:00 against the 11-17 setting. Such a block returns ``None`` and is
    listed in ``ResolvedSettings.ambiguous_blocks`` instead.
    """
    if block.key in schedule:
        return schedule[block.key]
    values = {schedule[key] for key in overlapping_keys(schedule, block)}
    if len(values) == 1:
        return values.pop()
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

    dia = data.get("insulin_action_hours")
    try:
        dia = float(dia) if dia is not None else None
    except (TypeError, ValueError) as exc:
        raise ValueError(f"Invalid insulin_action_hours in {path}: {dia!r} ({exc})") from exc

    return SettingsHistory(carb_ratio=parse("carb_ratio"),
                           correction_factor=parse("correction_factor"),
                           insulin_action_hours=dia)


def last_change_for_block(
    schedules: list[DatedSchedule], block: TimeBlock, as_of: str
) -> Optional[tuple[str, float, float]]:
    """Most recent change to this block's value on/before ``as_of``.

    Returns ``(effective_from, from_value, to_value)`` for the latest schedule
    whose value for this block differs from the one before it, or ``None`` if the
    block has fewer than two applicable schedules or the value never changed. Lets
    the reasoning step attribute an outcome to a specific setting change.
    """
    applicable = [(s.effective_from, value_for_block(s.blocks, block))
                  for s in schedules if s.effective_from <= as_of]
    applicable = [(d, v) for d, v in applicable if v is not None]
    applicable.sort(key=lambda x: x[0])
    for i in range(len(applicable) - 1, 0, -1):
        (_, prev_v), (cur_d, cur_v) = applicable[i - 1], applicable[i]
        if cur_v != prev_v:
            return (cur_d, prev_v, cur_v)
    return None


def _pick(schedules: list[DatedSchedule], as_of: str) -> dict[str, float]:
    applicable = [s for s in schedules if s.effective_from <= as_of]
    return dict(max(applicable, key=lambda s: s.effective_from).blocks) if applicable else {}


def resolve(history: Optional[SettingsHistory], as_of: str, dia_default: float = 2.0) -> ResolvedSettings:
    # DIA falls back to the configured default even in inference-only mode, so the
    # IOB stage stays available without a settings.json.
    if history is None:
        return ResolvedSettings(as_of=as_of, available=False, carb_ratio={}, correction_factor={},
                                change_dates=[], insulin_action_hours=dia_default)
    change_dates = sorted({s.effective_from for s in history.carb_ratio + history.correction_factor})
    dia = history.insulin_action_hours if history.insulin_action_hours is not None else dia_default
    return ResolvedSettings(
        as_of=as_of,
        available=True,
        carb_ratio=_pick(history.carb_ratio, as_of),
        correction_factor=_pick(history.correction_factor, as_of),
        change_dates=change_dates,
        insulin_action_hours=dia,
    )


def _map_grid(schedule: dict[str, float], config: Config
              ) -> tuple[dict[str, str], dict[str, list[str]], list[str]]:
    """(analysis block -> schedule key, schedule key -> shared blocks, ambiguous)."""
    by_block: dict[str, str] = {}
    shared: dict[str, list[str]] = {}
    ambiguous: list[str] = []
    for b in config.blocks:
        key = schedule_block_for(schedule, b)
        if key is None:
            continue
        by_block[b.key] = key
        shared.setdefault(key, []).append(b.key)
        if len({schedule[k] for k in overlapping_keys(schedule, b)}) > 1:
            ambiguous.append(b.key)
    return by_block, {k: v for k, v in shared.items() if len(v) > 1}, ambiguous


def map_to_schedule(resolved: ResolvedSettings, config: Config) -> None:
    """Fill in how the analysis grid maps onto the configured schedules.

    A change for an analysis block has to be entered in whichever schedule block
    contains it, which may cover other analysis blocks too: with a pump programmed
    ``17-24``, changing the evening also changes the late block. The report needs
    to say so, and a block straddling two different schedule values gets no single
    configured value at all. CR and CF are mapped separately, since the two
    schedules are programmed independently (CF is often a single 00-24 entry).
    """
    if resolved.carb_ratio:
        (resolved.schedule_block, resolved.schedule_shared_with,
         resolved.ambiguous_blocks) = _map_grid(resolved.carb_ratio, config)
    if resolved.correction_factor:
        (resolved.schedule_block_cf, resolved.schedule_shared_with_cf,
         resolved.ambiguous_blocks_cf) = _map_grid(resolved.correction_factor, config)


def run(state: PipelineState, config: Config) -> PipelineState:
    as_of = state.window.as_of if state.window else (config.as_of or "")
    history = load_history(config.settings_path)
    resolved = resolve(history, as_of, config.insulin_action_hours_default)
    if history is not None:
        changes = []
        for b in config.blocks:
            c = last_change_for_block(history.carb_ratio, b, as_of)
            if c is not None:
                changes.append(CrChange(block=b.key, effective_from=c[0], from_value=c[1], to_value=c[2]))
        resolved.cr_changes = changes
    map_to_schedule(resolved, config)
    state.settings = resolved
    return state

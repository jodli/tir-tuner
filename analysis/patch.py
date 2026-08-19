"""Turn the clamped proposals into a settings.json entry you can paste.

The report used to state a proposal in analysis-block terms and stop there, so
applying it meant translating block keys into pump schedule keys by hand, and the
next run had no way to tell whether the change had been entered. This module
builds the dated schedule entry that expresses the run's proposals in the *pump's*
own blocks, which is both the thing to type into the pump and the thing to append
to ``settings.json`` so the next run's backtest can see it.

Only ``up``/``down`` proposals are patched. Two proposals landing in the same
schedule key with different values are dropped and reported as a conflict rather
than silently picking one.
"""
from __future__ import annotations

import datetime as dt
import json
import os
from typing import Optional

from .contracts import Config, PipelineState

_PARAM_KEY = {"CR": "carb_ratio", "CF": "correction_factor"}


def _effective_from(as_of: str) -> str:
    """A change entered after a run takes effect the following day."""
    return (dt.date.fromisoformat(as_of) + dt.timedelta(days=1)).isoformat()


def build(state: PipelineState, config: Config) -> tuple[dict, list[str]]:
    """(patch, conflicts). ``patch`` is empty when there is nothing to apply."""
    rec, settings = state.recommendation, state.settings
    if rec is None or settings is None or not settings.available:
        return {}, []

    schedules = {"CR": dict(settings.carb_ratio), "CF": dict(settings.correction_factor)}
    proposed: dict[tuple[str, str], float] = {}
    conflicts: list[str] = []
    for p in rec.proposals:
        if p.direction not in ("up", "down") or p.proposed_value is None:
            continue
        key = p.schedule_block
        if key is None or key not in schedules.get(p.parameter, {}):
            conflicts.append(f"{p.parameter} {p.block}: kein passender Pumpenblock in settings.json")
            continue
        ident = (p.parameter, key)
        if ident in proposed and abs(proposed[ident] - p.proposed_value) > 1e-9:
            conflicts.append(f"{p.parameter} {key}: widersprüchliche Vorschläge "
                             f"({proposed[ident]} vs. {p.proposed_value}), nicht übernommen")
            proposed.pop(ident)
            continue
        proposed[ident] = p.proposed_value

    patch: dict = {}
    # CR before CF, mirroring settings.json, so the output can be pasted as-is.
    ordered = sorted(proposed.items(), key=lambda kv: (kv[0][0] != "CR", kv[0][1]))
    for (param, key), value in ordered:
        blocks = patch.setdefault(_PARAM_KEY[param], [{
            "effective_from": _effective_from(state.window.as_of),
            "blocks": dict(schedules[param]),
        }])[0]["blocks"]
        blocks[key] = value
    return patch, conflicts


def write(state: PipelineState, config: Config) -> Optional[str]:
    """Write ``runs/<as_of>/settings_patch.json``; return its path, or None."""
    patch, _ = build(state, config)
    if not patch:
        return None
    run_dir = os.path.join(config.out_dir, state.window.as_of)
    os.makedirs(run_dir, exist_ok=True)
    path = os.path.join(run_dir, "settings_patch.json")
    with open(path, "w", encoding="utf-8") as f:
        json.dump(patch, f, ensure_ascii=False, indent=2)
    return path

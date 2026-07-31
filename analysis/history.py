"""Run history persistence.

Keeps a compact per-run record (aggregate metrics only, no raw CGM) in
``runs/history.json`` so successive weekly runs can show how per-block
effective CR and TIR respond to setting changes. Also persists the full,
human-facing run record to ``runs/<as_of>/result.json``.

``load_refs`` reads the compact history; ``persist_run`` is the final write
stage. Trend construction (``build_current_ref`` / ``compute_trends``) lives in
:mod:`analysis.trends`.
"""
from __future__ import annotations

import datetime as dt
import json
import os
from typing import Optional

from .contracts import (
    Config,
    PipelineState,
    RunRef,
    to_jsonable,
)


def _history_path(config: Config) -> str:
    return os.path.join(config.out_dir, "history.json")


def load_refs(config: Config) -> list[RunRef]:
    path = _history_path(config)
    if not os.path.exists(path):
        return []
    with open(path, encoding="utf-8") as f:
        data = json.load(f)
    refs = [RunRef.from_json(d) for d in data]
    refs.sort(key=lambda r: r.as_of)
    return refs


def persist_run(state: PipelineState, config: Config, generated_at: Optional[str] = None) -> str:
    """Write history.json (compact refs) and runs/<as_of>/result.json (full record)."""
    if state.trends is None or state.window is None:
        raise ValueError("persist_run requires state.trends and state.window")
    as_of = state.window.as_of
    generated_at = generated_at or dt.datetime.now().isoformat(timespec="seconds")

    # Update the compact history: replace any existing entry for this as_of.
    refs = [r for r in load_refs(config) if r.as_of != as_of]
    refs.append(state.trends.current)
    refs.sort(key=lambda r: r.as_of)
    os.makedirs(config.out_dir, exist_ok=True)
    with open(_history_path(config), "w", encoding="utf-8") as f:
        json.dump([to_jsonable(r) for r in refs], f, ensure_ascii=False, indent=2)

    # Full human-facing record for this run.
    record = {
        "as_of": as_of,
        "generated_at": generated_at,
        "window": to_jsonable(state.window),
        "glycemic": to_jsonable(state.glycemic),
        "meals_per_block": to_jsonable(state.meals.per_block) if state.meals else None,
        "corrections": to_jsonable(state.corrections),
        "settings": to_jsonable(state.settings),
        "iob": to_jsonable(state.iob),
        "confounders": to_jsonable(state.confounders),
        "stats": to_jsonable(state.stats),
        "backtest": to_jsonable(state.backtest),
        "snapshot": to_jsonable(state.snapshot),
        "recommendation_raw": to_jsonable(state.recommendation_raw),
        "recommendation": to_jsonable(state.recommendation),
        "clamp_audit": to_jsonable(state.clamp_audit),
        "trends": to_jsonable(state.trends),
    }
    run_dir = os.path.join(config.out_dir, as_of)
    os.makedirs(run_dir, exist_ok=True)
    result_path = os.path.join(run_dir, "result.json")
    with open(result_path, "w", encoding="utf-8") as f:
        json.dump(record, f, ensure_ascii=False, indent=2)
    return result_path


def run(state: PipelineState, config: Config) -> PipelineState:
    persist_run(state, config)
    return state

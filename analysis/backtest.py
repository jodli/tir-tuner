"""Stage (backtest): did the previous run's recommendations play out?

Reads the most recent prior ``runs/<date>/result.json`` (which already stores the
clamped recommendation, the snapshot the model saw, and the resolved settings) and,
per block that had a CR/CF proposal, reports whether the configured value actually
moved that way (``applied``) and whether block TIR improved since
(``outcome``). This closes the loop: the reasoning step can see that its last
advice helped, did nothing, or was never applied, and escalate / hold / reverse.

Outcome is suggestive, not causal: everything the confounder stages flag still
applies. Runs before the snapshot so the result becomes evidence in it.
"""
from __future__ import annotations

import json
import os
from typing import Optional

from .contracts import (
    BacktestAnalysis,
    BlockBacktest,
    Config,
    GlycemicMetrics,
    PipelineState,
    ResolvedSettings,
)
from .settings import value_for_block
from .trends import most_recent_prior


def _applied(param, direction, block_key, prior_settings: dict, settings: ResolvedSettings, config: Config):
    if param != "CR" or direction not in ("up", "down"):
        return None
    if not prior_settings.get("available") or not settings.available:
        return None
    tb = next((b for b in config.blocks if b.key == block_key), None)
    if tb is None:
        return None
    prev = value_for_block(prior_settings.get("carb_ratio", {}), tb)
    cur = value_for_block(settings.carb_ratio, tb)
    if prev is None or cur is None:
        return None
    return cur < prev - 1e-9 if direction == "down" else cur > prev + 1e-9


def _outcome(before: Optional[float], after: Optional[float], config: Config) -> str:
    if before is None or after is None:
        return "unknown"
    d = after - before
    if d >= config.backtest_tir_epsilon:
        return "improved"
    if d <= -config.backtest_tir_epsilon:
        return "worsened"
    return "unchanged"


def analyze(prior_result: Optional[dict], prior_date: Optional[str],
            glycemic: GlycemicMetrics, settings: ResolvedSettings, config: Config) -> BacktestAnalysis:
    if prior_result is None:
        return BacktestAnalysis(prior_run_date=prior_date, per_block={})

    proposals = (prior_result.get("recommendation") or {}).get("proposals") or []
    prior_blocks = {b["block"]: b for b in (prior_result.get("snapshot") or {}).get("blocks", [])}
    prior_settings = prior_result.get("settings") or {}

    per_block: dict[str, BlockBacktest] = {}
    for p in proposals:
        key = p.get("block")
        tir_before = (prior_blocks.get(key) or {}).get("tir")
        tir_after = glycemic.per_block[key].tir if key in glycemic.per_block else None
        per_block[key] = BlockBacktest(
            block=key,
            had_reco=True,
            parameter=p.get("parameter"),
            direction=p.get("direction"),
            applied=_applied(p.get("parameter"), p.get("direction"), key, prior_settings, settings, config),
            tir_before=tir_before,
            tir_after=tir_after,
            outcome=_outcome(tir_before, tir_after, config),
        )
    return BacktestAnalysis(prior_run_date=prior_date, per_block=per_block)


def run(state: PipelineState, config: Config) -> PipelineState:
    for name in ("glycemic", "settings", "trends"):
        if getattr(state, name) is None:
            raise ValueError(f"backtest stage requires state.{name}")
    prior = most_recent_prior(state.trends)
    prior_result = None
    if prior is not None:
        path = os.path.join(config.out_dir, prior.as_of, "result.json")
        if os.path.exists(path):
            with open(path, encoding="utf-8") as f:
                prior_result = json.load(f)
    state.backtest = analyze(prior_result, prior.as_of if prior else None,
                             state.glycemic, state.settings, config)
    return state

"""Stage (trends): load run history and build cross-run trends.

Extracted from the snapshot stage so snapshot stays pure assembly. Loads the
compact per-run history (``runs/history.json``), builds this run's
:class:`RunRef` (including the configured CR in effect, so a later stage can line
up a setting against its effect), and computes the prior series.
"""
from __future__ import annotations

from typing import Optional

from . import history
from .contracts import (
    Config,
    GlycemicMetrics,
    MealAnalysis,
    PipelineState,
    ResolvedSettings,
    RunRef,
    Trends,
)
from .settings import value_for_block


def build_current_ref(
    as_of: str,
    glycemic: GlycemicMetrics,
    meals: MealAnalysis,
    settings: Optional[ResolvedSettings],
    config: Config,
) -> RunRef:
    configured: dict[str, float] = {}
    if settings is not None and settings.available:
        for b in config.blocks:
            v = value_for_block(settings.carb_ratio, b)
            if v is not None:
                configured[b.key] = v
    return RunRef(
        as_of=as_of,
        overall_tir=glycemic.overall.tir,
        per_block_effective_cr={
            k: v.median_effective_cr
            for k, v in meals.per_block.items()
            if v.median_effective_cr is not None
        },
        per_block_tir={k: v.tir for k, v in glycemic.per_block.items() if v.tir is not None},
        per_block_configured_cr=configured,
    )


def compute_trends(prior_refs: list[RunRef], current: RunRef) -> Trends:
    prior = sorted((r for r in prior_refs if r.as_of != current.as_of), key=lambda r: r.as_of)
    return Trends(prior=prior, current=current)


def most_recent_prior(trends: Trends) -> Optional[RunRef]:
    earlier = [r for r in trends.prior if r.as_of < trends.current.as_of]
    return earlier[-1] if earlier else None


def run(state: PipelineState, config: Config) -> PipelineState:
    for name in ("glycemic", "meals", "settings", "window"):
        if getattr(state, name) is None:
            raise ValueError(f"trends stage requires state.{name}")
    current = build_current_ref(state.window.as_of, state.glycemic, state.meals, state.settings, config)
    state.trends = compute_trends(history.load_refs(config), current)
    return state

"""Glooko type-1 diabetes analysis pipeline.

A staged pipeline that reads a de-identified Glooko CSV export (produced by
``strip_pii.py``) and produces time-in-range metrics plus conservative,
outcome-inferred carb-ratio (CR) and correction-factor (CF) tuning proposals.

The pipeline is intentionally split into small stages with explicit typed
inputs and outputs (see :mod:`analysis.contracts`). Every stage is a pure
function ``run(state, config) -> state`` so it can be exercised in isolation
from a saved artifact. Only the deterministic parts live in code; the judgment
step (which blocks to nudge and why) is delegated to an LLM via BAML, and its
numeric output is always bounded by a deterministic safety clamp.
"""

__all__ = ["contracts"]

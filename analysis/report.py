"""German terminal summary of a completed run.

Reads a fully-populated :class:`PipelineState` and prints a compact,
human-readable overview: glycemic status, the per-block table, the clamped
recommendations, trend deltas and the standing caveats. Pure output; it does
not mutate state.
"""
from __future__ import annotations

from typing import Optional

from .contracts import Config, PipelineState
from .strings import BLOCK_DE, CAVEATS, L


def _fmt(x: Optional[float], suffix: str = "") -> str:
    return "–" if x is None else f"{x}{suffix}"


def _block_de(config: Config, key: str) -> str:
    for b in config.blocks:
        if b.key == key:
            return BLOCK_DE.get(b.name, key)
    return key


def format_summary(state: PipelineState, config: Config) -> str:
    lines: list[str] = []
    w = state.window
    o = state.glycemic.overall
    lines.append("=" * 64)
    lines.append(f"  {L['title']}")
    lines.append("=" * 64)
    lines.append(f"{L['window']}: {w.start[:10]} – {w.as_of}  ({w.weeks} {L['weeks']}, "
                 f"{o.n_readings} {L['readings']})")
    if not state.settings.available:
        lines.append(f"! {L['inference_only']}")
    lines.append("")

    # Overall glycemic block
    lines.append(f"{L['overall']}:")
    lines.append(f"  {L['tir']:<28} {_fmt(o.tir, ' %')}")
    lines.append(f"  {L['tbr70']:<28} {_fmt(o.tbr_70, ' %')}   "
                 f"{L['tbr54']}: {_fmt(o.tbr_54, ' %')}")
    lines.append(f"  {L['tar180']:<28} {_fmt(o.tar_180, ' %')}   "
                 f"{L['tar250']}: {_fmt(o.tar_250, ' %')}")
    lines.append(f"  {L['mean']:<28} {_fmt(o.mean, ' mg/dl')}   "
                 f"{L['gmi']}: {_fmt(o.gmi, ' %')}   {L['cv']}: {_fmt(o.cv, ' %')}")
    lines.append("")

    # Per-block table
    lines.append(f"{L['per_block']}:")
    header = (f"  {L['col_block']:<12}{L['col_tir']:>7}{L['col_eff_cr']:>9}"
              f"{L['col_conf_cr']:>9}{L['col_peak']:>8}{L['col_inrange']:>8}"
              f"{L['col_hypo']:>8}{L['col_meals']:>8}")
    lines.append(header)
    lines.append("  " + "-" * (len(header) - 2))
    ev_by_block = {b.block: b for b in state.snapshot.blocks}
    for b in config.blocks:
        ev = ev_by_block[b.key]
        lines.append(
            f"  {_block_de(config, b.key):<12}"
            f"{_fmt(ev.tir):>7}{_fmt(ev.effective_cr):>9}{_fmt(ev.configured_cr):>9}"
            f"{_fmt(ev.median_peak_rise):>8}{_fmt(ev.pct_in_range):>8}"
            f"{_fmt(ev.pct_post_meal_hypo):>8}{ev.n_clean_meals:>8}"
        )
    lines.append("")

    # Recommendations
    rec = state.recommendation
    mode = L["mode_llm"] if config.use_llm else L["mode_rules"]
    lines.append(f"{L['recommendations']} ({mode}):")
    if rec.overall_narrative:
        lines.append(f"  {rec.overall_narrative}")
    if not rec.proposals:
        lines.append(f"  {L['no_recommendations']}")
    for p in rec.proposals:
        arrow = {"up": "↑", "down": "↓", "hold": "→"}.get(p.direction, "→")
        lines.append(
            f"  • [{p.parameter} {_block_de(config, p.block)}] "
            f"{_fmt(p.current_value)} {arrow} {_fmt(p.proposed_value)}  "
            f"({L['confidence']}: {p.confidence})"
        )
        lines.append(f"      {p.rationale}")
        if p.caveats:
            lines.append(f"      ⚠ {p.caveats}")
    if rec.insufficient_data_blocks:
        blocks = ", ".join(_block_de(config, k) for k in rec.insufficient_data_blocks)
        lines.append(f"  {L['insufficient']}: {blocks}")
    lines.append("")

    # Trend vs prior run
    prior = _prior(state)
    if prior is not None:
        cur = state.trends.current
        d_tir = None if (cur.overall_tir is None or prior.overall_tir is None) else round(cur.overall_tir - prior.overall_tir, 1)
        lines.append(f"{L['trend']} ({prior.as_of}):  TIR {_fmt(prior.overall_tir, ' %')} → "
                     f"{_fmt(cur.overall_tir, ' %')} ({_fmt(d_tir, ' pp')})")
        lines.append("")

    # Clamp adjustments
    if state.clamp_audit:
        lines.append(f"{L['clamp_note']}:")
        for a in state.clamp_audit:
            lines.append(f"  - [{a.parameter} {a.block}] {a.field}: {a.original} → {a.adjusted} ({a.reason})")
        lines.append("")

    # Caveats
    lines.append(f"{L['caveats']}:")
    for c in CAVEATS:
        lines.append(f"  - {c}")
    lines.append("=" * 64)
    return "\n".join(lines)


def _prior(state: PipelineState):
    if state.trends is None:
        return None
    earlier = [r for r in state.trends.prior if r.as_of < state.trends.current.as_of]
    return earlier[-1] if earlier else None


def print_summary(state: PipelineState, config: Config) -> None:
    print(format_summary(state, config))

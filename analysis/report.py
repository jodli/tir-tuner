"""German terminal summary of a completed run.

Reads a fully-populated :class:`PipelineState` and prints a compact,
human-readable overview: glycemic status, the per-block table, the clamped
recommendations, trend deltas and the standing caveats. Pure output; it does
not mutate state.
"""
from __future__ import annotations

import json
import os
from typing import Optional

from . import patch
from .contracts import Config, PipelineState
from .strings import BLOCK_DE, CAVEATS, L


def _fmt(x: Optional[float], suffix: str = "") -> str:
    return "–" if x is None else f"{x}{suffix}"


def _block_de(config: Config, key: str) -> str:
    for b in config.blocks:
        if b.key == key:
            return BLOCK_DE.get(b.name, key)
    return key


def _abbrev(items: list[str], limit: int = 4) -> str:
    """Comma-joined list, truncated so a long gap does not flood the report."""
    if len(items) <= limit:
        return ", ".join(items)
    return ", ".join(items[:limit]) + f" (+{len(items) - limit})"


def _data_quality(state: PipelineState, config: Config) -> list[str]:
    """Coverage against the requested window, missing/partial days, provenance."""
    gly = state.glycemic
    lines: list[str] = []
    cov = gly.overall.coverage_pct
    if gly.window_days:
        lines.append(f"{L['coverage']}: {_fmt(cov, ' %')}  "
                     f"({gly.days_with_data} {L['of_days']} {gly.window_days} {L['days']})")
    ds = state.dataset
    if ds is not None and ds.source_files:
        lines.append(f"{L['sources']}: {len(ds.source_files)} {L['files']}"
                     f"{f' ({ds.source_range})' if ds.source_range else ''}, "
                     f"{ds.n_duplicate_rows} {L['dupes_dropped']}")
    if gly.missing_days:
        lines.append(f"! {L['missing_days']} ({len(gly.missing_days)}): {_abbrev(gly.missing_days)}")
    if gly.partial_days:
        lines.append(f"! {L['partial_days']} ({len(gly.partial_days)}): {_abbrev(gly.partial_days)}")
    if cov is not None and cov < config.min_coverage_pct:
        lines.append(f"! {L['thin_window']}")
    amb = state.settings.ambiguous_blocks if state.settings else []
    if amb:
        names = ", ".join(f"{_block_de(config, k)} ({k})" for k in amb)
        lines.append(f"! {L['grid_mismatch'].format(blocks=names)}")
    return lines


_VERDICT_ARROW = {"change": "→ ändern", "watch": "beobachten",
                  "no_cr_lever": "kein CR-Hebel", "ok": "ok"}


def _proposal_lines(p, config: Config, ev=None) -> list[str]:
    """One proposal: the change, where to enter it, the reasoning, the caveat."""
    arrow = {"up": "↑", "down": "↓", "hold": "→"}.get(p.direction, "→")
    repeat = ""
    if ev is not None and ev.n_times_proposed_before and p.direction in ("up", "down"):
        nth = ev.n_times_proposed_before + 1
        repeat = f"  [{L['nth_time'].format(n=nth)}"
        repeat += f", {L['still_unapplied']}]" if ev.unapplied_streak else "]"
    out = [f"  • [{p.parameter} {_block_de(config, p.block)}] "
           f"{_fmt(p.current_value)} {arrow} {_fmt(p.proposed_value)}  "
           f"({L['confidence']}: {p.confidence}){repeat}"]
    if p.schedule_block and p.direction in ("up", "down"):
        where = f"      {L['enter_in']}: {p.schedule_block}"
        if p.also_affects:
            shared = ", ".join(_block_de(config, k) for k in p.also_affects)
            where += f"  ({L['also_affects']}: {shared})"
        out.append(where)
    out.append(f"      {p.rationale}")
    if p.caveats:
        out.append(f"      ⚠ {p.caveats}")
    return out


def _loss_section(state: PipelineState, config: Config) -> list[str]:
    """TIR loss per block, worst first, each with its verdict."""
    if not state.verdicts:
        return []
    lines = [f"{L['loss_title']}:"]
    header = (f"  {L['col_block']:<12}{L['col_share']:>8}{L['col_tir']:>7}"
              f"{L['col_loss_pp']:>9}{L['col_loss_share']:>9}{L['col_dominant']:>9}"
              f"  {L['col_verdict']}")
    lines.append(header)
    lines.append("  " + "-" * (len(header) - 2))
    dominant_de = {"high": L["dom_high"], "low": L["dom_low"], "mixed": L["dom_mixed"]}
    ranked = sorted(state.verdicts, key=lambda v: v.loss_pp or 0.0, reverse=True)
    for v in ranked:
        lines.append(
            f"  {v.label:<12}{_fmt(v.share_pct, ' %'):>8}{_fmt(v.tir):>7}"
            f"{_fmt(v.loss_pp):>9}{_fmt(v.loss_share_pct, ' %'):>9}"
            f"{dominant_de.get(v.dominant or '', '–'):>9}"
            f"  {_VERDICT_ARROW.get(v.verdict, v.verdict)}"
        )
    # "change" blocks are spelled out under Empfehlungen right below, so only the
    # blocks that would otherwise produce no output at all get a line here.
    for v in ranked:
        if v.verdict in ("watch", "no_cr_lever"):
            lines.append(f"  • {v.label}: {v.note}")
    lines.append("")
    return lines


def _patch_section(state: PipelineState, config: Config) -> list[str]:
    """The settings.json entry to append once the change is entered in the pump."""
    patch_data, conflicts = patch.build(state, config)
    if not patch_data and not conflicts:
        return []
    lines = [f"{L['patch_title']}:"]
    if patch_data:
        for line in json.dumps(patch_data, ensure_ascii=False, indent=2).splitlines():
            lines.append(f"  {line}")
        lines.append(f"  {L['patch_hint'].format(path=os.path.join(config.out_dir, state.window.as_of, 'settings_patch.json'))}")
    for c in conflicts:
        lines.append(f"  ! {c}")
    lines.append("")
    return lines


def format_summary(state: PipelineState, config: Config) -> str:
    lines: list[str] = []
    w = state.window
    o = state.glycemic.overall
    lines.append("=" * 64)
    lines.append(f"  {L['title']}")
    lines.append("=" * 64)
    lines.append(f"{L['window']}: {w.start[:10]} – {w.as_of}  ({w.weeks} {L['weeks']}, "
                 f"{o.n_readings} {L['readings']})")
    lines.extend(_data_quality(state, config))
    if not state.settings.available:
        lines.append(f"! {L['inference_only']}")
    if state.settings.insulin_action_hours is not None:
        lines.append(f"{L['dia']}: {state.settings.insulin_action_hours} h")
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

    # Per-block table. TBR/TAR sit next to the meal-level hypo columns on purpose:
    # a high "Hypo-E." share with a low TBR means short dips, not lost time.
    lines.append(f"{L['per_block']}:")
    header = (f"  {L['col_block']:<12}{L['col_tir']:>7}{L['col_tbr']:>7}{L['col_tar']:>7}"
              f"{L['col_eff_cr']:>9}{L['col_conf_cr']:>9}{L['col_peak']:>8}"
              f"{L['col_hypo_event']:>8}{L['col_dip']:>7}{L['col_meals']:>8}")
    lines.append(header)
    lines.append("  " + "-" * (len(header) - 2))
    ev_by_block = {b.block: b for b in state.snapshot.blocks}
    for b in config.blocks:
        ev = ev_by_block[b.key]
        lines.append(
            f"  {_block_de(config, b.key):<12}"
            f"{_fmt(ev.tir):>7}{_fmt(ev.tbr_70):>7}{_fmt(ev.tar_180):>7}"
            f"{_fmt(ev.effective_cr):>9}{_fmt(ev.configured_cr):>9}"
            f"{_fmt(ev.median_peak_rise):>8}"
            f"{_fmt(ev.pct_post_meal_hypo):>8}{_fmt(ev.pct_post_meal_dip):>7}"
            f"{ev.n_clean_meals:>8}"
        )
    lines.append(f"  {L['hypo_event_legend'].format(depth=int(config.hypo_event_min_depth), dur=int(config.hypo_event_min_dur_min), low=int(config.tir_low))}")
    lines.append("")

    # Where the TIR is actually lost, and a verdict for every block
    lines.extend(_loss_section(state, config))

    # Recommendations, changes first: a run's one actionable item must not be
    # buried among the "hold" proposals it is printed next to.
    rec = state.recommendation
    mode = L["mode_llm"] if config.use_llm else L["mode_rules"]
    lines.append(f"{L['recommendations']} ({mode}):")
    if rec.overall_narrative:
        lines.append(f"  {rec.overall_narrative}")
    changes = [p for p in rec.proposals if p.direction in ("up", "down")]
    holds = [p for p in rec.proposals if p.direction not in ("up", "down")]
    if not changes:
        lines.append(f"  {L['no_recommendations']}")
    else:
        lines.append(f"  {L['apply_now']}:")
        for p in changes:
            lines.extend(_proposal_lines(p, config, ev_by_block.get(p.block)))
    if holds:
        lines.append(f"  {L['watch_only']}:")
        for p in holds:
            lines.extend(_proposal_lines(p, config, ev_by_block.get(p.block)))
    if rec.insufficient_data_blocks:
        blocks = ", ".join(_block_de(config, k) for k in rec.insufficient_data_blocks)
        lines.append(f"  {L['insufficient']}: {blocks}")
    lines.append("")
    lines.extend(_patch_section(state, config))

    # Trend vs prior run
    prior = _prior(state)
    if prior is not None:
        cur = state.trends.current
        d_tir = None if (cur.overall_tir is None or prior.overall_tir is None) else round(cur.overall_tir - prior.overall_tir, 1)
        lines.append(f"{L['trend']} ({prior.as_of}):  TIR {_fmt(prior.overall_tir, ' %')} → "
                     f"{_fmt(cur.overall_tir, ' %')} ({_fmt(d_tir, ' pp')})")
        lines.append("")

    # Backtest: did the previous run's recommendations play out?
    bt = state.backtest
    if bt is not None and bt.per_block:
        applied_de = {True: L["applied_yes"], False: L["applied_no"], None: L["applied_unknown"]}
        lines.append(f"{L['backtest']} ({bt.prior_run_date}):")
        for key, r in bt.per_block.items():
            arrow = {"up": "↑", "down": "↓", "hold": "→"}.get(r.direction or "hold", "→")
            outcome = L.get(f"outcome_{r.outcome}", r.outcome)
            tir = ""
            if r.tir_before is not None and r.tir_after is not None:
                tir = f"  (TIR {r.tir_before} → {r.tir_after})"
            lines.append(f"  • [{r.parameter} {_block_de(config, key)}] {arrow}  "
                         f"{L['applied']}: {applied_de[r.applied]} → {outcome}{tir}")
        lines.append("")

    # Suspected insulin-delivery failures (excluded from CR estimation)
    nodel = [(_block_de(config, b.block), b.n_suspected_no_delivery)
             for b in state.snapshot.blocks if b.n_suspected_no_delivery]
    if nodel:
        detail = ", ".join(f"{name} ({n})" for name, n in nodel)
        lines.append(f"! {L['no_delivery_note']}: {detail}")
        lines.append("")

    # Clamp adjustments
    if state.clamp_audit:
        lines.append(f"{L['clamp_note']}:")
        for a in state.clamp_audit:
            lines.append(f"  - [{a.parameter} {_block_de(config, a.block)}] "
                         f"{L.get('clamp_' + a.field, a.field)}: "
                         f"{a.original} → {a.adjusted} ({a.reason})")
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

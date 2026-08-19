"""Charts: PNGs that track glycemic status and CR/TIR reaction across runs.

Uses the non-interactive Agg backend so it runs headless. Every chart is
defensive: it is skipped (not fatal) when the underlying data is insufficient.
"""
from __future__ import annotations

import datetime as dt
import os

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt  # noqa: E402

from .contracts import Config, PipelineState  # noqa: E402
from .strings import BLOCK_DE  # noqa: E402


def render(state: PipelineState, config: Config) -> list[str]:
    as_of = state.window.as_of
    out_dir = os.path.join(config.out_dir, as_of, "charts")
    os.makedirs(out_dir, exist_ok=True)
    written: list[str] = []

    for fn in (_chart_block_tir, _chart_agp, _chart_effective_cr_trend, _chart_tir_trend):
        try:
            path = fn(state, config, out_dir)
        except Exception:  # a single bad chart must not break the run
            path = None
        if path:
            written.append(path)
    return written


def _chart_block_tir(state, config, out_dir):
    gly = state.glycemic
    keys = [b.key for b in config.blocks]
    tirs = [gly.per_block[k].tir for k in keys]
    labels = [BLOCK_DE.get(b.name, b.key) for b in config.blocks]
    if not any(t is not None for t in tirs):
        return None
    fig, ax = plt.subplots(figsize=(8, 4))
    ax.bar(labels, [t or 0 for t in tirs], color="#4C78A8")
    ax.axhline(70, color="#54A24B", linestyle="--", linewidth=1, label="Ziel 70 %")
    ax.set_ylabel("TIR %")
    ax.set_title(f"Time-in-Range nach Tagesblock ({state.window.as_of})")
    ax.set_ylim(0, 100)
    ax.legend()
    return _save(fig, out_dir, "tir_per_block.png")


def _chart_agp(state, config, out_dir):
    cgm = state.dataset.cgm if state.dataset else None
    if cgm is None or len(cgm) == 0:
        return None
    df = cgm.assign(hour=cgm["time"].dt.hour)
    grp = df.groupby("hour")["mg_dl"]
    med = grp.median()
    q25 = grp.quantile(0.25)
    q75 = grp.quantile(0.75)
    fig, ax = plt.subplots(figsize=(8, 4))
    ax.fill_between(med.index, q25, q75, color="#4C78A8", alpha=0.25, label="25-75 %")
    ax.plot(med.index, med.values, color="#4C78A8", label="Median")
    ax.axhspan(70, 180, color="#54A24B", alpha=0.10)
    ax.set_xlabel("Stunde")
    ax.set_ylabel("Glukose mg/dl")
    ax.set_xlim(0, 23)
    ax.set_title(f"Tagesprofil (AGP-artig) ({state.window.as_of})")
    ax.legend()
    return _save(fig, out_dir, "daily_profile.png")


def _runs_sorted(state):
    refs = list(state.trends.prior) + [state.trends.current]
    return sorted(refs, key=lambda r: r.as_of)


def _dates(refs):
    """Real dates, not strings: on a categorical axis a change marker drawn with
    ``axvline("2026-07-30")`` becomes a *new* category appended after the last
    run, which put the setting-change line to the right of the newest point."""
    return [dt.date.fromisoformat(r.as_of) for r in refs]


def _chart_effective_cr_trend(state, config, out_dir):
    """Effective CR per block against the configured CR that produced it."""
    if state.trends is None:
        return None
    refs = _runs_sorted(state)
    dates = _dates(refs)
    fig, ax = plt.subplots(figsize=(8, 4))
    plotted = False
    for b in config.blocks:
        # Drop points below the min-sample gate: a 2-meal block swung between 28
        # and 37 g/E and flattened every informative block into a single band.
        ys = []
        for r in refs:
            n = r.per_block_n_clean_meals.get(b.key)
            value = r.per_block_effective_cr.get(b.key)
            ys.append(value if (n is None or n >= config.min_clean_meals) else None)
        if not any(y is not None for y in ys):
            continue
        line, = ax.plot(dates, ys, marker="o", label=BLOCK_DE.get(b.name, b.key))
        plotted = True
        # The configured value in the same colour: the point of the chart is
        # whether a setting change actually moved the effective CR.
        conf = [r.per_block_configured_cr.get(b.key) for r in refs]
        if any(c is not None for c in conf):
            ax.step(dates, conf, where="post", linestyle="--", linewidth=1,
                    color=line.get_color(), alpha=0.6)
    if not plotted:
        plt.close(fig)
        return None
    ax.set_ylabel("CR (g/E)")
    ax.set_title(f"Effektives CR (durchgezogen) vs. konfiguriertes CR (gestrichelt)\n"
                 f"nur Blöcke mit mindestens {config.min_clean_meals} sauberen Mahlzeiten")
    ax.legend(fontsize=8, ncol=2)
    fig.autofmt_xdate(rotation=30)
    return _save(fig, out_dir, "effective_cr_trend.png")


def _chart_tir_trend(state, config, out_dir):
    if state.trends is None:
        return None
    refs = _runs_sorted(state)
    dates = _dates(refs)
    tirs = [r.overall_tir for r in refs]
    if len(refs) < 1 or not any(t is not None for t in tirs):
        return None
    fig, ax = plt.subplots(figsize=(8, 4))
    ax.plot(dates, tirs, marker="o", color="#4C78A8", label="Gesamt-TIR")
    ax.axhline(70, color="#54A24B", linestyle="--", linewidth=1)
    # Mark configured-setting change dates inside the plotted span.
    labelled = False
    for cd in (state.settings.change_dates if state.settings else []):
        day = dt.date.fromisoformat(cd)
        if dates[0] <= day <= dates[-1]:
            ax.axvline(day, color="#E45756", linestyle=":", linewidth=1,
                       label=None if labelled else "Einstellungsänderung")
            labelled = True
    ax.set_ylabel("TIR %")
    ax.set_ylim(0, 100)
    ax.set_title("Gesamt-TIR über die Läufe")
    ax.legend()
    fig.autofmt_xdate(rotation=30)
    return _save(fig, out_dir, "tir_trend.png")


def _save(fig, out_dir, name):
    path = os.path.join(out_dir, name)
    fig.tight_layout()
    fig.savefig(path, dpi=110)
    plt.close(fig)
    return path

"""Charts: PNGs that track glycemic status and CR/TIR reaction across runs.

Uses the non-interactive Agg backend so it runs headless. Every chart is
defensive: it is skipped (not fatal) when the underlying data is insufficient.
"""
from __future__ import annotations

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


def _chart_effective_cr_trend(state, config, out_dir):
    if state.trends is None:
        return None
    refs = _runs_sorted(state)
    dates = [r.as_of for r in refs]
    fig, ax = plt.subplots(figsize=(8, 4))
    plotted = False
    for b in config.blocks:
        ys = [r.per_block_effective_cr.get(b.key) for r in refs]
        if any(y is not None for y in ys):
            ax.plot(dates, ys, marker="o", label=BLOCK_DE.get(b.name, b.key))
            plotted = True
    if not plotted:
        plt.close(fig)
        return None
    ax.set_ylabel("effektives CR (g/E)")
    ax.set_title("Effektives CR pro Block über die Läufe")
    ax.legend(fontsize=8, ncol=2)
    fig.autofmt_xdate(rotation=30)
    return _save(fig, out_dir, "effective_cr_trend.png")


def _chart_tir_trend(state, config, out_dir):
    if state.trends is None:
        return None
    refs = _runs_sorted(state)
    dates = [r.as_of for r in refs]
    tirs = [r.overall_tir for r in refs]
    if len(refs) < 1 or not any(t is not None for t in tirs):
        return None
    fig, ax = plt.subplots(figsize=(8, 4))
    ax.plot(dates, tirs, marker="o", color="#4C78A8", label="Gesamt-TIR")
    ax.axhline(70, color="#54A24B", linestyle="--", linewidth=1)
    # Mark configured-setting change dates.
    for cd in (state.settings.change_dates if state.settings else []):
        if dates and dates[0] <= cd <= dates[-1]:
            ax.axvline(cd, color="#E45756", linestyle=":", linewidth=1)
    ax.set_ylabel("TIR %")
    ax.set_ylim(0, 100)
    ax.set_title("Gesamt-TIR über die Läufe (rote Linie = Einstellungsänderung)")
    ax.legend()
    fig.autofmt_xdate(rotation=30)
    return _save(fig, out_dir, "tir_trend.png")


def _save(fig, out_dir, name):
    path = os.path.join(out_dir, name)
    fig.tight_layout()
    fig.savefig(path, dpi=110)
    plt.close(fig)
    return path

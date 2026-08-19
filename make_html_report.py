#!/usr/bin/env python3
"""Build a self-contained HTML report from the runs in ``runs/``.

Reads every ``runs/<as_of>/result.json`` plus the newest run's charts and writes
``runs/report.html``: one file, PNGs embedded as base64, no external assets, so it
can be opened offline or handed to the care team.

The page shows the newest run in detail (status, where TIR is lost, the clamped
recommendation, the per-block table, the charts) and then the whole week-by-week
history including each run's LLM narrative, so the arc across runs is readable.

Colour follows the data-viz rules used elsewhere: bar length carries magnitude, so
the bars are a single hue; status colours (good/warning/critical) never appear
without a glyph and a label, because green and red are indistinguishable under
deuteranopia. Dark mode has its own steps, not an inverted copy.

Usage
-----
    uv run python make_html_report.py [RUNS_DIR] [-o OUT]
"""
from __future__ import annotations

import argparse
import base64
import datetime as dt
import glob
import html
import json
import os
import sys

CHARTS = [
    ("daily_profile.png", "Tagesprofil (AGP-artig)",
     "Median und 25-75-%-Band je Stunde über das aktuelle Fenster. Grün: Zielbereich 70-180."),
    ("tir_per_block.png", "TIR nach Tagesblock",
     "Zeit im Zielbereich pro Block, aktuelles Fenster."),
    ("tir_trend.png", "Gesamt-TIR über die Läufe",
     "Ein Punkt pro Lauf. Gepunktete Linie: Datum einer Einstellungsänderung."),
    ("effective_cr_trend.png", "Effektives vs. konfiguriertes CR",
     "Durchgezogen: aus den Daten geschätztes CR. Gestrichelt: konfigurierter Wert. "
     "Nur Blöcke mit ausreichend sauberen Mahlzeiten."),
]

# Consensus targets, shown next to each number so a value can be judged on sight.
TARGETS = {
    "tir": (">70 %", lambda v: "good" if v >= 70 else ("warning" if v >= 60 else "critical")),
    "tbr_70": ("<4 %", lambda v: "good" if v < 4 else ("warning" if v < 6 else "critical")),
    "tbr_54": ("<1 %", lambda v: "good" if v < 1 else "critical"),
    "tar_180": ("<25 %", lambda v: "good" if v < 25 else ("warning" if v < 35 else "critical")),
    "tar_250": ("<5 %", lambda v: "good" if v < 5 else ("warning" if v < 10 else "critical")),
    "cv": ("≤36 %", lambda v: "good" if v <= 36 else "warning"),
    "coverage_pct": (">70 %", lambda v: "good" if v >= 80 else ("warning" if v >= 70 else "critical")),
}
STATUS_GLYPH = {"good": "✓", "warning": "!", "critical": "✕", "action": "→"}
VERDICT_DE = {"change": "ändern", "watch": "beobachten",
              "no_cr_lever": "kein CR-Hebel", "ok": "im Ziel"}
VERDICT_STATUS = {"change": "action", "watch": "warning",
                  "no_cr_lever": "critical", "ok": "good"}
DOM_DE = {"high": "zu hoch", "low": "zu tief", "mixed": "beides"}
AUDIT_FIELD_DE = {"dropped": "verworfen", "confidence": "Sicherheit",
                  "direction": "Richtung", "proposed_value": "Vorschlag"}


# --- helpers ---------------------------------------------------------------
def e(x) -> str:
    return html.escape(str(x), quote=True)


def num(x, suffix: str = "", digits: int = 1) -> str:
    if x is None:
        return "–"
    if isinstance(x, float):
        return f"{x:.{digits}f}".replace(".", ",") + suffix
    return f"{x}{suffix}"


def load_runs(runs_dir: str) -> list[dict]:
    out = []
    for path in sorted(glob.glob(os.path.join(runs_dir, "*", "result.json"))):
        with open(path, encoding="utf-8") as f:
            out.append(json.load(f))
    return sorted(out, key=lambda r: r["as_of"])


def embed_png(path: str) -> str | None:
    if not os.path.exists(path):
        return None
    with open(path, "rb") as f:
        return "data:image/png;base64," + base64.b64encode(f.read()).decode("ascii")


def badge(status: str, label: str) -> str:
    """Status pill: colour plus glyph plus text, never colour alone."""
    return (f'<span class="badge {status}"><span aria-hidden="true">{STATUS_GLYPH[status]}</span>'
            f'{e(label)}</span>')


# --- sections --------------------------------------------------------------
def tile(label: str, value: str, target: str | None, status: str | None,
         delta: str | None = None) -> str:
    parts = [f'<div class="tile"><div class="tile-label">{e(label)}</div>',
             f'<div class="tile-value">{value}</div>']
    foot = []
    if target:
        foot.append(f'<span class="muted">Ziel {e(target)}</span>')
    if status:
        foot.append(badge(status, "erreicht" if status == "good" else "über Ziel"))
    if delta:
        foot.append(delta)
    parts.append(f'<div class="tile-foot">{"".join(foot)}</div></div>')
    return "".join(parts)


def kpi_row(run: dict, prior: dict | None) -> str:
    o = run["glycemic"]["overall"]
    prev = prior["glycemic"]["overall"] if prior else None

    def delta_cue(key: str, higher_is_better: bool, unit: str = " pp") -> str | None:
        """Change vs. the previous run, as a glyph plus a signed number (not colour alone)."""
        if prev is None or o.get(key) is None or prev.get(key) is None:
            return None
        d = o[key] - prev[key]
        if abs(d) < 0.05:
            return f'<span class="muted">±0{unit}</span>'
        good = (d > 0) == higher_is_better
        arrow = "↑" if d > 0 else "↓"
        cls = "delta-good" if good else "delta-bad"
        return f'<span class="{cls}"><span aria-hidden="true">{arrow}</span> {num(abs(d), unit)}</span>'

    tiles = []
    for key, label, hib in (("tir", "Zeit im Zielbereich 70-180", True),
                            ("tbr_70", "Zeit unter 70", False),
                            ("tar_180", "Zeit über 180", False),
                            ("cv", "Variationskoeffizient", False)):
        target, rule = TARGETS[key]
        value = o.get(key)
        status = rule(value) if value is not None else None
        tiles.append(tile(label, num(value, " %"), target, status, delta_cue(key, hib)))
    tiles.append(tile("Mittelwert", num(o.get("mean"), " mg/dl"), None, None,
                      delta_cue("mean", False, " mg/dl")))
    tiles.append(tile("GMI (geschätztes HbA1c)", num(o.get("gmi"), " %"), None, None, None))
    cov = o.get("coverage_pct")
    tiles.append(tile("Datenabdeckung", num(cov, " %"), TARGETS["coverage_pct"][0],
                      TARGETS["coverage_pct"][1](cov) if cov is not None else None, None))
    tiles.append(tile("Zeit unter 54", num(o.get("tbr_54"), " %"), TARGETS["tbr_54"][0],
                      TARGETS["tbr_54"][1](o["tbr_54"]) if o.get("tbr_54") is not None else None))
    return f'<div class="tiles">{"".join(tiles)}</div>'


def header(run: dict) -> str:
    w, gly, ds = run["window"], run["glycemic"], run.get("snapshot", {})
    readings = f'{gly["overall"]["n_readings"]:,}'.replace(",", ".")   # German thousands
    lines = [
        f'<p class="lede">Fenster <strong>{e(w["start"][:10])} bis {e(w["as_of"])}</strong> '
        f'({w["weeks"]} Wochen, {readings} Messwerte), '
        f'Stand {e(run["generated_at"][:16].replace("T", " "))}</p>'
    ]
    warn = []
    if gly.get("missing_days"):
        warn.append(f'{len(gly["missing_days"])} Tage ohne Daten: '
                    + ", ".join(e(d) for d in gly["missing_days"][:5]))
    if gly.get("partial_days"):
        warn.append(f'{len(gly["partial_days"])} unvollständige Tage: '
                    + ", ".join(e(d) for d in gly["partial_days"][:5]))
    amb = (run.get("settings") or {}).get("ambiguous_blocks") or []
    if amb:
        warn.append("Analyse-Blöcke ohne eindeutiges konfiguriertes CR (überspannen zwei "
                    "Pumpenblöcke): " + ", ".join(e(b) for b in amb))
    for f in ds.get("active_flags", []):
        if f.startswith("no_delivery:"):
            warn.append("Verdacht auf Insulin-Abgabestörung: " + e(f.split(":", 1)[1]))
    body = "".join(f'<li>{w_}</li>' for w_ in warn)
    if body:
        lines.append(f'<div class="notice"><strong>Datenqualität und Vorbehalte</strong>'
                     f'<ul>{body}</ul></div>')
    return "".join(lines)


def proposal_card(p: dict, ev: dict | None) -> str:
    arrow = {"up": "↑", "down": "↓"}.get(p["direction"], "→")
    head = (f'<span class="mono">{e(p["parameter"])}</span> '
            f'<strong>{e(block_label(ev, p["block"]))}</strong> '
            f'<span class="change">{num(p["current_value"])} {arrow} '
            f'{num(p["proposed_value"])}</span>')
    meta = [f'Sicherheit: {e(p["confidence"])}']
    if p.get("schedule_block"):
        where = f'Pumpenblock {e(p["schedule_block"])}'
        if p.get("also_affects"):
            where += " (betrifft auch " + ", ".join(e(b) for b in p["also_affects"]) + ")"
        meta.append(where)
    if ev and ev.get("n_times_proposed_before"):
        n = ev["n_times_proposed_before"] + 1
        tail = ", bisher nicht umgesetzt" if ev.get("unapplied_streak") else ""
        meta.append(f'{n}. Mal vorgeschlagen{tail}')
    return (f'<div class="prop"><div class="prop-head">{head}</div>'
            f'<div class="prop-meta">{" · ".join(meta)}</div>'
            f'<p>{e(p["rationale"])}</p>'
            + (f'<p class="caveat"><span aria-hidden="true">⚠</span> {e(p["caveats"])}</p>'
               if p.get("caveats") else "")
            + "</div>")


def block_label(ev: dict | None, key: str) -> str:
    de = {"00-06": "Nacht", "06-11": "Frühstück", "11-15": "Mittag",
          "15-18": "Nachmittag", "18-22": "Abend", "22-24": "Spät"}
    return f'{de.get(key, key)} ({key})'


def recommendation_section(run: dict) -> str:
    rec = run["recommendation"]
    ev_by_block = {b["block"]: b for b in run["snapshot"]["blocks"]}
    changes = [p for p in rec["proposals"] if p["direction"] in ("up", "down")]
    holds = [p for p in rec["proposals"] if p["direction"] not in ("up", "down")]
    out = [f'<p class="narrative">{e(rec["overall_narrative"])}</p>']
    out.append('<h3>Jetzt umsetzen</h3>')
    out.append("".join(proposal_card(p, ev_by_block.get(p["block"])) for p in changes)
               if changes else '<p class="muted">Keine Änderung mit ausreichender Evidenz.</p>')
    if holds:
        out.append('<h3>Nur beobachten (keine Änderung)</h3>')
        out.append("".join(proposal_card(p, ev_by_block.get(p["block"])) for p in holds))
    if rec.get("insufficient_data_blocks"):
        blocks = ", ".join(block_label(None, b) for b in rec["insufficient_data_blocks"])
        out.append(f'<p class="muted">Zu wenig Daten für: {e(blocks)}</p>')
    return "".join(out)


def patch_section(runs_dir: str, run: dict) -> str:
    path = os.path.join(runs_dir, run["as_of"], "settings_patch.json")
    if not os.path.exists(path):
        return ""
    with open(path, encoding="utf-8") as f:
        text = f.read()
    return (f'<h3>settings.json ergänzen (nach dem Eintragen in der Pumpe)</h3>'
            f'<pre class="code">{e(text)}</pre>')


def loss_section(run: dict) -> str:
    verdicts = sorted(run.get("verdicts", []), key=lambda v: v.get("loss_pp") or 0, reverse=True)
    if not verdicts:
        return ""
    top = max((v.get("loss_pp") or 0) for v in verdicts) or 1
    rows = []
    for v in verdicts:
        loss = v.get("loss_pp") or 0
        width = max(2.0, 76.0 * loss / top)   # leave room for the value label
        status = VERDICT_STATUS.get(v["verdict"], "warning")
        rows.append(
            '<tr>'
            f'<th scope="row">{e(v["label"])} <span class="muted">{e(v["block"])}</span></th>'
            f'<td class="n">{num(v.get("share_pct"), " %")}</td>'
            f'<td class="n">{num(v.get("tir"), " %")}</td>'
            f'<td class="bar-cell"><span class="bar" style="width:{width:.1f}%" '
            f'title="{num(loss)} pp Verlust, {num(v.get("loss_share_pct"), " %")} des '
            f'Gesamtverlusts"></span><span class="bar-label">{num(loss)} pp</span></td>'
            f'<td class="n">{num(v.get("loss_share_pct"), " %")}</td>'
            f'<td>{e(DOM_DE.get(v.get("dominant") or "", "–"))}</td>'
            f'<td>{badge(status, VERDICT_DE.get(v["verdict"], v["verdict"]))}</td>'
            '</tr>')
    notes = "".join(f'<li><strong>{e(v["label"])}:</strong> {e(v["note"])}</li>'
                    for v in verdicts if v["verdict"] in ("watch", "no_cr_lever"))
    return (
        '<table class="grid"><caption>Verlust = Zeitanteil des Blocks × Anteil außerhalb '
        '70-180, also Prozentpunkte der Gesamt-TIR, die in diesem Block verloren gehen.</caption>'
        '<thead><tr><th scope="col">Block</th><th scope="col">Zeitanteil</th>'
        '<th scope="col">TIR</th><th scope="col" class="l">Verlust</th><th scope="col">Anteil</th>'
        '<th scope="col" class="l">Richtung</th><th scope="col" class="l">Bewertung</th></tr></thead>'
        f'<tbody>{"".join(rows)}</tbody></table>'
        + (f'<ul class="notes">{notes}</ul>' if notes else ""))


def block_table(run: dict) -> str:
    rows = []
    for ev in run["snapshot"]["blocks"]:
        rows.append(
            '<tr>'
            f'<th scope="row">{e(block_label(ev, ev["block"]))}</th>'
            f'<td class="n">{num(ev.get("tir"), " %")}</td>'
            f'<td class="n">{num(ev.get("tbr_70"), " %")}</td>'
            f'<td class="n">{num(ev.get("tar_180"), " %")}</td>'
            f'<td class="n">{num(ev.get("effective_cr"), "", 2)}</td>'
            f'<td class="n">{num(ev.get("configured_cr"), "", 1)}</td>'
            f'<td class="n">{num(ev.get("median_peak_rise"))}</td>'
            f'<td class="n">{num(ev.get("pct_post_meal_hypo"), " %")}</td>'
            f'<td class="n">{num(ev.get("pct_post_meal_dip"), " %")}</td>'
            f'<td class="n">{ev.get("n_clean_meals", 0)}</td>'
            f'<td class="n">{num(ev.get("coverage_pct"), " %")}</td>'
            '</tr>')
    return (
        '<table class="grid"><caption>Hypo-E. = Mahlzeiten mit echtem Ereignis '
        '(mindestens 15 min unter 70, Nadir mindestens 5 mg/dl darunter). Dips = jede kurze '
        'Unterschreitung. Viele Dips bei kleinem Wert in der Spalte &lt;70 heißt: kurze '
        'Einbrüche, nicht zu viel Mahlzeit-Insulin.</caption>'
        '<thead><tr><th scope="col">Block</th><th scope="col">TIR</th><th scope="col">&lt;70</th>'
        '<th scope="col">&gt;180</th><th scope="col">eff. CR</th><th scope="col">konf. CR</th>'
        '<th scope="col">Anstieg</th><th scope="col">Hypo-E.</th><th scope="col">Dips</th>'
        '<th scope="col">Mahlz.</th><th scope="col">Abdeckung</th></tr></thead>'
        f'<tbody>{"".join(rows)}</tbody></table>')


def charts_section(runs_dir: str, run: dict) -> str:
    out = []
    for name, title, caption in CHARTS:
        src = embed_png(os.path.join(runs_dir, run["as_of"], "charts", name))
        if src is None:
            continue
        out.append(f'<figure><img src="{src}" alt="{e(title)}">'
                   f'<figcaption><strong>{e(title)}</strong> {e(caption)}</figcaption></figure>')
    return f'<div class="figures">{"".join(out)}</div>' if out else ""


def history_section(runs: list[dict]) -> str:
    rows = []
    for r in runs:
        o = r["glycemic"]["overall"]
        props = []
        for p in r["recommendation"]["proposals"]:
            if p["direction"] in ("up", "down"):
                arrow = "↑" if p["direction"] == "up" else "↓"
                props.append(f'{p["parameter"]} {p["block"]} {num(p["current_value"])}'
                             f'{arrow}{num(p["proposed_value"])}')
        rows.append(
            '<tr>'
            f'<th scope="row">{e(r["as_of"])}</th>'
            f'<td class="n">{num(o.get("tir"), " %")}</td>'
            f'<td class="n">{num(o.get("tbr_70"), " %")}</td>'
            f'<td class="n">{num(o.get("tar_180"), " %")}</td>'
            f'<td class="n">{num(o.get("cv"), " %")}</td>'
            f'<td class="n">{num(o.get("coverage_pct"), " %")}</td>'
            f'<td>{e(", ".join(props)) or "<span class=\'muted\'>keine</span>"}</td>'
            '</tr>')
    table = ('<table class="grid"><thead><tr><th scope="col">Lauf (Stand)</th>'
             '<th scope="col">TIR</th><th scope="col">&lt;70</th><th scope="col">&gt;180</th>'
             '<th scope="col">CV</th><th scope="col">Abdeckung</th>'
             '<th scope="col" class="l">Änderungsvorschlag</th></tr></thead>'
             f'<tbody>{"".join(rows)}</tbody></table>')

    cards = []
    for r in reversed(runs):
        o = r["glycemic"]["overall"]
        cards.append(
            f'<details class="run"{" open" if r is runs[-1] else ""}>'
            f'<summary><strong>{e(r["as_of"])}</strong> '
            f'<span class="muted">TIR {num(o.get("tir"), " %")}, '
            f'&lt;70 {num(o.get("tbr_70"), " %")}, '
            f'Abdeckung {num(o.get("coverage_pct"), " %")}</span></summary>'
            f'<p class="narrative">{e(r["recommendation"]["overall_narrative"])}</p></details>')
    return table + '<h3>Bewertung des Sprachmodells je Lauf</h3>' + "".join(cards)


def loop_section(run: dict) -> str:
    bt = run.get("backtest") or {}
    per_block = bt.get("per_block") or {}
    out = []
    if per_block:
        applied_de = {True: "ja", False: "nein", None: "unbekannt"}
        outcome_de = {"improved": "verbessert", "worsened": "verschlechtert",
                      "unchanged": "unverändert", "unknown": "unbekannt"}
        rows = "".join(
            '<tr>'
            f'<th scope="row">{e(b.get("parameter", ""))} {e(block_label(None, k))}</th>'
            f'<td>{e(applied_de[b.get("applied")])}</td>'
            f'<td>{e(outcome_de.get(b.get("outcome"), b.get("outcome")))}</td>'
            f'<td class="n">{num(b.get("tir_before"), " %")} → {num(b.get("tir_after"), " %")}</td>'
            '</tr>' for k, b in per_block.items())
        out.append(f'<h3>Ergebnis der Empfehlungen vom {e(bt.get("prior_run_date") or "–")}</h3>'
                   '<table class="grid"><thead><tr><th scope="col">Vorschlag</th>'
                   '<th scope="col" class="l">umgesetzt</th><th scope="col" class="l">Wirkung auf Block-TIR</th>'
                   f'<th scope="col">TIR vorher → nachher</th></tr></thead><tbody>{rows}</tbody></table>')
    audit = run.get("clamp_audit") or []
    if audit:
        items = "".join(
            f'<li><span class="mono">{e(a["parameter"])} {e(a["block"])}</span> '
            f'{e(AUDIT_FIELD_DE.get(a["field"], a["field"]))}: '
            f'{e(a["original"]) if a["original"] not in ("None", None) else "–"} → '
            f'{e(a["adjusted"])} '
            f'<span class="muted">({e(a["reason"])})</span></li>' for a in audit)
        out.append(f'<h3>Sicherheitsbegrenzung angewendet</h3><ul class="notes">{items}</ul>')
    return "".join(out)


CAVEATS = [
    "CR und CF sind aus den Daten abgeleitet, nicht aus den Geräteeinstellungen gelesen.",
    "Das effektive CR kann eine vom Loop hinzugefügte Korrektur enthalten.",
    "Korrekturfaktor-Aussagen sind im Closed Loop grundsätzlich unsicher.",
    "Bei vermuteter Insulin-Abgabestörung werden betroffene Werte aus der CR-Schätzung "
    "ausgeschlossen; IOB nutzt die konfigurierte Insulinwirkdauer.",
    "Jede Änderung ist auf ±10 % pro Lauf begrenzt; klein und schrittweise umsetzen.",
    "Entscheidungshilfe zur Besprechung mit dem Behandlungsteam, keine ärztliche Anweisung.",
]

STYLE = """
:root {
  --page: #f9f9f7; --surface: #fcfcfb; --ink: #0b0b0b; --ink-2: #52514e;
  --muted: #898781; --grid: #e1e0d9; --axis: #c3c2b7; --accent: #2a78d6;
  --good: #0ca30c; --warning: #fab219; --critical: #d03b3b; --good-text: #006300;
  --border: rgba(11,11,11,0.10); --bar: #86b6ef;
  color-scheme: light;
}
@media (prefers-color-scheme: dark) {
  :root:where(:not([data-theme="light"])) {
    --page: #0d0d0d; --surface: #1a1a19; --ink: #ffffff; --ink-2: #c3c2b7;
    --muted: #898781; --grid: #2c2c2a; --axis: #383835; --accent: #3987e5;
    --good: #0ca30c; --good-text: #0ca30c; --border: rgba(255,255,255,0.10);
    --bar: #256abf; color-scheme: dark;
  }
}
:root[data-theme="dark"] {
  --page: #0d0d0d; --surface: #1a1a19; --ink: #ffffff; --ink-2: #c3c2b7;
  --muted: #898781; --grid: #2c2c2a; --axis: #383835; --accent: #3987e5;
  --good: #0ca30c; --good-text: #0ca30c; --border: rgba(255,255,255,0.10);
  --bar: #256abf; color-scheme: dark;
}
* { box-sizing: border-box; }
body { margin: 0; padding: 32px 20px 64px; background: var(--page); color: var(--ink);
  font: 15px/1.55 system-ui, -apple-system, "Segoe UI", sans-serif; }
main { max-width: 1080px; margin: 0 auto; }
h1 { font-size: 26px; margin: 0 0 4px; letter-spacing: -0.01em; }
h2 { font-size: 19px; margin: 0 0 14px; }
h3 { font-size: 15px; margin: 22px 0 8px; }
p { margin: 0 0 10px; }
.lede { color: var(--ink-2); }
.muted { color: var(--muted); }
.mono { font-family: ui-monospace, "SF Mono", Menlo, monospace; font-size: 0.92em; }
section { background: var(--surface); border: 1px solid var(--border); border-radius: 10px;
  padding: 20px 22px; margin: 18px 0; }
.topbar { display: flex; justify-content: space-between; align-items: baseline; gap: 16px; }
button.theme { background: none; border: 1px solid var(--border); border-radius: 6px;
  color: var(--ink-2); padding: 4px 10px; font: inherit; font-size: 13px; cursor: pointer; }
.notice { border-left: 3px solid var(--warning); background: color-mix(in srgb, var(--warning) 8%, transparent);
  padding: 10px 14px; border-radius: 0 6px 6px 0; margin: 12px 0 0; }
.notice ul { margin: 6px 0 0; padding-left: 18px; }
.tiles { display: grid; gap: 12px; grid-template-columns: repeat(auto-fit, minmax(190px, 1fr)); }
.tile { border: 1px solid var(--border); border-radius: 8px; padding: 12px 14px; }
.tile-label { font-size: 12px; color: var(--ink-2); min-height: 30px; }
.tile-value { font-size: 27px; font-weight: 600; letter-spacing: -0.02em; margin: 2px 0 6px; }
.tile-foot { display: flex; flex-wrap: wrap; gap: 8px; align-items: center; font-size: 12px; }
.badge { display: inline-flex; align-items: center; gap: 4px; font-size: 12px; font-weight: 500;
  padding: 1px 7px; border-radius: 999px; border: 1px solid; }
.badge.good { color: var(--good-text); border-color: var(--good); }
.badge.warning { color: var(--ink); border-color: var(--warning);
  background: color-mix(in srgb, var(--warning) 14%, transparent); }
.badge.critical { color: var(--critical); border-color: var(--critical); }
.badge.action { color: var(--accent); border-color: var(--accent); font-weight: 600; }
.delta-good { color: var(--good-text); font-weight: 500; }
.delta-bad { color: var(--critical); font-weight: 500; }
table.grid { width: 100%; border-collapse: collapse; font-variant-numeric: tabular-nums;
  font-size: 13.5px; margin: 6px 0 4px; }
table.grid caption { caption-side: bottom; text-align: left; color: var(--muted);
  font-size: 12px; padding-top: 8px; }
table.grid th[scope=col] { text-align: right; font-weight: 500; color: var(--ink-2);
  font-size: 12px; border-bottom: 1px solid var(--axis); padding: 4px 8px; }
table.grid th[scope=col]:first-child, table.grid th[scope=row], table.grid th.l { text-align: left; }
table.grid th[scope=row] { font-weight: 500; padding: 5px 8px; white-space: nowrap; }
table.grid td { padding: 5px 8px; border-bottom: 1px solid var(--grid); }
table.grid td.n { text-align: right; }
table.grid tbody tr:hover { background: color-mix(in srgb, var(--accent) 6%, transparent); }
.bar-cell { min-width: 170px; white-space: nowrap; padding-right: 16px; }
.bar { display: inline-block; height: 9px; background: var(--bar); border-radius: 0 4px 4px 0;
  vertical-align: middle; margin-right: 8px; }
.bar-label { font-size: 12px; color: var(--ink-2); }
.prop { border: 1px solid var(--border); border-left: 3px solid var(--accent);
  border-radius: 0 8px 8px 0; padding: 10px 14px; margin: 0 0 10px; }
.prop-head { font-size: 15px; }
.prop-head .change { font-weight: 600; }
.prop-meta { font-size: 12px; color: var(--muted); margin: 2px 0 6px; }
.prop p { margin: 0 0 6px; font-size: 14px; }
.caveat { color: var(--ink-2); }
.narrative { font-size: 14.5px; color: var(--ink-2); }
.notes { margin: 4px 0 0; padding-left: 18px; font-size: 13.5px; }
.notes li { margin-bottom: 4px; }
pre.code { background: var(--page); border: 1px solid var(--border); border-radius: 8px;
  padding: 12px 14px; overflow-x: auto; font-size: 12.5px; }
.figures { display: grid; gap: 20px; grid-template-columns: repeat(auto-fit, minmax(420px, 1fr)); }
figure { margin: 0; }
figure img { width: 100%; height: auto; border: 1px solid var(--border); border-radius: 8px;
  background: #ffffff; }
figcaption { font-size: 12.5px; color: var(--muted); margin-top: 6px; }
details.run { border-top: 1px solid var(--grid); padding: 10px 0; }
details.run summary { cursor: pointer; font-size: 14px; }
footer { color: var(--muted); font-size: 12.5px; max-width: 1080px; margin: 0 auto; }
footer ul { padding-left: 18px; }
@media print { body { background: #fff; } section { break-inside: avoid; } button.theme { display: none; } }
"""

SCRIPT = """
const b = document.documentElement, t = document.querySelector('button.theme');
t.addEventListener('click', () => {
  const dark = getComputedStyle(b).getPropertyValue('color-scheme').trim() === 'dark';
  b.dataset.theme = dark ? 'light' : 'dark';
});
"""


def render(runs: list[dict], runs_dir: str) -> str:
    latest = runs[-1]
    prior = runs[-2] if len(runs) > 1 else None
    caveats = "".join(f"<li>{e(c)}</li>" for c in CAVEATS)
    return f"""<!DOCTYPE html>
<html lang="de">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>tir-tuner Auswertung {e(latest['as_of'])}</title>
<style>{STYLE}</style>
</head>
<body>
<main>
  <div class="topbar">
    <div>
      <h1>tir-tuner: Auswertung {e(latest['as_of'])}</h1>
      {header(latest)}
    </div>
    <button class="theme" type="button">Hell / Dunkel</button>
  </div>

  <section>
    <h2>Status im aktuellen Fenster</h2>
    {kpi_row(latest, prior)}
  </section>

  <section>
    <h2>Empfehlung des Sprachmodells</h2>
    {recommendation_section(latest)}
    {patch_section(runs_dir, latest)}
  </section>

  <section>
    <h2>Wo die TIR verloren geht</h2>
    {loss_section(latest)}
  </section>

  <section>
    <h2>Kennzahlen nach Tagesblock</h2>
    {block_table(latest)}
  </section>

  <section>
    <h2>Diagramme</h2>
    {charts_section(runs_dir, latest)}
  </section>

  <section>
    <h2>Verlauf über die Läufe</h2>
    {history_section(runs)}
  </section>

  <section>
    <h2>Rückblick und Sicherheitsnetz</h2>
    {loop_section(latest)}
  </section>
</main>
<footer>
  <p><strong>Hinweise</strong></p>
  <ul>{caveats}</ul>
</footer>
<script>{SCRIPT}</script>
</body>
</html>
"""


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("runs_dir", nargs="?", default="runs", help="run directory (default: runs)")
    ap.add_argument("-o", "--out", default=None, help="output file (default: <runs_dir>/report.html)")
    args = ap.parse_args(argv)

    runs = load_runs(args.runs_dir)
    if not runs:
        print(f"no result.json under {args.runs_dir}/*/", file=sys.stderr)
        return 1
    out = args.out or os.path.join(args.runs_dir, "report.html")
    with open(out, "w", encoding="utf-8") as f:
        f.write(render(runs, args.runs_dir))
    size_kb = os.path.getsize(out) / 1024
    print(f"{out} ({size_kb:.0f} kB, {len(runs)} Läufe, "
          f"aktuellster Stand {runs[-1]['as_of']}, erzeugt {dt.date.today().isoformat()})")
    return 0


if __name__ == "__main__":
    sys.exit(main())

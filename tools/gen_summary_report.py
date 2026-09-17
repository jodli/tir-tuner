#!/usr/bin/env python3
"""Generate docs/verification_summary.html, the plain-language algorithm page.

A simple page that explains how the algorithm works and how its parts connect,
with mermaid diagrams. The recurring numbers are still extracted from src/, so
the page cannot drift quietly from the constants the code actually uses.

Usage: python3 tools/gen_summary_report.py
"""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from gen_verification_report import (  # noqa: E402
    CANONICAL_ORDER,
    REPO,
    SRC,
    esc,
    parse_rust,
)

OUT = REPO / "docs" / "verification_summary.html"

def mm_model() -> str:
    return """flowchart TD
    IN[Insulin infusion basal + bolus in U/h] --> I1[i1 depot]
    IN --> I2[i2 depot]
    I1 --> IP["plasma insulin i(t) in mU/L"]
    I2 --> IP
    IP --> RD[r_d: remote action, drains glucose]
    IP --> RE[r_e: remote action, shapes the liver]
    M[Meals: carbohydrate in g/min] --> A1[a1 gut depot]
    A1 --> GU[gut uptake u_A]
    GU --> Q1[q1 glucose pool, plasma g_P]
    E[EGP: liver adds glucose] --> Q1
    RE -. controls .-> E
    Q1 --> RD
    Q1 --> F01[F01: fixed drain]
    Q1 <--> Q2[q2: tissue stores]
    Q1 --> Q3[q3 interstitial, sensor g_IG]
    Q3 --> C[CGM: the reading]
"""


def mm_gates() -> str:
    return """flowchart LR
    C[CGM reading g_IG] --> H{below 4.4? the hard hypo cutoff}
    H -- yes --> Z[deliver 0 U/h]
    H -- no --> M[mode decides: Standard, Ease-off, Boost]
    M --> U[cap at u_max, never above the max rate]
    U --> P[delivery rate in U/h]
    M -. Ease-off extra gate .-> E[full suspension below 7.0]
    M -. Boost .-> B[plus 35%, but never below Standard]
"""


def mm_minute() -> str:
    return """sequenceDiagram
    participant CGM as Sensor g_IG
    participant C as Controller
    participant N as NMPC grid
    participant G as Cutoff and clamp
    participant P as Pump
    CGM->>C: reading in mmol/L
    C->>N: candidates, target, mode
    N->>N: one-step cost of each candidate
    N->>G: best rate
    G->>P: 0 below 4.4, else capped at u_max
    P-->>CGM: insulin arrives, next reading
    Note over P,CGM: repeat every minute
"""


def mm_imm() -> str:
    return """sequenceDiagram
    participant F as three filters, mu
    participant M as mixing
    participant T as Markov transitions
    participant B as Bayes update
    participant O as blended output
    F->>M: mode probabilities
    M->>T: prognostic weights c_j
    T->>B: likelihood weighting
    B->>O: normalized posterior
    O-->>F: feeds the next blend
"""


def build_const_chips(items_by_file: dict) -> str:
    wanted = {
        "TARGET_GLUCOSE_MMOL_L": "target glucose",
        "TARGET_RANGE_MIN_MMOL_L": "range low",
        "TARGET_RANGE_MAX_MMOL_L": "range high",
        "HARD_HYPO_CUTOFF_MMOL_L": "hard hypo cutoff",
        "EASE_OFF_TARGET_MMOL_L": "ease-off target",
        "BOOST_DELIVERY_FACTOR": "boost factor",
        "TIME_IN_RANGE_MIN_MMOL_L": "TIR low",
        "TIME_IN_RANGE_MAX_MMOL_L": "TIR high",
    }
    consts = {}
    for rel in CANONICAL_ORDER:
        for it in items_by_file[rel]:
            if it["kind"] == "const":
                consts[it["name"]] = it["value"]
    chips = []
    for key, label in wanted.items():
        if key in consts:
            chips.append(
                f'<div class="chip2"><div class="chip2v">{esc(consts[key])}</div><div class="chip2l">{label}</div></div>'
            )
    return "".join(chips)


def main():
    items = {rel: parse_rust(SRC / rel.split("/")[-1]) for rel in CANONICAL_ORDER}
    const_chips = build_const_chips(items)

    css = """
:root { --ink:#1e293b; --line:#cbd5e1; }
* { box-sizing: border-box; }
body { margin:0; font-family:-apple-system,"Segoe UI",Roboto,Helvetica,Arial,sans-serif; color:var(--ink); line-height:1.5; }
main { max-width:56rem; margin:0 auto; padding:2rem 1.5rem 4rem; }
h1 { font-size:1.7rem; margin:0 0 .2rem; }
h2 { font-size:1.25rem; margin:2.4rem 0 .5rem; color:#0f172a; }
p { margin:.5rem 0; }
code { font-family:ui-monospace,SFMono-Regular,Menlo,Consolas,monospace; font-size:.88em;
       background:#f1f5f9; padding:.08em .3em; border-radius:4px; }
.mermaid { display:flex; justify-content:center; margin:.4rem 0; }
.mermaid svg { max-width:100%; height:auto; }
.caption { font-size:.9rem; color:#64748b; margin:.2rem 0 1.2rem; }
.lead { font-size:1.02rem; color:#334155; max-width:52ch; }
.chip2 { flex:1 1 7rem; text-align:center; background:#f8fafc; border:1px solid var(--line); border-radius:8px; padding:.5rem .3rem; }
.chip2v { font-family:ui-monospace,Monaco,monospace; font-size:1.15rem; color:#0f172a; }
.chip2l { font-size:.74rem; color:#64748b; }
.chiprow { display:flex; flex-wrap:wrap; gap:.6rem; margin:.8rem 0; }
.tag { font-size:.72rem; font-weight:700; letter-spacing:.06em; text-transform:uppercase; color:#7c3aed; }
footer { margin-top:3.5rem; border-top:1px solid var(--line); padding-top:.8rem; font-size:.82rem; color:#64748b; }
"""

    page = f"""<!DOCTYPE html>
<html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>How the CamAPS FX algorithm works</title>
<style>{css}</style>
<script src="https://cdn.jsdelivr.net/npm/mermaid@11.4.1/dist/mermaid.min.js"></script>
<script>mermaid.initialize({{ startOnLoad: true, theme: "base" }});</script></head>
<body><main>

<h1>How the CamAPS FX algorithm works</h1>
<p class="lead">CamAPS FX is a closed-loop insulin system, an artificial pancreas. Every minute it reads a
glucose sensor, works out how much insulin the body needs, and double-checks that number before the pump
delivers it. This page walks through that minute: how the algorithm sees the body, how it decides, and what
it locks down before a single unit of insulin is pumped.</p>

<h2>Start here: one minute in the loop</h2>
<p>Everything happens once a minute, in the same order:</p>
<pre class="mermaid">{mm_minute()}</pre>
<p class="caption">Sense, propose, check, deliver, repeat. The sensor reports a reading, the controller
proposes a rate, the guard checks it and fixes anything wrong, the pump delivers. The next three sections
introduce the actors one by one.</p>

<h2>The body it measures insulin around</h2>
<p>The algorithm carries a model of how glucose moves in the body. Insulin enters through two depots and
reaches the plasma; meals enter through the gut. Insulin does not act instantly: it works through two slow
remote actions, one that draws glucose out of the pool and one that shapes how much glucose the liver adds.
Meals add glucose through the gut, the liver adds its own, and every inflow and drain lands in the same
glucose pool. A small sensor compartment sits next to this pool, and that compartment is what the CGM reading
shows.</p>
<pre class="mermaid">{mm_model()}</pre>
<p class="caption">The arrows are the model's clock. Nothing happens instantly: insulin and meals need minutes
to take effect, and every step hands its result on to the next.</p>

<h2>The brain: three opinions, one answer</h2>
<p>No single model fits every body, so the algorithm runs three mode filters in parallel and blends them into
one estimate. Each minute the previous probabilities are projected forward, reweighted by how well each mode
explained the newest reading, and normalized. The blend is the estimate that drives the dose decision.</p>
<pre class="mermaid">{mm_imm()}</pre>
<p class="caption">The algorithm keeps trusting whichever mode keeps explaining the readings best, and the
blend keeps counting for more.</p>

<h2>The watchdog before every dose</h2>
<p>Even after the controller has decided on a rate, nothing is delivered until it passes the watchdog. Below
the hard hypo cutoff the answer is always 0. The active mode sets the behavior: Standard, Ease-off, which
suspends fully below 7.0, or Boost, which is plus 35 percent but never below what Standard would give. And no
delivery ever exceeds the maximum rate.</p>
<pre class="mermaid">{mm_gates()}</pre>
<p class="caption">The controller proposes, the watchdog disposes. Every delivery is checked before the pump
moves.</p>

<div class="tag">the numbers at a glance</div>
<div class="chiprow">{const_chips}</div>

<p>That is the whole loop: sense, estimate, propose, guard, deliver, repeat, every minute of the day. The
loop is small and fixed, which is exactly why its safety is provable. The claim-by-claim proof lives in the
<a href="verification_report.html">verification catalogue</a>.</p>

<footer>Regenerate with <code>python3 tools/gen_summary_report.py</code>. The diagrams draw at open time from
a CDN; offline, the plain source of each diagram stays readable. The proofs and tests behind this page live in
<code>docs/verification_report.html</code>.</footer>
</main></body></html>"""

    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(page, encoding="utf-8")
    print(f"wrote {OUT}")


if __name__ == "__main__":
    main()
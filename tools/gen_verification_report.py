#!/usr/bin/env python3
"""Generate docs/verification_report.html from the workspace crate sources.

The report documents what the verification suite checks and where each
claim traces back to the literature and the patent. It is extracted
from the Rust sources (harness doc comments, kani::assert messages,
native test docs, constants) so it stays in sync with the suite. It is
NOT a test-results report: nothing here runs the verifiers.

Usage: python3 tools/gen_verification_report.py
"""

import html
import re
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
OUT = REPO / "docs" / "verification_report.html"

# Crate -> source files (relative to the crate root), display order.
CRATE_FILES = {
    "tir-tuner-aps": ["lib.rs", "hovorka.rs", "controller.rs", "imm.rs"],
    "tir-tuner-common": ["lib.rs", "units.rs", "euler.rs", "metrics.rs", "random.rs"],
    "tir-tuner-body": ["lib.rs", "subject.rs", "state.rs", "derivative.rs", "solver.rs"],
    "tir-tuner-cgm": ["lib.rs", "device.rs"],
}

# Keys like "tir-tuner-aps/src/lib.rs", used everywhere as display and
# source-link references.
CANONICAL_ORDER = sorted(
    f"{crate}/src/{f}" for crate, files in CRATE_FILES.items() for f in files
)

# The Kani suites, one verification module per crate.
VERIFICATION_FILES = sorted(f"{crate}/src/verification.rs" for crate in CRATE_FILES)


def esc(text: str) -> str:
    return html.escape(text, quote=False)


def inline(text: str) -> str:
    """Escape, then render `code` spans as <code>."""
    return re.sub(r"`([^`]+)`", r"<code>\1</code>", esc(text))


def parse_rust(path: Path) -> list[dict]:
    """Return source items of interest in file order.

    Items carry ({kind, name, docs, value, line}). Module docs (`//!`)
    come back as kind='module'. The file's leading `//!` block is its
    own module-level doc even without a `pub mod` declaration, so a
    crate's lib.rs and a suite's verification.rs each yield one
    regardless of their item layout. kani::assert messages and
    kani::assume expressions are collected per harness by
    harness_claims().
    """
    lines = path.read_text(encoding="utf-8").splitlines()
    items: list[dict] = []
    file_docs: list[str] = []
    docs: list[str] = []
    pending = None  # None | 'kani' | 'test'

    def flush(kind, name, line, value=None, src=None):
        items.append(
            {
                "kind": kind,
                "name": name,
                "docs": " ".join(src if src is not None else docs).strip(),
                "value": value,
                "line": line,
            }
        )

    def flush_file_docs(line):
        if file_docs:
            flush("module", path.stem, line, src=file_docs)
            file_docs.clear()

    return_lines = lines
    i = 0
    while i < len(return_lines):
        raw = return_lines[i]
        s = raw.strip()

        if s.startswith("//!"):
            file_docs.append(s[3:].strip())
            i += 1
            continue

        # The `//!` block, if any, is flushed before any other line type
        # so it does not get glued onto a following `///` item or const.
        flush_file_docs(i + 1)

        if s.startswith("///"):
            docs.append(s[3:].strip())
            i += 1
            continue

        if s.startswith("#[kani::proof]"):
            pending = "kani"
            i += 1
            continue
        if s == "#[test]":
            pending = "test"
            i += 1
            continue
        if s.startswith("#[derive(") or s.startswith("#["):
            # Attributes between the doc comment and the declaration keep
            # the doc block pending.
            i += 1
            continue

        mn = re.match(r"^pub\s+mod\s+([A-Za-z_]\w*)", s)
        if mn:
            if docs or pending:
                flush("module", mn.group(1), i + 1)
                pending = None
                docs = []
            i += 1
            continue

        cn = re.match(r"^(?:pub\s+)?const\s+([A-Za-z_]\w*)\s*:", s)
        if cn:
            value = raw.split("=", 1)[1].strip() if "=" in raw else ""
            while not value.rstrip().endswith(";"):
                i += 1
                value = value + " " + return_lines[i].strip()
            flush("const", cn.group(1), i + 1, value.rstrip(";").strip())
            pending = None
            docs = []
            i += 1
            continue

        fn = re.match(r"^(?:pub\s+)?fn\s+([A-Za-z_]\w*)\s*\(", s)
        if fn and pending:
            flush("harness" if pending == "kani" else "native_test", fn.group(1), i + 1)
            pending = None
            docs = []
            i += 1
            continue

        fns = re.match(r"^pub\s+fn\s+([A-Za-z_]\w*)\s*\(", s)
        if fns:
            if docs:
                flush("pub_fn", fns.group(1), i + 1)
            pending = None
            docs = []
            i += 1
            continue

        st = re.match(r"^pub\s+struct\s+([A-Za-z_]\w*)", s)
        if st:
            if docs:
                flush("struct", st.group(1), i + 1)
            pending = None
            docs = []
            i += 1
            continue

        en = re.match(r"^pub\s+enum\s+([A-Za-z_]\w*)", s)
        if en:
            if docs:
                flush("enum", en.group(1), i + 1)
            pending = None
            docs = []
            i += 1
            continue

        # Structural lines reset any stale doc block.
        if re.match(r"^(use\s|impl\s|mod\s|\}|\)|else\s|#)", s) or s == "}":
            docs = []
            pending = None
        i += 1

    return items


def harness_claims(path: Path) -> dict:
    """Map harness name -> (asserts, assumes) extracted from its body.

    Every kani::assert / kani::assume in the file is attributed to the
    harness whose `pub fn` declaration precedes it. Claims are kept in
    line order: entries are (line_no, expression, message).
    """
    text = path.read_text(encoding="utf-8")
    decls = list(re.finditer(r"^pub fn (\w+)\(", text, re.MULTILINE))

    def line_of(match) -> int:
        return text[: match.start()].count("\n") + 1

    def owner(line_no: int):
        name = None
        for d in decls:
            if line_of(d) < line_no:
                name = d.group(1)
            else:
                break
        return name

    claims: dict = {}
    asserts = re.finditer(r'kani::assert\((.*?),\s*"([^"]*)"\s*,?\s*\)\s*;', text, re.DOTALL)
    for m in asserts:
        name = owner(line_of(m))
        if name:
            claims.setdefault(name, {"asserts": [], "assumes": []})["asserts"].append(
                (line_of(m), m.group(1).strip().replace("\n", " "), m.group(2))
            )
    assumes = re.finditer(r"kani::assume\(([^)]*)\)\s*;", text)
    for m in assumes:
        name = owner(line_of(m))
        if name:
            claims.setdefault(name, {"asserts": [], "assumes": []})["assumes"].append(
                (line_of(m), m.group(1).strip().replace("\n", " "), None)
            )
    return claims


FUZZ_TARGETS = [
    ("hovorka_step",
     "Arbitrary bounded f64 inputs clamped to the physiological domain (NaN / infinite inputs are skipped); "
     "after one forward-Euler step every compartment stays finite and non-negative.",
     "fuzz/fuzz_targets/hovorka_step.rs"),
    ("nmpc_grid_dose",
     "The grid dose stays in [0, u_max]; every candidate roll-out cost is finite and non-negative; "
     "the selected rate equals the min-cost candidate. The refined sequence stays in [0, u_max] and "
     "its full-sequence cost is finite and non-negative.",
     "fuzz/fuzz_targets/nmpc_grid_dose.rs"),
]


def src_link(path: str, line: int = 0) -> str:
    return f"{path}#L{line}" if line else path


def section(title: str, body: str) -> str:
    return f'<section><h2>{title}</h2>{body}</section>\n'


REFS = [
    ("W04", "Hovorka, R., Canonico, V., Chassin, L., Haueter, U., Massi-Benedetti, M., Orsini Federici, M., Pieber, T., Schaller, H., Schaupp, L., Vering, T., Wilinska, M. E. Nonlinear model predictive control of glucose concentration in subjects with type 1 diabetes. Physiological Measurement 2004;25(4):905-920.",
     "https://pubmed.ncbi.nlm.nih.gov/15382830/"),
    ("W10", "Wilinska, M. E., Chassin, L. J., Acerini, C. L., Allen, J. M., Dunger, D. B., Hovorka, R. Simulation Environment to Evaluate Closed-Loop Insulin Delivery Systems in Type 1 Diabetes. Journal of Diabetes Science and Technology 2010;4(1):132-144. PMC2825634.",
     "https://pmc.ncbi.nlm.nih.gov/articles/PMC2825634/"),
    ("BQ13", "Bequette, B. W. Algorithms for a Closed-Loop Artificial Pancreas: The Case for Model Predictive Control. Journal of Diabetes Science and Technology 2013;7(6):1632-1643. PMC3876342.",
     "https://pmc.ncbi.nlm.nih.gov/articles/PMC3876342/"),
    ("W22", "Ware, J., Boughton, C. K., Allen, J. M., Wilinska, M. E., Tauschmann, M., Denvir, L., Thankamony, A., Campbell, F. M., Wadwa, R. P., Buckingham, B. A., Davis, N., et al. Cambridge hybrid closed-loop algorithm in children and adolescents with type 1 diabetes: a multicentre 6-month randomised controlled trial. The Lancet Digital Health 2022;4(4):e245-e255.",
     "https://doi.org/10.1016/S2589-7500(22)00020-6"),
    ("W22B", "Ware, J., Wilinska, M. E., Ruan, Y., Allen, J. M., Boughton, C. K., Hartnell, S., et al. Safety of user-initiated intensification of insulin delivery using Cambridge hybrid closed-loop algorithm. Journal of Diabetes Science and Technology 2022;16(6):1435-1440.",
     "https://doi.org/10.1177/19322968221141924"),
    ("B26", "Boughton, C. K., Tong, M., Wilinska, M. E., Hartnell, S., Hovorka, R., et al. Real-world evidence on the CamAPS FX hybrid closed-loop system in people living with type 1 diabetes. Metabologia 2026;2.",
     "https://doi.org/10.1007/s44357-025-00002-2"),
    ("A23", "Alwan, H., et al. Real-World Evidence Analysis of a Hybrid Closed-Loop System. Journal of Diabetes Science and Technology 2023.",
     "https://doi.org/10.1177/19322968231185348"),
    ("F14", "Facchinetti, A., Del Favero, S., Sparacino, G., Castle, J. R., Ward, W. K., Cobelli, C. Modeling the Glucose Sensor Error. IEEE Transactions on Biomedical Engineering 2014;61(3):620-629.",
     "https://pubmed.ncbi.nlm.nih.gov/24108706/"),
    ("B08", "Breton, M., Kovatchev, B. Analysis, Modeling, and Simulation of the Accuracy of Continuous Glucose Sensors. Journal of Diabetes Science and Technology 2008;2(5):853-862. PMC2740661.",
     "https://pubmed.ncbi.nlm.nih.gov/19750186/"),
    ("P09", "Patek, S. D., Bequette, B. W., Breton, M., Buckingham, B. A., Dassau, E., Doyle, F. J. III, Lum, J., Magni, L., Zisser, H. In Silico Preclinical Trials: Methodology and Engineering Guide to Closed-Loop Control in Type 1 Diabetes Mellitus. Journal of Diabetes Science and Technology 2009;3(2):269-282. PMC2771529.",
     "https://pubmed.ncbi.nlm.nih.gov/20144358/"),
    ("CA2345", "Hovorka, R. (inventor). Substance Monitoring and Control in Human or Animal Bodies. Canadian patent CA2702345C, granted 2018, assigned to Cambridge Enterprise Ltd. (US family: US9402953B2, Glucose Monitoring and Control Using Multi-Model Approach.)",
     "https://patents.google.com/patent/CA2702345C/en"),
]

DOCUMENTS = [
    ("Verification blueprint", "docs/camaps_fx_kani_specification.md", "Model specification (section 3), verification suite blueprint (section 6) and data-science roadmap (section 7)."),
    ("Research report", "docs/Research_report_CamAPS_FX__Cambridge_Artificial_Pancreas_Alg.md", "Reading notes on the source papers and the patent, with corrections applied."),
]

# Constant name -> (reference keys, annotation). The extracted doc
# comment is shown in the table; this adds the citation and the role.
CONSTANT_NOTES = {
    "TARGET_GLUCOSE_MMOL_L": (["W22"], "Default glucose target; also the basal-equilibrium anchor tuned in hovorka.rs."),
    "TARGET_RANGE_MIN_MMOL_L": (["W22"], "User-adjustable target range."),
    "TARGET_RANGE_MAX_MMOL_L": (["W22"], "User-adjustable target range; also the upper bound of the Kani dose-domain slice."),
    "HARD_HYPO_CUTOFF_MMOL_L": ([], "Engineering safety invariant: mandatory zero delivery below this reading."),
    "EASE_OFF_TARGET_MMOL_L": (["W22"], "Ease-off exercise target and suspension threshold."),
    "BOOST_DELIVERY_FACTOR": (["W22", "W22B"], "User-initiated intensification factor."),
    "DOSE_GAIN_U_H_PER_MMOL_L": ([], "Illustrative internal stand-in for the one-step grid NMPC (spec section 5.1); not a published CamAPS parameter."),
    "TIME_IN_RANGE_MIN_MMOL_L": (["B26", "A23"], "TIR band lower bound (3.9 mmol/L)."),
    "TIME_IN_RANGE_MAX_MMOL_L": (["B26", "A23"], "TIR band upper bound (10.0 mmol/L)."),
    "SENSOR_NOISE_SD_MMOL_L": (["W10"], "In-silico scenario noise."),
    "NMPC_GRID_STEPS": ([], "Internal grid resolution, spec section 5.1."),
    "NMPC_GRID_POINTS": ([], "Internal: STEPS + 1, including the zero rate."),
    "IMM_MODE_COUNT": (["CA2345"], "Number of parallel modes (N = 3)."),
    "IMM_PROBABILITY_SUM_TOLERANCE": ([], "Accepted deviation of the normalized mixture sum from 1.0."),
    "IMM_MARKOV_TRANSITION": (["CA2345"], "Matrix structure (column-stochastic, diagonal-dominant) from the patent; entries are illustrative tuning values."),
    "EGP_MAX_FOLD_OVER_BASAL": ([], "Crate safety cap on the low-insulin EGP branch; documented in spec section 3.2D."),
    "MU_PER_UNIT": (["W04"], "mU-per-U convention of the insulin mass state."),
}

MODEL_PARAM_DEFAULTS = [
    ("t_max, I", "55.0", "min", ["W04"], "Time-to-peak subcutaneous insulin absorption."),
    ("t_max, G", "40.0", "min", ["W04"], "Time-to-peak gut glucose absorption."),
    ("MCR_I", "0.021", "L/kg/min", ["W04"], "Metabolic clearance rate of insulin."),
    ("weight", "70", "kg", [], "Adult reference body weight."),
    ("p2_D", "0.02", "/min", ["W04"], "Fractional disappearance of remote disposal action."),
    ("p2_E", "0.011", "/min", ["W04"], "Fractional disappearance of remote EGP action."),
    ("k12 = k21", "0.066", "/min", ["W04"], "Inter-compartmental glucose transfer."),
    ("k31", "0.01", "/min", ["W04"], "Interstitial transport to the CGM compartment."),
    ("V_G", "0.16", "L/kg", ["W04"], "Glucose distribution volume."),
    ("F01", "0.01", "mmol/kg/min", ["W04"], "Non-insulin dependent utilization; applied through the glucose-dependent Michaelis-Menten form F01/0.85*g_P/(g_P+1) (spec 3.2D), the same form the virtual patient body uses."),
    ("S_ID", "5.7664e-4", "/min per mU/L", [], "Tuned so the basal equilibrium sits on the 5.8 target under the saturable uptake; not a published catch-all value."),
    ("EGP_B", "0.0161", "mmol/kg/min", ["W04"], "Basal endogenous glucose production."),
    ("BIR", "1.0", "U/h", [], "Basal insulin requirement of the default configuration."),
]


def ref_keys(keys) -> str:
    if not keys:
        return ""
    labels = "".join(f'<a href="#ref-{k}">[{k}]</a>' for k in keys)
    return f'<span class="refs">{labels}</span>'


def build_harness_cards(ver_items, claims, ver_rel) -> list[str]:
    cards = []
    for h in ver_items:
        if h["kind"] != "harness":
            continue
        c = claims.get(h["name"], {"asserts": [], "assumes": []})
        doc_html = f'<p>{inline(h["docs"])}</p>' if h["docs"] else ""
        domain = "".join(
            f'<li><code class="dom">{inline(expr)}</code></li>'
            for _, expr, _ in sorted(c["assumes"])
        )
        assert_points = "".join(
            f"<li>{inline(msg)}</li>" for _, _, msg in sorted(c["asserts"])
        )
        cards.append(
            f"""<details id="harness-{h['name']}" open class="harness">
  <summary><code>{h['name']}</code><span class="src">{src_link(ver_rel, h['line'])}</span></summary>
  {doc_html}
  <div class="cols">
    <div><h4>Input domain</h4><ul>{domain}</ul></div>
    <div><h4>Proved properties</h4><ul>{assert_points}</ul></div>
  </div>
</details>"""
        )
    return cards


def build_native_groups(all_items) -> list[str]:
    groups = []
    for rel in CANONICAL_ORDER:
        items = all_items[rel]
        mod_doc = next((it for it in items if it["kind"] == "module"), None)
        tests = [it for it in items if it["kind"] == "native_test"]
        if not tests:
            continue
        rows = []
        for t in sorted(tests, key=lambda x: x["line"]):
            doc = render_doc_text(t["docs"]) if t["docs"] else ""
            rows.append(
                f'<li><div class="test-name"><code>{t["name"]}</code>'
                f'<span class="src">{src_link(rel, t["line"])}</span></div>{doc}</li>'
            )
        mod_header = inline(mod_doc["docs"]) if mod_doc else ""
        groups.append(
            f'<div class="module"><h3>{rel}</h3>'
            f'<div class="moddoc">{mod_header}</div><ul class="tests">{"".join(rows)}</ul></div>'
        )
    return groups


def render_doc_text(docs: str) -> str:
    out = inline(docs)
    return f"<p>{out}</p>" if out else ""


def build_const_rows(all_items) -> str:
    rows = ""
    for rel in CANONICAL_ORDER:
        for it in sorted(all_items[rel], key=lambda x: x["line"]):
            if it["kind"] != "const":
                continue
            keys, note_text = [], ""
            if it["name"] in CONSTANT_NOTES:
                keys, note_text = CONSTANT_NOTES[it["name"]]
            rows += (
                f'<tr><td><code>{it["name"]}</code></td>'
                f'<td><code>{esc(it["value"])}</code></td>'
                f'<td>{inline(it["docs"])} {note_text} {ref_keys(keys)}</td>'
                f'<td class="src">{src_link(rel, it["line"])}</td></tr>'
            )
    return rows


def render():
    all_items = {}
    for rel in CANONICAL_ORDER:
        all_items[rel] = parse_rust(REPO / rel)

    # Per-crate: verification suite items, harness claims, module docs.
    crate_ver = {}
    crate_lib = {}
    for crate, files in CRATE_FILES.items():
        lib_rel = f"{crate}/src/lib.rs"
        ver_rel = f"{crate}/src/verification.rs"
        ver_items = parse_rust(REPO / ver_rel)
        claims = harness_claims(REPO / ver_rel)
        crate_ver[crate] = {
            "rel": ver_rel,
            "cards": build_harness_cards(ver_items, claims, ver_rel),
            "philosophy": inline(
                next((it for it in ver_items if it["kind"] == "module"), {}).get("docs", "")
            ),
            "count": sum(1 for it in ver_items if it["kind"] == "harness"),
        }
        library_mod = next(
            (it for it in all_items[lib_rel] if it["kind"] == "module"), None
        )
        crate_lib[crate] = inline(library_mod["docs"]) if library_mod else ""

    native_groups = build_native_groups(all_items)
    const_rows = build_const_rows(all_items)

    model_rows = "".join(
        f'<tr><td><code>{n}</code></td><td><code>{v}</code></td><td>{u}</td>'
        f'<td>{note} {ref_keys(keys)}</td></tr>'
        for n, v, u, keys, note in MODEL_PARAM_DEFAULTS
    )

    fuzz_rows = ""
    for target, desc, rel in FUZZ_TARGETS:
        fuzz_rows += (
            f'<tr><td><code>{target}</code></td><td>{esc(desc)}</td>'
            f'<td class="src">{src_link(rel)}</td></tr>'
        )

    ver_philosophy = "\n".join(
        f'<div class="moddoc"><h4>{crate}</h4>{crate_ver[crate]["philosophy"]}</div>'
        for crate in CRATE_FILES
    )
    library_docs = "\n".join(
        f'<div class="moddoc"><h4>{crate}</h4>{crate_lib[crate]}</div>'
        for crate in CRATE_FILES
    )

    crate_harnesses = "\n".join(
        f'<div class="module"><h3>{crate}</h3><p class="moddoc">{crate_ver[crate]["count"]} '
        f'harness(es) in {crate_ver[crate]["rel"]}.</p>{"".join(crate_ver[crate]["cards"])}</div>'
        for crate in CRATE_FILES
    )

    references = "".join(
        f'<li id="ref-{key}"><strong>[{key}]</strong> {esc(text)} '
        f'<a href="{url}">{esc(url)}</a></li>'
        for key, text, url in REFS
    )
    documents = "".join(
        f'<li><strong>{name}</strong> ({inline(desc)}): '
        f'<a href="{path}">{esc(path)}</a></li>'
        for name, path, desc in DOCUMENTS
    )

    tool_cmds = "<pre><code>" + "\n".join([
        "# whole Kani suite (budget: 90s wall, 30s per harness)",
        "cargo kani -Z unstable-options --harness-timeout 30s -j --output-format terse",
        "",
        "# native properties incl. proptest (opt-in soak via TIR_TUNER_SOAK_ITERS)",
        "cargo test",
        "TIR_TUNER_SOAK_ITERS=50000 cargo test",
        "",
        "# coverage-guided soak",
        "cargo +nightly fuzz run hovorka_step",
        "cargo +nightly fuzz run nmpc_grid_dose",
    ]) + "</code></pre>"

    return f"""<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>CamAPS FX controller core: verification suite catalog</title>
<style>
:root {{ color-scheme: light dark; --line: #8888; }}
* {{ box-sizing: border-box; }}
body {{ font-family: -apple-system, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
       line-height: 1.55; margin: 0; color: #1a1a1a; background: #fff; }}
main {{ max-width: 52rem; margin: 0 auto; padding: 2rem 1.5rem 4rem; }}
h1 {{ font-size: 1.6rem; margin: 0 0 .2rem; }}
h2 {{ font-size: 1.25rem; margin: 2.2rem 0 .6rem; border-bottom: 1px solid var(--line); padding-bottom: .3rem; }}
h3 {{ font-size: 1.05rem; margin: 1.4rem 0 .4rem; }}
h4 {{ font-size: .85rem; margin: .4rem 0; text-transform: uppercase; letter-spacing: .04em; }}
p {{ margin: .5rem 0; }}
code {{ font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
       font-size: .86em; background: #f1f1f1; padding: .1em .3em; border-radius: 3px; }}
pre {{ background: #f6f6f6; padding: .8rem 1rem; overflow-x: auto; border-radius: 6px; }}
pre code {{ background: none; padding: 0; }}
table {{ border-collapse: collapse; width: 100%; margin: .8rem 0; font-size: .92rem; }}
th, td {{ text-align: left; vertical-align: top; border: 1px solid var(--line);
         padding: .45rem .6rem; }}
th {{ background: #f4f4f4; }}
blockquote {{ margin: .8rem 0; padding: .2rem 1rem; border-left: 3px solid var(--line); color: #444; }}
a {{ color: #1a56db; text-decoration: none; }}
a:hover {{ text-decoration: underline; }}
.summary {{ font-size: .95rem; color: #444; }}
.refs {{ white-space: nowrap; font-size: .85em; }}
.src {{ font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: .78em; color: #666; }}
.harness {{ border: 1px solid var(--line); border-radius: 6px; margin: .7rem 0; padding: .4rem .8rem; }}
.harness summary {{ cursor: pointer; font-weight: 600; }}
.harness summary .src {{ margin-left: .6em; font-weight: 400; }}
.cols {{ display: flex; gap: 1.2rem; flex-wrap: wrap; }}
.cols > div {{ flex: 1 1 46%; min-width: 15rem; }}
ul.tests, ul {{ margin: .4rem 0 .8rem; padding-left: 1.2rem; }}
.tests li {{ margin: .4rem 0; }}
.test-name {{ font-weight: 600; }}
.moddoc {{ font-size: .92rem; color: #333; }}
.lint {{ background: #fdf3f3; border: 1px solid #ecc; border-radius: 6px; padding: .6rem 1rem; }}
footer {{ margin-top: 3rem; padding-top: 1rem; border-top: 1px solid var(--line); font-size: .82rem; color: #666; }}
@media print {{ body {{ font-size: 11pt; }} a {{ color: inherit; }} }}
</style>
</head>
<body>
<main>
<h1>CamAPS FX controller core: verification suite catalog</h1>
<p class="summary">What the formal verification suite checks and which tool checks it: Kani proofs, native
property tests, and coverage-guided fuzzing, each tied to the source paper or the patent it implements.
Generated from the Rust sources by <code>tools/gen_verification_report.py</code>. This document describes the
current state of the suite, not a test run; it contains no pass/fail results.</p>

{section("1. Scope and tool split", f"""
<p>Three tools share the workload, chosen by what each is good at. <strong>Kani</strong> proves universal
statements over bounded symbolic inputs whose operations are comparisons, clamps, additions and
multiplications by constants. <strong>Native Rust tests</strong> (exhaustive lattice sweeps, <code>proptest</code>
randomized cases) cover the rounding-level claims: sum-to-one identities, dense sweeps, the wired
behaviour of the full model. <strong>Coverage-guided fuzzing</strong> soaks the actual numerical code paths.
The split is documented in <a href="docs/camaps_fx_kani_specification.md">the blueprint</a> and repeated in the module docs the
report is extracted from.</p>
{library_docs}
{ver_philosophy}
""")}

{section("2. What the model is and where it comes from", f"""
<p>The crate implements a deterministic core of the Cambridge hybrid closed-loop algorithm: the
10-dimensional extended Hovorka glucoregulatory state, subcutaneous insulin and gut absorption
submodels, endogenous glucose production (EGP), the interstitial (CGM) compartment, an NMPC-style
dose calculator with Ease-off / Boost modes, and interacting-multiple-model (IMM) mode-probability
bookkeeping.</p>
<table>
<tr><th>Submodel</th><th>Contents</th><th>Source</th></tr>
<tr><td>10-dimensional state</td><td>Two subcutaneous insulin depots (<code>i1</code>, <code>i2</code>),
two remote insulin actions (<code>r_d</code>, <code>r_e</code>), two gut depots (<code>a1</code>,
<code>a2</code>), glucose masses in the accessible, non-accessible and interstitial compartments
(<code>q1</code>, <code>q2</code>, <code>q3</code>), and the process-noise state <code>u_s</code>.</td>
<td>{ref_keys(['CA2345'])} claim 33; {ref_keys(['W04'])}</td></tr>
<tr><td>Insulin absorption</td><td>Two-depot subcutaneous chain with time-to-peak <code>t_max,I</code>
and clearance <code>MCR_I &middot; W</code>; basal/bolus inputs converted to the mU mass convention.</td>
<td>{ref_keys(['CA2345'])}{ref_keys(['W04'])}</td></tr>
<tr><td>Glucose kinetics</td><td>Two-compartment transfer (<code>k12</code>, <code>k21</code>), constant
non-insulin utilization <code>F01</code>, insulin disposal <code>S_ID &middot; r_d</code>, EGP, gut
absorption, and interstitial kinetics driving the sensor value <code>g_IG</code>.</td>
<td>{ref_keys(['CA2345'])}{ref_keys(['W04'])}</td></tr>
<tr><td>EGP</td><td>Exponential suppression halving basal EGP per 0.5 units of the remote EGP action <code>S_EGP*r_E</code>
risen above its resting value, capped at <code>EGP_MAX_FOLD_OVER_BASAL &times; EGP_B</code> on the low-insulin branch;
with the population gain this mirrors the virtual patient's suppression
(see <a href="#caveats">caveats</a>).</td>
<td>spec &sect;3.2D; {ref_keys(['W04'])}</td></tr>
<tr><td>NMPC dose calculator</td><td>Hovorka 2004 eq. 9 sequence objective
<code>J = &Sigma;(g_IG(t+j)-w(t+j))&sup2; + &Sigma; ((u(t+j)-u(t+j-1))/K_u)&sup2;/k_agr</code> over a moving target
trajectory (&sect;3.3), seeded by a candidate-grid constant rate and refined by bounded coordinate descent;
the first rate of the best sequence is applied. <code>K_u</code> normalizes the effort term to the glucose
scale (with <code>K_u = 1</code> the objective is eq. 9 as published). Hard hypoglycemia cutoff and
<code>[0, u_max]</code> delivery bounds.</td>
<td>spec &sect;5.1; {ref_keys(['BQ13'])}</td></tr>
<tr><td>Operating modes</td><td>Standard, Ease-off (exercise, elevated target and suspension below it),
Boost (temporary +35% intensification).</td>
<td>{ref_keys(['W22'])}{ref_keys(['W22B'])}</td></tr>
<tr><td>IMM bookkeeping</td><td>Bayesian mode-probability update, mixing of per-mode estimates, Markov
transition matrix (column-stochastic, diagonal-dominant).</td>
<td>{ref_keys(['CA2345'])} &sect;4.1</td></tr>
</table>

<h3>Operating parameters and safety thresholds</h3>
<p>The user-facing constants come from the Ware et al. 2022 cohort trial; the two TIR band bounds come
from the real-world analyses. The extraction is from the workspace crate modules.</p>
<table>
<tr><th>Constant</th><th>Value</th><th>Meaning</th><th>Source</th></tr>
{const_rows}
</table>

<h3>Model parameter defaults</h3>
<p>Defaults of <code>HovorkaParams</code> (<code>HovorkaParams::default()</code>), the operating point
both the Kani suite and the native tests run at.</p>
<table>
<tr><th>Parameter</th><th>Value</th><th>Unit</th><th>Notes</th></tr>
{model_rows}
</table>
""")}

{section("3. Kani formal proofs", f"""
<p>The four crates each carry one verification module, compiled only under <code>cargo kani</code>. Each harness below
shows its extracted doc comment, the symbolic input domain it is bounded to (<code>kani::assume</code>) and the
assertions it discharges (<code>kani::assert</code>), pulled verbatim from the source.</p>
{crate_harnesses}
""")}

{section("4. Native verification coverage", f"""
<p>Items the Kani suite deliberately does not discharge, per section 6.3 of the blueprint: the
rounding-dependent IMM sum-to-one and mixing/update identities, the full-state wired non-negativity and
finiteness of the model, and the NMPC realized-cost composition with the hypoglycemia guard. The
exhaustive lattice sweeps and the <code>proptest</code> cases live in the <code>#[cfg(test)]</code> modules
of the crate and are extracted here alongside their doc comments.</p>
{ ''.join(native_groups) }

<h3>Grid-selector composition detail</h3>
<p><code>nmpc_grid_dose</code> is deliberately a pure cost minimizer: it does not see the CGM reading. The
delivered pump rate must pass through the hypoglycemia guard (<code>is_hypoglycemic</code> /
<code>compute_nmpc_dose_mode</code>), and that composition is what the native
<code>grid_dose_composed_with_hypo_cutoff_is_safe</code> case checks. It is not a single Kani harness
because the two layers are proved separately there.</p>
""")}

{section("5. Coverage-guided fuzz targets", f"""
<p>Soak targets in <code>fuzz/fuzz_targets/</code> over the actual numerical code, asserting the same
safety invariants the native suite checks.</p>
<table>
<tr><th>Target</th><th>Properties asserted</th><th>Source</th></tr>
{fuzz_rows}
</table>
""")}

{section("6. Budget and tooling", tool_cmds + """
<p>Reference points from spec section 6.1: the Kani suite finishes in about two minutes sequential,
dominated by the sequence-cost harness at about 72 seconds; the remaining eight proofs stay well under
30 seconds each. Native properties run under plain <code>cargo test</code> (default
budget about 10 seconds).</p>
""")}

{section("7. Out of scope and future work", inline("""\
Per the blueprint section 6.4 and the suite module docs: the full 10-state IMM extended Kalman filter
(state/covariance mixing, predict, update, likelihood) is not implemented; the crate covers
mode-probability bookkeeping only. Multi-step eq. 9 NMPC over the full 240-minute sequence is implemented
and wired into the closed-loop simulation (`nmpc_sequence` + refinement),
but the Marquardt minimization of the paper is replaced by bounded
coordinate descent over the quantized sequence. The bayesian real-time adaptation of the six
individual dynamic parameters (section 3) is future work and would require a tractable linearization or
stubbing of the nonlinear model before it could be verified. The process-noise state u_s is reserved
and carried through unchanged; the stochastic increment is injected externally."""))}

<div class="lint" id="caveats">
<strong>Source-mapping caveats</strong>
<p>The EGP submodel is the specification form of section 3.2D: a fixed 0.5 mU/L relative half-increment
denominator plus the low-insulin-branch cap at <code>EGP_MAX_FOLD_OVER_BASAL &times; EGP_B</code>, and
<code>F01</code> is constant (no glucose-dependent saturable elimination). This is the model the suite
verifies, not the literal functional form of the 2004 publication; the divergence is documented in
<code>tir-tuner-aps/src/hovorka.rs</code>. <code>S_ID</code> is a crate calibration chosen so the basal equilibrium sits
on the 5.8 mmol/L target; it is not a published catch-all value. The IMM Markov entries are illustrative
tuning values; the verified properties rely only on column stochasticity and non-negativity. The patent
paragraph pointers [0102]-[0108] and [0109]-[0116] used in the blueprint could not be cross-checked
against the flattened patent text and are unverified.</p>
</div>

{section("References", f"""
<ol>
{references}
</ol>
<h3>Project documents</h3>
<ul>{documents}</ul>
""")}

<footer>
Generated from the four workspace crates' <code>src/</code> directories and <code>fuzz/fuzz_targets/</code>.
Regenerate with <code>python3 tools/gen_verification_report.py</code>. This catalog reflects source
structure, not verification results. For a plain-language companion see the pages under
<code>docs/patient/</code>.
</footer>
</main>
</body>
</html>"""


def main():
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(render(), encoding="utf-8")
    print(f"wrote {OUT}")


if __name__ == "__main__":
    main()
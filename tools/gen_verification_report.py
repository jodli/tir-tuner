#!/usr/bin/env python3
"""Generate docs/verification_report.html from the sources.

Everything comes from files the gates also read: docs/sources.toml (claims
and the medical file list), the `src:` tags in those files, the Kani
harnesses, the tests, the fuzz targets and .cargo/mutants.toml. Nothing
here runs a verifier.

Usage: uv run --python 3.12 tools/gen_verification_report.py
"""

import html
import re
import tomllib
from bisect import bisect_right
from collections import Counter, defaultdict
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
OUT = REPO / "docs" / "verification_report.html"
# Same syntax as tag_re() in tir-tuner-common/tests/sources.rs.
TAG = re.compile(r"src: ([a-z0-9-]+)")
CONST = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?const\s+([A-Z][A-Z0-9_]*)\s*:\s*[^=]+=\s*(.*)$")
FN = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?fn\s+(\w+)")


def esc(text: str) -> str:
    return html.escape(text, quote=False)


def inline(text: str) -> str:
    """Escape, render `code`, and link `src:` tags to their claim."""
    out = re.sub(r"`([^`]+)`", r"<code>\1</code>", esc(text))
    return TAG.sub(r'<a href="#claim-\1">\1</a>', out)


def module_doc(lines: list[str]) -> str:
    docs = []
    for line in lines:
        s = line.strip()
        if s.startswith("//!"):
            docs.append(s[3:].strip())
        elif s and not s.startswith("#!"):
            break
    return " ".join(docs)


def parse(rel: str) -> dict:
    """Consts outside test code, tests, Kani harnesses and tag sites."""
    lines = (REPO / rel).read_text(encoding="utf-8").splitlines()
    out = {"doc": module_doc(lines), "consts": [], "tests": [], "harnesses": [], "tags": []}
    docs, kind, in_test = [], None, False
    i = 0
    while i < len(lines):
        s = lines[i].strip()
        for m in TAG.finditer(s):
            out["tags"].append((m.group(1), i + 1))
        if s == "#[cfg(test)]" and not lines[i].startswith(" "):
            in_test = True
        if s.startswith("///"):
            docs.append(s[3:].strip())
        elif s == "#[test]":
            kind = "tests"
        elif s.startswith("#[kani::proof]"):
            kind = "harnesses"
        elif s.startswith("#["):
            pass
        elif (m := CONST.match(lines[i])) and not in_test:
            value, start = m.group(2), i
            while not value.rstrip().endswith(";"):
                i += 1
                value += " " + lines[i].strip()
            out["consts"].append({"name": m.group(1), "value": value.rstrip(";").strip(),
                                  "docs": " ".join(docs), "line": start + 1})
            docs = []
        elif (m := FN.match(lines[i])) and kind:
            out[kind].append({"name": m.group(1), "docs": " ".join(docs), "line": i + 1})
            docs, kind = [], None
        else:
            docs = []
        i += 1
    return out


def kani_claims(rel: str, harnesses: list[str]) -> dict:
    """Harness name -> (assumes, assert messages), in line order."""
    text = (REPO / rel).read_text(encoding="utf-8")
    fns = [(m.start(), m.group(1)) for m in re.finditer(r"^(?:pub )?fn (\w+)", text, re.MULTILINE)]
    starts = [s for s, _ in fns]
    claims = {h: ([], []) for h in harnesses}

    def owner(pos: int):
        i = bisect_right(starts, pos) - 1
        return claims.get(fns[i][1]) if i >= 0 else None

    for m in re.finditer(r"kani::assume\((.*?)\);", text, re.DOTALL):
        if (c := owner(m.start())) is not None:
            c[0].append(" ".join(m.group(1).split()))
    for m in re.finditer(r'kani::assert\(.*?,\s*"([^"]*)"\s*,?\s*\)\s*;', text, re.DOTALL):
        if (c := owner(m.start())) is not None:
            c[1].append(m.group(1))
    return claims


def src(rel: str, line: int = 0) -> str:
    label = f"{rel}:{line}" if line else rel
    return f'<span class="src">{esc(label)}</span>'


def section(anchor: str, title: str, body: str) -> str:
    return f'<section id="{anchor}"><h2>{title}</h2>{body}</section>\n'


def claims_section(toml: dict, used: dict) -> str:
    sources = toml["sources"]
    by_source = defaultdict(list)
    for c in toml["claim"]:
        by_source[c["source"]].append(c)
    counts = Counter(c["status"] for c in toml["claim"])
    summary = ", ".join(f"{n} {s}" for s, n in counts.most_common())
    parts = [f"<p>{len(toml['claim'])} claims: {summary}. The statuses are defined at the top of "
             f"<code>docs/sources.toml</code>.</p>"]
    for key in sources:
        if key not in by_source:
            continue
        rows = ""
        for c in by_source[key]:
            values = ", ".join(f"{v:g}" for v in c.get("values", []))
            sites = "<br>".join(src(r, l) for r, l in used.get(c["id"], []))
            rows += (f'<tr id="claim-{c["id"]}" class="st-{c["status"]}"><td><code>{c["id"]}</code></td>'
                     f'<td>{c["status"]}</td><td>{esc(str(c.get("at", "")))}</td><td>{values}</td>'
                     f'<td>{inline(c.get("note", ""))}</td><td>{sites}</td></tr>')
        parts.append(f'<h3 id="src-{key}">{key}</h3><p class="moddoc">{inline(sources[key])}</p>'
                     f"<table><tr><th>Claim</th><th>Status</th><th>Where</th><th>Values</th>"
                     f"<th>Note</th><th>Used at</th></tr>{rows}</table>")
    return "".join(parts)


def files_section(files: list[str], parsed: dict) -> str:
    parts = []
    for rel in files:
        p = parsed[rel]
        rows = "".join(
            f'<tr><td><code>{c["name"]}</code></td><td><code>{esc(c["value"])}</code></td>'
            f'<td>{inline(c["docs"])}</td><td>{src(rel, c["line"])}</td></tr>'
            for c in p["consts"])
        table = (f"<table><tr><th>Const</th><th>Value</th><th>Doc</th><th>At</th></tr>{rows}</table>"
                 if rows else "")
        tests = "".join(
            f'<li><code>{t["name"]}</code> {src(rel, t["line"])}'
            f'{"<br>" + inline(t["docs"]) if t["docs"] else ""}</li>' for t in p["tests"])
        tests = f'<h4>Tests ({len(p["tests"])})</h4><ul class="tests">{tests}</ul>' if tests else ""
        parts.append(f'<div class="module"><h3>{rel}</h3><p class="moddoc">{inline(p["doc"])}</p>'
                     f"{table}{tests}</div>")
    return "".join(parts)


def kani_section(crates: list[str]) -> tuple[str, int]:
    parts, total = [], 0
    for crate in crates:
        rel = f"{crate}/src/verification.rs"
        p = parse(rel)
        claims = kani_claims(rel, [h["name"] for h in p["harnesses"]])
        total += len(p["harnesses"])
        cards = ""
        for h in p["harnesses"]:
            assumes, asserts = claims[h["name"]]
            dom = "".join(f"<li><code>{esc(a)}</code></li>" for a in assumes)
            props = "".join(f"<li>{inline(a)}</li>" for a in asserts)
            cards += (f'<details class="harness" open><summary><code>{h["name"]}</code>{src(rel, h["line"])}'
                      f'</summary><p>{inline(h["docs"])}</p><div class="cols">'
                      f"<div><h4>Input domain</h4><ul>{dom}</ul></div>"
                      f"<div><h4>Proved</h4><ul>{props}</ul></div></div></details>")
        parts.append(f'<div class="module"><h3>{crate}</h3><p class="moddoc">{inline(p["doc"])}</p>'
                     f'<p class="moddoc">{len(p["harnesses"])} harnesses, run with '
                     f"<code>cargo kani -p {crate}</code>.</p>{cards}</div>")
    return "".join(parts), total


def mutants_section(files: list[str]) -> str:
    text = (REPO / ".cargo" / "mutants.toml").read_text(encoding="utf-8")
    block = text.split("exclude_re = [", 1)[1].split("]", 1)[0]
    # A comment explains the patterns below it, up to the next comment.
    reason, rows, after_pattern = "", "", False
    for s in (line.strip() for line in block.splitlines()):
        if s.startswith("#"):
            reason = s.lstrip("# ") if after_pattern else f"{reason} {s.lstrip('# ')}".strip()
            after_pattern = False
        elif s.startswith('"'):
            pattern = tomllib.loads(f"p = {s.rstrip(',')}")["p"]
            rows += f"<tr><td><code>{esc(pattern)}</code></td><td>{esc(reason)}</td></tr>"
            after_pattern = True
    return (f"<p><code>cargo mutants</code> mutates the {len(files)} medical files (the same list as "
            f"<code>lint.files</code>; the lint test fails if they differ). Every surviving mutant needs a test "
            f"or the code goes. These mutants are excluded because they can't change a result:</p>"
            f"<table><tr><th>Mutant</th><th>Why it can't change a result</th></tr>{rows}</table>")


def fuzz_section() -> str:
    rows = ""
    for f in sorted((REPO / "fuzz" / "fuzz_targets").glob("*.rs")):
        rel = f.relative_to(REPO).as_posix()
        doc = module_doc(f.read_text(encoding="utf-8").splitlines())
        rows += f"<tr><td><code>{f.stem}</code></td><td>{inline(doc)}</td><td>{src(rel)}</td></tr>"
    return ("<p>Coverage-guided soak over the real numerical code: <code>cargo +nightly fuzz run "
            f"&lt;target&gt;</code>.</p><table><tr><th>Target</th><th>Asserts</th><th>File</th></tr>{rows}</table>")


def render() -> str:
    toml = tomllib.loads((REPO / "docs" / "sources.toml").read_text(encoding="utf-8"))
    files = toml["lint"]["files"]
    parsed = {rel: parse(rel) for rel in files}
    used = defaultdict(list)
    for rel in files:
        for tag, line in parsed[rel]["tags"]:
            used[tag].append((rel, line))
    crates = sorted(p.parent.parent.name for p in REPO.glob("tir-tuner-*/src/verification.rs"))
    kani, harnesses = kani_section(crates)
    tests = sum(len(p["tests"]) for p in parsed.values())
    consts = sum(len(p["consts"]) for p in parsed.values())
    specs = "".join(f'<li><a href="{Path(d).relative_to("docs")}">{d}</a></li>' for d in toml["lint"]["docs"])

    return f"""<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>tir-tuner: verification report</title>
<style>
:root {{ color-scheme: light dark; --line: #8888; }}
* {{ box-sizing: border-box; }}
body {{ font-family: -apple-system, "Segoe UI", Roboto, Helvetica, Arial, sans-serif; line-height: 1.5; margin: 0; }}
main {{ max-width: 64rem; margin: 0 auto; padding: 2rem 1.5rem 4rem; }}
h2 {{ margin-top: 2.4rem; border-bottom: 1px solid var(--line); padding-bottom: .3rem; }}
h4 {{ font-size: .85rem; margin: .6rem 0 .3rem; text-transform: uppercase; letter-spacing: .04em; }}
code {{ font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: .86em; }}
table {{ border-collapse: collapse; width: 100%; margin: .6rem 0; font-size: .88rem; }}
th, td {{ text-align: left; vertical-align: top; border: 1px solid var(--line); padding: .35rem .5rem; }}
a {{ color: #3b6fd8; text-decoration: none; }}
.src {{ font-family: ui-monospace, Menlo, monospace; font-size: .76em; opacity: .7; margin-left: .4em; }}
.moddoc {{ font-size: .92rem; }}
.st-assumption td:nth-child(2), .st-open td:nth-child(2), .st-user td:nth-child(2) {{ font-weight: 600; color: #c26a00; }}
.harness {{ border: 1px solid var(--line); border-radius: 6px; margin: .6rem 0; padding: .3rem .8rem; }}
.harness summary {{ cursor: pointer; }}
.cols {{ display: flex; gap: 1rem; flex-wrap: wrap; }}
.cols > div {{ flex: 1 1 45%; min-width: 15rem; }}
.tests li {{ margin: .3rem 0; }}
footer {{ margin-top: 3rem; font-size: .82rem; opacity: .7; }}
</style>
</head>
<body>
<main>
<h1>tir-tuner: verification report</h1>
<p>Every number on the medical path, where it comes from, and what checks it. Generated by
<code>tools/gen_verification_report.py</code> from the sources the gates read; it lists the checks, it doesn't
run them. For the plain-language version see <a href="patient/">the patient pages</a>.</p>
<ul>
<li><a href="#claims">{len(toml["claim"])} claims</a> in <code>docs/sources.toml</code>, each tied to a source.</li>
<li><a href="#files">{len(files)} medical files</a> with {consts} consts and {tests} tests. The lint test
<code>tir-tuner-common/tests/sources.rs</code> fails when a number in these files doesn't trace to a claim.</li>
<li><a href="#mutants">Mutation gate</a> on the same files.</li>
<li><a href="#kani">{harnesses} Kani proofs</a> in {len(crates)} crates.</li>
<li><a href="#fuzz">Fuzz targets</a>.</li>
</ul>

{section("claims", "Claims by source", claims_section(toml, used))}
{section("files", "Medical files", files_section(files, parsed))}
{section("mutants", "Mutation gate", mutants_section(files))}
{section("kani", "Kani proofs", kani)}
{section("fuzz", "Fuzz targets", fuzz_section())}
{section("specs", "Specs", f"<p>Specs whose <code>[id]</code> citations the lint checks:</p><ul>{specs}</ul>")}

<footer>Regenerate with <code>uv run --python 3.12 tools/gen_verification_report.py</code>. The papers stay
local; only their citations are here.</footer>
</main>
</body>
</html>
"""


if __name__ == "__main__":
    OUT.write_text(render(), encoding="utf-8")
    print(f"wrote {OUT}")

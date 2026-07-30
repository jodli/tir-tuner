#!/usr/bin/env python3
"""De-identification stage for the Glooko data pipeline.

Takes a raw Glooko export that still contains PII (a ``.zip`` or an already
extracted directory) and writes a de-identified copy to an output directory,
preserving the relative structure. Downstream analysis stages should read from
the output directory only, never from the raw PII input.

PII removed
-----------
* Patient name in the banner row of every CSV:
  ``Name:<value>,Datumsbereich:...`` -> the value after ``Name:`` becomes
  the placeholder (default ``REDACTED``).

Left untouched (not PII)
------------------------
* Timestamps and glucose/insulin values (the actual measurement data).
* The ``Seriennummer`` column, which in Glooko exports holds generic device
  model names (e.g. ``CamAPS FreeStyle Libre 3``), not personal serials.

Usage
-----
    python strip_pii.py INPUT [OUTPUT] [--placeholder TEXT]

    INPUT   Path to the raw export: a .zip file or a directory of CSVs.
    OUTPUT  Directory for the de-identified copy (default: ./ingest).
"""
from __future__ import annotations

import argparse
import os
import re
import sys
import tempfile
import zipfile

# Matches `Name:<anything up to the next comma>`, keeping the `Name:` prefix so
# only the value is replaced.
_NAME_RE = re.compile(r"(Name:)[^,\r\n]*")


def redact_line(line: str, placeholder: str) -> str:
    """Redact the patient name in a Glooko banner line.

    Non-banner lines (no ``Name:`` token) are returned unchanged, so this is
    safe to apply to every line of a file.
    """
    if "Name:" not in line:
        return line
    return _NAME_RE.sub(rf"\1{placeholder}", line)


def deidentify_file(src: str, dst: str, placeholder: str) -> None:
    """Read one CSV, redact PII, write the de-identified copy.

    Reads as utf-8-sig to transparently drop any BOM and writes plain utf-8.
    Only the banner (first) line can contain the name; everything else is
    preserved verbatim.
    """
    with open(src, encoding="utf-8-sig") as f:
        text = f.read()

    lines = text.split("\n")
    if lines:
        lines[0] = redact_line(lines[0], placeholder)
    out = "\n".join(lines)

    os.makedirs(os.path.dirname(dst) or ".", exist_ok=True)
    with open(dst, "w", encoding="utf-8", newline="") as f:
        f.write(out)


def deidentify_tree(src_root: str, dst_root: str, placeholder: str) -> list[str]:
    """De-identify every ``*.csv`` under ``src_root`` into ``dst_root``."""
    processed: list[str] = []
    for dirpath, _dirnames, filenames in os.walk(src_root):
        for name in sorted(filenames):
            if not name.lower().endswith(".csv"):
                continue
            src = os.path.join(dirpath, name)
            rel = os.path.relpath(src, src_root)
            deidentify_file(src, os.path.join(dst_root, rel), placeholder)
            processed.append(rel)
    return processed


def run(input_path: str, output_dir: str, placeholder: str) -> list[str]:
    """Run the stage. ``input_path`` may be a .zip or a directory."""
    if not os.path.exists(input_path):
        raise FileNotFoundError(input_path)

    if zipfile.is_zipfile(input_path):
        with tempfile.TemporaryDirectory() as tmp:
            with zipfile.ZipFile(input_path) as zf:
                zf.extractall(tmp)
            return deidentify_tree(tmp, output_dir, placeholder)

    if os.path.isdir(input_path):
        return deidentify_tree(input_path, output_dir, placeholder)

    raise ValueError(f"Input must be a .zip or a directory: {input_path}")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("input", help="Raw export: a .zip file or a directory of CSVs")
    parser.add_argument(
        "output",
        nargs="?",
        default="ingest",
        help="Output directory for the de-identified copy (default: ./ingest)",
    )
    parser.add_argument(
        "--placeholder",
        default="REDACTED",
        help="Text that replaces the patient name (default: REDACTED)",
    )
    args = parser.parse_args(argv)

    processed = run(args.input, args.output, args.placeholder)
    for rel in processed:
        print(f"deidentified: {rel}")
    print(f"\n{len(processed)} files written to {args.output}")
    return 0


if __name__ == "__main__":
    sys.exit(main())

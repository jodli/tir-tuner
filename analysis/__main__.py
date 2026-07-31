"""Command-line entry point.

    uv run python -m analysis run   [--data DIR] [--out DIR] [--settings PATH]
                                    [--as-of DATE] [--weeks N] [--no-llm] [--no-charts]

    uv run python -m analysis stage NAME --in ARTIFACT.json [--out ARTIFACT.json]

``run`` executes the whole pipeline, writes each stage's cumulative state to
``runs/<as_of>/stages/<stage>.json``, persists the run (history + result JSON),
renders charts and prints the German summary. ``stage`` re-runs a single stage
from a saved artifact so any step can be exercised in isolation.
"""
from __future__ import annotations

import argparse
import json
import os
import sys

from . import (
    charts,
    clamp,
    corrections,
    glycemic,
    history,
    iob,
    loaders,
    meals,
    recommend,
    report,
    settings,
    snapshot,
    trends,
    window,
)
from .contracts import Config, PipelineState
from .strings import L

# Ordered state-mutating stages. `history` persists; report/charts are output-only.
STAGES = [
    ("load", loaders.run),
    ("window", window.run),
    ("glycemic", glycemic.run),
    ("meals", meals.run),
    ("corrections", corrections.run),
    ("settings", settings.run),
    ("iob", iob.run),
    ("trends", trends.run),
    ("snapshot", snapshot.run),
    ("recommend", recommend.run),
    ("clamp", clamp.run),
    ("history", history.run),
]
STAGE_MAP = dict(STAGES)


def _config_from_args(args) -> Config:
    return Config(
        data_dir=args.data,
        out_dir=args.out_dir,
        settings_path=args.settings,
        as_of=args.as_of,
        weeks=args.weeks,
        use_llm=not args.no_llm,
        make_charts=not args.no_charts,
    )


def cmd_run(args) -> int:
    config = _config_from_args(args)
    state = PipelineState()
    artifacts: list[tuple[str, dict]] = []
    for name, fn in STAGES:
        state = fn(state, config)
        artifacts.append((name, state.to_json()))

    as_of = state.window.as_of
    stages_dir = os.path.join(config.out_dir, as_of, "stages")
    os.makedirs(stages_dir, exist_ok=True)
    for name, payload in artifacts:
        with open(os.path.join(stages_dir, f"{name}.json"), "w", encoding="utf-8") as f:
            json.dump(payload, f, ensure_ascii=False)

    written_charts: list[str] = []
    if config.make_charts:
        written_charts = charts.render(state, config)

    report.print_summary(state, config)
    print(f"\n{L['result_written']}: {os.path.join(config.out_dir, as_of, 'result.json')}")
    if written_charts:
        print(f"{L['charts_written']}: {os.path.dirname(written_charts[0])}")
    return 0


def cmd_stage(args) -> int:
    config = _config_from_args(args)
    name = args.name

    if name in ("report", "charts"):
        if not args.infile:
            print("stage report/charts requires --in", file=sys.stderr)
            return 2
        with open(args.infile, encoding="utf-8") as f:
            state = PipelineState.from_json(json.load(f))
        if name == "report":
            report.print_summary(state, config)
        else:
            paths = charts.render(state, config)
            print(f"{L['charts_written']}: {os.path.dirname(paths[0]) if paths else '(keine)'}")
        return 0

    if name not in STAGE_MAP:
        print(f"unknown stage '{name}'. known: {', '.join(STAGE_MAP)}, report, charts", file=sys.stderr)
        return 2

    if name == "load":
        state = PipelineState()
    else:
        if not args.infile:
            print(f"stage '{name}' requires --in <artifact.json>", file=sys.stderr)
            return 2
        with open(args.infile, encoding="utf-8") as f:
            state = PipelineState.from_json(json.load(f))

    state = STAGE_MAP[name](state, config)

    out = args.outfile
    if not out:
        base = os.path.dirname(args.infile) if args.infile else os.path.join(config.out_dir, "stages")
        os.makedirs(base or ".", exist_ok=True)
        out = os.path.join(base or ".", f"{name}.json")
    with open(out, "w", encoding="utf-8") as f:
        json.dump(state.to_json(), f, ensure_ascii=False)
    print(f"stage '{name}' -> {out}")
    return 0


def _add_common(p: argparse.ArgumentParser) -> None:
    p.add_argument("--data", default="ingest", help="de-identified export directory")
    p.add_argument("--settings", default="settings.json", help="configured CR/CF history")
    p.add_argument("--as-of", default=None, help="ISO end date of the window (default: last CGM day)")
    p.add_argument("--weeks", type=int, default=4, help="rolling window length in weeks")
    p.add_argument("--no-llm", action="store_true", help="use the deterministic rule engine instead of BAML")
    p.add_argument("--no-charts", action="store_true", help="skip chart rendering")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="analysis", description=__doc__.splitlines()[0])
    sub = parser.add_subparsers(dest="command", required=True)

    p_run = sub.add_parser("run", help="run the whole pipeline")
    _add_common(p_run)
    p_run.add_argument("--out", dest="out_dir", default="runs", help="output/run directory")
    p_run.set_defaults(func=cmd_run)

    p_stage = sub.add_parser("stage", help="run a single stage from a saved artifact")
    p_stage.add_argument("name", help="stage name (load, window, ... , clamp, history, report, charts)")
    p_stage.add_argument("--in", dest="infile", default=None, help="input state artifact JSON")
    p_stage.add_argument("--out", dest="outfile", default=None, help="output state artifact JSON")
    p_stage.add_argument("--run-dir", dest="out_dir", default="runs", help="output/run directory")
    _add_common(p_stage)
    p_stage.set_defaults(func=cmd_stage)
    return parser


def main(argv=None) -> int:
    args = build_parser().parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())

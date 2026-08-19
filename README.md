# tir-tuner

WIP. Analyzes a Glooko export (CamAPS FX closed loop + Libre 3 + YpsoPump) and
proposes **conservative** carb-ratio (CR) and correction-factor (CF) changes to
improve time-in-range. Meant to be run weekly over a rolling 4-week window,
keeping a history so you can see how glucose reacts to changes.

Everything measurable is deterministic Python. Only the *judgment* step (which
blocks to nudge and why) goes to an LLM via BAML, and its numbers are always
bounded afterwards by a safety clamp (±10% per run, min-sample gate). User-facing
output is German.

## Key constraint
The export does **not** contain your configured CR/CF (the columns are blank,
BG-entry is 0). So:
- effective CR is inferred per meal as `carbs / units`;
- CF is inferred weakly from correction boluses (low confidence on a closed loop);
- you keep your *actual* configured schedule in `settings.json` (dated history) as
  ground truth. Without it the pipeline still runs in inference-only mode.

## Setup
```sh
uv sync
uv run baml-cli generate          # regenerates baml_client/ (git-ignored)
export ZAI_API_KEY=...             # only needed for the LLM path
cp settings.example.json settings.json   # then edit with your real schedules
```

## De-identify first
The raw export contains your name. Strip it into `ingest/` (analysis only reads
from there):
```sh
python strip_pii.py path/to/raw_export.zip ingest
```

## Run
```sh
# Full run (LLM recommendations, charts, German summary; writes runs/<as_of>/)
uv run python -m analysis run

# Offline / no token: deterministic rule engine instead of the LLM
uv run python -m analysis run --no-llm

# Historical window / weekly cadence
uv run python -m analysis run --as-of 2026-07-30 --weeks 4
```
Outputs per run under `runs/<as_of>/`: `result.json` (full record), `stages/*.json`
(per-stage artifacts), `charts/*.png`. Cross-run history is `runs/history.json`.

## Stages and isolation
`load → window → glycemic → meals → corrections → settings → iob → confounders →
trends → stats → backtest → snapshot → recommend → clamp → history`, then report
+ charts. Each stage is a pure `run(state, config) -> state`, so any stage can be
re-run from a saved artifact:
```sh
uv run python -m analysis stage glycemic --in runs/2026-07-30/stages/window.json
uv run python -m analysis stage report   --in runs/2026-07-30/stages/clamp.json
```

## settings.json
Dated history of your configured schedules; latest `effective_from <= as_of`
wins. Block keys are `HH-HH` (or coarse like `00-24`). CR = g/unit, CF = mg/dl
per unit. Optional `insulin_action_hours` sets the duration of insulin action for
IOB (defaults to 2). See `settings.example.json`. This file is git-ignored.

## Tests
```sh
uv run pytest                        # offline, deterministic
```
LLM-path evals hit the real model API and are skipped by default. To run the
rubric + safety-invariant checks on real model output:
```sh
RUN_LLM_EVALS=1 ZAI_API_KEY=... uv run pytest -m llm
uv run baml-cli test                 # BAML-native prompt evals (playground)
```

## Caveats
Decision support for review with your care team, not medical advice. CR/CF are
inferred, not read; effective CR can include a loop-added correction; CF is
inherently low-confidence on a closed loop. Prefer the 4-week window before
acting on CF.

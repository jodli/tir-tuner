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
from there). Use one subdirectory per export, named after its end date:
```sh
python strip_pii.py path/to/raw_export.zip ingest/2026-08-19
```

Glooko lets you export at most two weeks at a time, and it splits tables over
20000 rows into `cgm_data_2.csv`, `_3.csv`, ... The loader therefore reads *every*
matching CSV at any depth under the data dir and merges them, de-duplicating rows
that several exports carry. So the recommended cadence is: export two weeks every
week (one week of overlap), drop each into its own `ingest/<end-date>/`, and a
4-week window is assembled from them automatically. Where two exports disagree on
a timestamp, the newer export wins.

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
(per-stage artifacts), `charts/*.png`, and `settings_patch.json` when a change is
proposed. Cross-run history is `runs/history.json`.

The patch expresses the run's proposals in your *pump's* schedule blocks, so it is
both what to type into the pump and what to append to `settings.json` afterwards
(the next run's backtest uses it to tell whether the advice was applied).

## Stages and isolation
`load → window → glycemic → meals → corrections → settings → iob → confounders →
trends → stats → backtest → snapshot → recommend → clamp → verdicts → history`,
then report + charts. Each stage is a pure `run(state, config) -> state`, so any stage can be
re-run from a saved artifact:
```sh
uv run python -m analysis stage glycemic --in runs/2026-07-30/stages/window.json
uv run python -m analysis stage report   --in runs/2026-07-30/stages/clamp.json
```
Only `load.json` and `window.json` carry the raw CGM series; later artifacts drop
it and `stage` restores it from `window.json` in the same directory. Pass
`--full-artifacts` to write it everywhere (about 7x the disk).

## settings.json
Dated history of your configured schedules; latest `effective_from <= as_of`
wins. Block keys are `HH-HH` (or coarse like `00-24`). CR = g/unit, CF = mg/dl
per unit. Optional `insulin_action_hours` sets the duration of insulin action for
IOB (defaults to 2). See `settings.example.json`. This file is git-ignored.

Keep the block boundaries aligned with the analysis blocks (`00-06`, `06-11`,
`11-15`, `15-18`, `18-22`, `22-24`). An analysis block that straddles two
schedule entries with different values has no single configured value: the report
says so and drops the comparison for that block rather than picking one. Where a
schedule entry spans several analysis blocks, the report names the other blocks a
change would also affect.

## Reading the report
- `Datenabdeckung` is measured against the *requested* window, and missing or
  partial days are listed. A thin window makes every number below it weaker.
- `Hypo-E.` counts meals with a real hypo event (at least 15 min below 70, nadir
  at least 5 mg/dl under); `Dips` counts any brief excursion. Compare both with
  the `<70` column: many dips at a low TBR means short excursions, not too much
  meal insulin.
- `Wo TIR verloren geht` weights each block's out-of-range share by its time
  share, so it ranks blocks by pp of the overall shortfall. Every block gets a
  verdict, including "problem block, no CR lever" with the lever to check instead.
- Recommendations are split into `Jetzt umsetzen` and `Nur beobachten`, each
  saying which pump block to enter it in, and a standing proposal is marked with
  how many runs it has been open.

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

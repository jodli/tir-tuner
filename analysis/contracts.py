"""Typed input/output contracts for every pipeline stage.

Design goals
------------
* **Explicit stage boundaries.** Each stage consumes and returns a
  :class:`PipelineState`, filling in exactly one field. Any stage can therefore
  be re-run in isolation from a saved artifact.
* **Round-trippable to JSON.** A single generic (de)serializer
  (:func:`to_jsonable` / :func:`from_jsonable`) handles dataclasses, lists,
  dicts, optionals, datetimes and ``pandas.DataFrame`` fields, so no per-class
  boilerplate is needed and the round-trip is uniformly testable.

Field names, JSON keys and code identifiers stay English. Only *user-facing*
strings (report text, LLM narrative) are German; those are produced later in the
pipeline and are not part of these structural contracts.
"""
from __future__ import annotations

import dataclasses
import datetime as dt
import json
import math
import types
import typing
from dataclasses import dataclass, field
from typing import Optional

import pandas as pd

# ---------------------------------------------------------------------------
# Generic JSON (de)serialization
# ---------------------------------------------------------------------------
# A DataFrame is encoded as {"__dataframe__": {columns, datetime_cols, records}}
# so that column order and datetime dtypes survive the round-trip.
_DF_KEY = "__dataframe__"


def _df_to_payload(df: pd.DataFrame) -> dict:
    dt_cols = [c for c in df.columns if pd.api.types.is_datetime64_any_dtype(df[c])]
    out = df.copy()
    for c in dt_cols:
        out[c] = out[c].apply(lambda x: x.isoformat() if pd.notna(x) else None)
    # to_json normalizes numpy scalars to native types and NaN -> null; the
    # datetime columns are already ISO strings above so they stay strings.
    records = json.loads(out.to_json(orient="records"))
    return {"columns": list(df.columns), "datetime_cols": dt_cols, "records": records}


def _df_from_payload(payload: dict) -> pd.DataFrame:
    df = pd.DataFrame(payload["records"], columns=payload["columns"])
    for c in payload.get("datetime_cols", []):
        df[c] = pd.to_datetime(df[c])
    return df


def to_jsonable(x: typing.Any) -> typing.Any:
    """Recursively convert a contract object into JSON-native Python."""
    if x is None or isinstance(x, (str, int, bool)):
        return x
    if isinstance(x, float):
        return None if math.isnan(x) else x
    if isinstance(x, pd.DataFrame):
        return {_DF_KEY: _df_to_payload(x)}
    if isinstance(x, (dt.datetime, dt.date)):
        return x.isoformat()
    if dataclasses.is_dataclass(x) and not isinstance(x, type):
        return {f.name: to_jsonable(getattr(x, f.name)) for f in dataclasses.fields(x)}
    if isinstance(x, dict):
        return {k: to_jsonable(v) for k, v in x.items()}
    if isinstance(x, (list, tuple)):
        return [to_jsonable(v) for v in x]
    return x


def _is_union(origin: typing.Any) -> bool:
    return origin is typing.Union or origin is getattr(types, "UnionType", ())


def from_jsonable(tp: typing.Any, data: typing.Any) -> typing.Any:
    """Reconstruct a value of type ``tp`` from JSON-native ``data``."""
    if data is None:
        return None
    origin = typing.get_origin(tp)
    if _is_union(origin):
        args = [a for a in typing.get_args(tp) if a is not type(None)]
        return from_jsonable(args[0], data) if args else data
    if tp is pd.DataFrame:
        return _df_from_payload(data[_DF_KEY])
    if dataclasses.is_dataclass(tp) and isinstance(tp, type):
        hints = typing.get_type_hints(tp)
        kwargs = {}
        for f in dataclasses.fields(tp):
            if f.name in data:
                kwargs[f.name] = from_jsonable(hints[f.name], data[f.name])
            elif f.default is not dataclasses.MISSING:
                kwargs[f.name] = f.default
            elif f.default_factory is not dataclasses.MISSING:  # type: ignore[misc]
                kwargs[f.name] = f.default_factory()
            else:
                kwargs[f.name] = from_jsonable(hints[f.name], None)
        return tp(**kwargs)
    if origin in (list, tuple):
        args = typing.get_args(tp)
        itemtp = args[0] if args else typing.Any
        return [from_jsonable(itemtp, v) for v in data]
    if origin is dict:
        args = typing.get_args(tp)
        valtp = args[1] if len(args) == 2 else typing.Any
        return {k: from_jsonable(valtp, v) for k, v in data.items()}
    if tp is dt.datetime:
        return dt.datetime.fromisoformat(data)
    return data


class JsonMixin:
    """Adds uniform ``to_json``/``from_json`` to any dataclass contract."""

    def to_json(self) -> dict:
        return to_jsonable(self)

    @classmethod
    def from_json(cls, data: dict):
        return from_jsonable(cls, data)


# ---------------------------------------------------------------------------
# Time blocks (shared vocabulary across glycemic / meals / settings)
# ---------------------------------------------------------------------------
@dataclass(frozen=True)
class TimeBlock:
    name: str          # human label, e.g. "breakfast"
    start_hour: int    # inclusive
    end_hour: int      # exclusive (24 == end of day)

    @property
    def key(self) -> str:
        return f"{self.start_hour:02d}-{self.end_hour:02d}"


# Default day partition. Aligned to typical meal windows so per-block CR analysis
# maps onto breakfast / lunch / dinner behaviour.
DEFAULT_BLOCKS: list[TimeBlock] = [
    TimeBlock("night", 0, 6),
    TimeBlock("breakfast", 6, 11),
    TimeBlock("lunch", 11, 15),
    TimeBlock("afternoon", 15, 18),
    TimeBlock("dinner", 18, 22),
    TimeBlock("late", 22, 24),
]


def block_for_hour(hour: int, blocks: list[TimeBlock]) -> TimeBlock:
    for b in blocks:
        if b.start_hour <= hour < b.end_hour:
            return b
    # Fallback (should not happen with a full 0..24 partition).
    return blocks[-1]


# ---------------------------------------------------------------------------
# Configuration (tunables passed to every stage; not part of saved artifacts)
# ---------------------------------------------------------------------------
@dataclass
class Config:
    data_dir: str = "ingest"
    out_dir: str = "runs"
    settings_path: str = "settings.json"
    as_of: Optional[str] = None        # ISO date; default = last CGM day
    weeks: int = 4

    # Glycemic thresholds (mg/dl)
    tir_low: float = 70.0
    tir_high: float = 180.0
    tir_tight_high: float = 140.0      # upper bound for time-in-tight-range (70-140)
    vlow: float = 54.0
    high: float = 250.0

    # Meal analysis
    meal_clean_gap_min: int = 180      # no other meal bolus within +/- this
    excursion_peak_h: float = 3.0
    excursion_tail_h: float = 4.0
    hypo_start_h: float = 1.5          # post-meal hypo judged only from here on
                                       # (insulin tail), so early dips don't count
    cr_plausible_min: float = 1.0      # effective-CR outside [min,max] is a mis-log,
    cr_plausible_max: float = 50.0     # trimmed before aggregating (g/U)

    # Recommendation guardrails (the safety clamp)
    min_clean_meals: int = 5           # below this a CR block -> insufficient data
    min_isolated_corrections: int = 3  # below this a CF block -> insufficient data
    max_change_pct: float = 0.10       # never propose more than +/-10% per run

    # Insulin-on-board (IOB): fallback duration of insulin action when the user's
    # settings.json does not carry one. A configured value always wins.
    insulin_action_hours_default: float = 2.0
    iob_contamination_units: float = 1.0     # residual bolus IOB above this contaminates a meal
    # Delivery robustness: a bolus at least this large whose CGM rises and never
    # descends is treated as logged-but-not-absorbed (pump-to-body failure).
    no_delivery_min_units: float = 1.0
    no_delivery_expected_drop: float = 40.0  # mg/dl a delivered dose should have produced

    # Confounder detection (dawn phenomenon)
    dawn_start_h: int = 3
    dawn_end_h: int = 6
    dawn_min_rise_mgdl: float = 30.0
    dawn_min_nights: int = 3            # need this many qualifying nights to assert
    dawn_min_fraction: float = 0.5     # ... and this fraction of them rising

    # Robust statistics (bootstrap CI on effective CR)
    n_boot: int = 1000
    ci_pct: float = 90.0
    bootstrap_seed: int = 0            # fixed -> deterministic / testable
    min_ci_samples: int = 3

    # History / backtest
    history_series_len: int = 6        # prior runs surfaced to the reasoning step
    backtest_tir_epsilon: float = 3.0  # pp change counted as improved/worsened

    # Behaviour flags
    use_llm: bool = True
    make_charts: bool = True
    blocks: list[TimeBlock] = field(default_factory=lambda: list(DEFAULT_BLOCKS))


# ---------------------------------------------------------------------------
# Stage 1-2: raw + windowed data
# ---------------------------------------------------------------------------
@dataclass
class Dataset(JsonMixin):
    """Normalized measurement frames. Empty files yield empty (typed) frames.

    Columns:
      cgm:          time, mg_dl
      bolus:        time, kind ('meal'|'correction'), carbs, total_units,
                    delivered_u, initial_u, delayed_u, bg_entry
      basal:        time, duration_min, rate, delivered_u
      daily_totals: time, bolus_total, insulin_total, basal_total
      manual_bg:    time, mg_dl
    """
    cgm: pd.DataFrame
    bolus: pd.DataFrame
    basal: pd.DataFrame
    daily_totals: pd.DataFrame
    manual_bg: pd.DataFrame
    source_range: str = ""


@dataclass
class WindowInfo(JsonMixin):
    as_of: str          # ISO date (end of window, inclusive)
    weeks: int
    start: str          # ISO datetime
    end: str            # ISO datetime


# ---------------------------------------------------------------------------
# Stage 3: glycemic metrics
# ---------------------------------------------------------------------------
@dataclass
class GlycemicBand(JsonMixin):
    tir: Optional[float]        # % in 70-180
    tbr_70: Optional[float]     # % < 70
    tbr_54: Optional[float]     # % < 54
    tar_180: Optional[float]    # % > 180
    tar_250: Optional[float]    # % > 250
    mean: Optional[float]
    cv: Optional[float]         # % coefficient of variation
    gmi: Optional[float]        # glucose management indicator (%)
    n_readings: int
    titr: Optional[float] = None          # % in tight range 70-140
    coverage_pct: Optional[float] = None  # actual vs ~1/min readings over the span


@dataclass
class DayStat(JsonMixin):
    date: str
    tir: Optional[float]
    mean: Optional[float]
    n_readings: int


@dataclass
class GlycemicMetrics(JsonMixin):
    overall: GlycemicBand
    per_block: dict[str, GlycemicBand]
    per_day: list[DayStat]


# ---------------------------------------------------------------------------
# Stage 4: meals
# ---------------------------------------------------------------------------
@dataclass
class MealFeature(JsonMixin):
    time: str
    block: str                       # block key
    carbs: float
    units: float
    effective_cr: Optional[float]    # carbs / units
    baseline_mgdl: Optional[float]
    peak_rise: Optional[float]       # max(0..peak_h) - baseline
    delta_3h: Optional[float]
    delta_4h: Optional[float]
    min_0_4h: Optional[float]
    ended_in_range: Optional[bool]   # value at tail_h within [tir_low, tir_high]
    post_meal_hypo: Optional[bool]   # min over window < tir_low
    clean: bool                      # no overlapping meal bolus
    # Excursion shape (help tell "wrong CR" from "wrong timing / over-bolus")
    time_to_peak_min: Optional[float] = None    # minutes from meal to peak CGM
    auc_over_baseline: Optional[float] = None   # positive area over baseline (mg/dl*min)
    undershoot_depth: Optional[float] = None    # how far the nadir fell below tir_low
    undershoot_dur_min: Optional[float] = None  # minutes below tir_low in the tail
    rebound: Optional[float] = None             # recovery above nadir after a hypo


@dataclass
class MealBlockStats(JsonMixin):
    block: str
    n_clean: int
    median_effective_cr: Optional[float]
    median_peak_rise: Optional[float]
    pct_in_range: Optional[float]        # % of clean meals ending in range
    pct_post_meal_hypo: Optional[float]
    # Distribution of the effective-CR estimate (spread + sample the median hides)
    effective_cr_q25: Optional[float] = None
    effective_cr_q75: Optional[float] = None
    effective_cr_min: Optional[float] = None
    effective_cr_max: Optional[float] = None
    n_trimmed: int = 0                   # clean meals dropped as implausible CR
    # Median excursion-shape features over the clean meals
    median_time_to_peak_min: Optional[float] = None
    median_auc_over_baseline: Optional[float] = None
    median_undershoot_depth: Optional[float] = None
    # Effective CR split by starting glucose (a high start likely folded in a
    # correction, deflating the apparent CR)
    median_effective_cr_inrange_start: Optional[float] = None
    n_inrange_start: int = 0
    median_effective_cr_high_start: Optional[float] = None
    n_high_start: int = 0


@dataclass
class MealAnalysis(JsonMixin):
    meals: list[MealFeature]
    per_block: dict[str, MealBlockStats]


# ---------------------------------------------------------------------------
# Stage 5: corrections
# ---------------------------------------------------------------------------
@dataclass
class CorrectionFeature(JsonMixin):
    time: str
    block: str
    units: float
    start_mgdl: Optional[float]
    nadir_mgdl: Optional[float]
    observed_drop: Optional[float]       # start - nadir over window
    drop_per_unit: Optional[float]       # observed_drop / units
    isolated: bool                       # no meal within window
    confounds: list[str]


@dataclass
class CorrectionBlockStats(JsonMixin):
    block: str
    n_isolated: int
    median_drop_per_unit: Optional[float]


@dataclass
class CorrectionAnalysis(JsonMixin):
    corrections: list[CorrectionFeature]
    per_block: dict[str, CorrectionBlockStats]
    overall_median_drop_per_unit: Optional[float]


# ---------------------------------------------------------------------------
# Stage (iob): insulin-on-board + delivery robustness
# ---------------------------------------------------------------------------
@dataclass
class EventIob(JsonMixin):
    time: str
    kind: str                            # 'meal' | 'correction'
    units: float
    iob_before: Optional[float]          # residual bolus IOB active just before this bolus
    suspected_no_delivery: bool          # logged units but CGM rose and never descended


@dataclass
class IobAnalysis(JsonMixin):
    available: bool                      # False when no DIA is resolvable
    dia_hours: Optional[float]
    per_event: list[EventIob]


# ---------------------------------------------------------------------------
# Stage (confounders): non-CR patterns that would otherwise mislead the reasoning
# ---------------------------------------------------------------------------
@dataclass
class ConfounderFlags(JsonMixin):
    dawn_rise: bool                      # consistent pre-wake rise, not food/rebound
    dawn_magnitude: Optional[float]      # median overnight rise on rising nights (mg/dl)
    dawn_nights: int                     # qualifying (bolus-free, no-preceding-hypo) nights
    dawn_nights_rising: int


# ---------------------------------------------------------------------------
# Stage (stats): robust effective-CR estimation
# ---------------------------------------------------------------------------
@dataclass
class BlockRobustStats(JsonMixin):
    block: str
    n: int                               # trimmed effective-CR sample size
    median_effective_cr: Optional[float]
    ci_low: Optional[float]              # bootstrap CI on the median
    ci_high: Optional[float]
    delta_vs_prior: Optional[float]      # current median - most-recent prior median
    delta_significant: Optional[bool]    # prior median outside the current CI
    cr_iob_filtered: Optional[float]     # median excluding contaminated / no-delivery meals
    n_high_iob: int
    n_suspected_no_delivery: int


@dataclass
class RobustStats(JsonMixin):
    per_block: dict[str, BlockRobustStats]


# ---------------------------------------------------------------------------
# Stage (backtest): did the previous run's recommendations play out?
# ---------------------------------------------------------------------------
@dataclass
class BlockBacktest(JsonMixin):
    block: str
    had_reco: bool
    parameter: Optional[str]             # "CR" | "CF"
    direction: Optional[str]             # "up" | "down" | "hold"
    applied: Optional[bool]              # configured value actually moved that way
    tir_before: Optional[float]
    tir_after: Optional[float]
    outcome: str                         # "improved"|"worsened"|"unchanged"|"unknown"


@dataclass
class BacktestAnalysis(JsonMixin):
    prior_run_date: Optional[str]
    per_block: dict[str, BlockBacktest]


# ---------------------------------------------------------------------------
# Stage 6: resolved settings (ground truth, if provided)
# ---------------------------------------------------------------------------
@dataclass
class DatedSchedule(JsonMixin):
    effective_from: str              # ISO date
    blocks: dict[str, float]         # block key -> value


@dataclass
class SettingsHistory(JsonMixin):
    carb_ratio: list[DatedSchedule]
    correction_factor: list[DatedSchedule]
    insulin_action_hours: Optional[float] = None   # duration of insulin action (h)


@dataclass
class CrChange(JsonMixin):
    """The most recent configured-CR change in a block, so an outcome can be
    lined up against the setting change that plausibly caused it."""
    block: str
    effective_from: str
    from_value: float
    to_value: float


@dataclass
class ResolvedSettings(JsonMixin):
    as_of: str
    available: bool                          # False if no settings.json
    carb_ratio: dict[str, float]             # block key -> configured g/U
    correction_factor: dict[str, float]      # block key -> configured mg/dl per U
    change_dates: list[str]                  # every effective_from (chart markers)
    insulin_action_hours: Optional[float] = None   # resolved DIA (configured or default)
    cr_changes: list[CrChange] = field(default_factory=list)  # most recent CR change per block


# ---------------------------------------------------------------------------
# Stage 7: analysis snapshot (the LLM's typed input)
# ---------------------------------------------------------------------------
@dataclass
class BlockEvidence(JsonMixin):
    block: str
    label: str
    configured_cr: Optional[float]
    effective_cr: Optional[float]
    cr_gap_pct: Optional[float]              # (effective - configured) / configured
    n_clean_meals: int
    median_peak_rise: Optional[float]
    pct_in_range: Optional[float]
    pct_post_meal_hypo: Optional[float]
    tir: Optional[float]
    tbr_70: Optional[float]
    tar_180: Optional[float]
    trend_effective_cr_delta: Optional[float]
    trend_tir_delta: Optional[float]
    # Variability + data quality
    cv: Optional[float] = None
    tbr_54: Optional[float] = None
    titr: Optional[float] = None
    coverage_pct: Optional[float] = None
    # Distribution + shape of the effective-CR / excursion sample
    effective_cr_q25: Optional[float] = None
    effective_cr_q75: Optional[float] = None
    median_time_to_peak_min: Optional[float] = None
    median_auc_over_baseline: Optional[float] = None
    median_undershoot_depth: Optional[float] = None
    median_effective_cr_inrange_start: Optional[float] = None
    n_inrange_start: int = 0
    # Robust estimate (bootstrap CI + significance + contamination-filtered)
    ci_low: Optional[float] = None
    ci_high: Optional[float] = None
    delta_significant: Optional[bool] = None
    cr_iob_filtered: Optional[float] = None
    n_high_iob: int = 0
    n_suspected_no_delivery: int = 0
    # Confounder + causal-attribution + backtest context
    dawn_rise: Optional[bool] = None
    config_last_change: Optional[CrChange] = None
    backtest_outcome: Optional[str] = None


@dataclass
class CorrectionEvidence(JsonMixin):
    block: str
    configured_cf: Optional[float]
    observed_drop_per_unit: Optional[float]
    n_isolated: int
    confounds: list[str]
    n_suspected_no_delivery: int = 0         # corrections that likely never delivered


@dataclass
class AnalysisSnapshot(JsonMixin):
    as_of: str
    window_weeks: int
    overall_tir: Optional[float]
    overall_tbr_70: Optional[float]
    overall_tar_180: Optional[float]
    overall_mean: Optional[float]
    overall_cv: Optional[float]
    overall_gmi: Optional[float]
    settings_available: bool
    blocks: list[BlockEvidence]
    corrections: list[CorrectionEvidence]
    prior_run_date: Optional[str]
    # Multi-run series (compact per-run refs, most recent last) for trend/arc reasoning
    prior_runs: list[RunRef] = field(default_factory=list)
    # Honesty ledger: what cannot be observed, and which derived flags fired
    unavailable_signals: list[str] = field(default_factory=list)
    active_flags: list[str] = field(default_factory=list)


# ---------------------------------------------------------------------------
# Stage 8-9: recommendations (LLM output) + clamp audit
# ---------------------------------------------------------------------------
@dataclass
class Proposal(JsonMixin):
    block: str
    parameter: str            # "CR" | "CF"
    direction: str            # "down" | "up" | "hold"
    current_value: Optional[float]
    proposed_value: Optional[float]
    confidence: str           # "low" | "medium" | "high"
    rationale: str            # German (user-facing)
    caveats: str              # German (user-facing)


@dataclass
class RecommendationSet(JsonMixin):
    proposals: list[Proposal]
    overall_narrative: str                 # German (user-facing)
    insufficient_data_blocks: list[str]


@dataclass
class ClampAudit(JsonMixin):
    block: str
    parameter: str
    field: str                # which attribute was adjusted
    original: str
    adjusted: str
    reason: str


# ---------------------------------------------------------------------------
# Stage 10: history / trends
# ---------------------------------------------------------------------------
@dataclass
class RunRef(JsonMixin):
    """Compact per-run record kept in runs/history.json (aggregate only, no raw CGM)."""
    as_of: str
    overall_tir: Optional[float]
    per_block_effective_cr: dict[str, float]
    per_block_tir: dict[str, float]
    # Configured CR in effect at this run, so the multi-run series can show the
    # setting alongside its effect. Defaulted for backward-compatible history.json.
    per_block_configured_cr: dict[str, float] = field(default_factory=dict)


@dataclass
class Trends(JsonMixin):
    prior: list[RunRef]        # earlier runs (most recent last), excluding current
    current: RunRef


# ---------------------------------------------------------------------------
# The accumulating pipeline state (single serialized artifact per stage)
# ---------------------------------------------------------------------------
@dataclass
class PipelineState(JsonMixin):
    dataset: Optional[Dataset] = None                    # after load / window
    window: Optional[WindowInfo] = None                  # after window
    glycemic: Optional[GlycemicMetrics] = None           # after glycemic
    meals: Optional[MealAnalysis] = None                 # after meals
    corrections: Optional[CorrectionAnalysis] = None     # after corrections
    settings: Optional[ResolvedSettings] = None          # after settings
    iob: Optional[IobAnalysis] = None                    # after iob
    confounders: Optional[ConfounderFlags] = None        # after confounders
    stats: Optional[RobustStats] = None                  # after stats
    backtest: Optional[BacktestAnalysis] = None          # after backtest
    snapshot: Optional[AnalysisSnapshot] = None          # after snapshot
    recommendation_raw: Optional[RecommendationSet] = None   # after recommend
    recommendation: Optional[RecommendationSet] = None       # after clamp
    clamp_audit: list[ClampAudit] = field(default_factory=list)
    trends: Optional[Trends] = None                      # after history

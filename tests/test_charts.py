"""Chart rendering: the axes must be real dates and thin blocks must be dropped."""
import datetime as dt

import matplotlib.dates as mdates

from analysis import charts
from analysis.contracts import (
    Config,
    GlycemicBand,
    GlycemicMetrics,
    PipelineState,
    ResolvedSettings,
    RunRef,
    Trends,
    WindowInfo,
)


def _ref(as_of, tir, eff, n, configured=11.0):
    return RunRef(as_of=as_of, overall_tir=tir,
                  per_block_effective_cr={"06-11": eff, "00-06": 30.0},
                  per_block_tir={"06-11": 80.0},
                  per_block_configured_cr={"06-11": configured},
                  per_block_n_clean_meals={"06-11": n, "00-06": 2})


def _state(change_dates):
    st = PipelineState()
    st.window = WindowInfo(as_of="2026-08-19", weeks=4,
                           start="2026-07-23T00:00:00", end="2026-08-20T00:00:00")
    st.glycemic = GlycemicMetrics(
        overall=GlycemicBand(75.0, 3.0, 0.3, 21.0, 4.0, 140.0, 38.0, 6.6, 100),
        per_block={}, per_day=[])
    st.settings = ResolvedSettings(as_of="2026-08-19", available=True, carb_ratio={},
                                   correction_factor={}, change_dates=change_dates)
    st.trends = Trends(prior=[_ref("2026-08-05", 78.0, 10.3, 12, 10.0),
                              _ref("2026-08-12", 78.8, 11.2, 11)],
                       current=_ref("2026-08-19", 75.3, 11.0, 9))
    return st


def _capture(monkeypatch):
    captured = {}

    def fake_save(fig, out_dir, name):
        captured[name] = fig
        return name

    monkeypatch.setattr(charts, "_save", fake_save)
    return captured


def test_setting_change_marker_stays_inside_the_run_range(monkeypatch, tmp_path):
    """On a string x axis the marker became a new category after the last run."""
    captured = _capture(monkeypatch)
    charts._chart_tir_trend(_state(["2026-08-10"]), Config(), str(tmp_path))
    ax = captured["tir_trend.png"].axes[0]
    # Axis margins add a little slack; the categorical bug put the marker days
    # past the last run, which moved the limit far outside this range.
    lo, hi = (mdates.num2date(x).date() for x in ax.get_xlim())
    assert dt.date(2026, 8, 3) <= lo and hi <= dt.date(2026, 8, 21)
    # The marker is drawn and labelled once.
    labels = [t.get_text() for t in ax.get_legend().get_texts()]
    assert "Einstellungsänderung" in labels


def test_change_marker_outside_the_range_is_skipped(monkeypatch, tmp_path):
    captured = _capture(monkeypatch)
    charts._chart_tir_trend(_state(["2026-01-01"]), Config(), str(tmp_path))
    ax = captured["tir_trend.png"].axes[0]
    lo, hi = (mdates.num2date(x).date() for x in ax.get_xlim())
    assert dt.date(2026, 8, 3) <= lo and hi <= dt.date(2026, 8, 21)
    assert ax.get_legend() is not None and len(ax.get_legend().get_texts()) == 1


def test_cr_trend_drops_blocks_below_the_sample_gate_and_shows_configured(monkeypatch, tmp_path):
    captured = _capture(monkeypatch)
    charts._chart_effective_cr_trend(_state([]), Config(), str(tmp_path))
    ax = captured["effective_cr_trend.png"].axes[0]
    labels = [t.get_text() for t in ax.get_legend().get_texts()]
    assert labels == ["Frühstück"]           # night block has n=2, below the gate
    # One effective-CR line plus one configured-CR step, in the same colour.
    assert len(ax.lines) == 2
    assert ax.lines[0].get_color() == ax.lines[1].get_color()
    assert max(ax.get_ylim()) < 20           # the n=2 block no longer squashes the axis

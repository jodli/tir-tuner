import os

import pandas as pd
import pytest

from analysis.contracts import Config, Dataset

FIXTURES = os.path.join(os.path.dirname(__file__), "fixtures")


def cgm_df(pairs):
    """Build a CGM frame from (timestamp_str, mg_dl) pairs."""
    times = pd.to_datetime([p[0] for p in pairs])
    return pd.DataFrame({"time": times, "mg_dl": [float(p[1]) for p in pairs]})


def bolus_df(rows):
    """Build a bolus frame from dicts (time, kind, carbs, total_units, ...)."""
    cols = ["time", "kind", "carbs", "total_units", "delivered_u", "initial_u", "delayed_u", "bg_entry"]
    if not rows:
        return pd.DataFrame({c: pd.Series(dtype="datetime64[ns]" if c == "time" else ("object" if c == "kind" else "float64")) for c in cols})
    df = pd.DataFrame(rows)
    df["time"] = pd.to_datetime(df["time"])
    for c in cols:
        if c not in df.columns:
            df[c] = None
    return df[cols]


def _empty(cols):
    return pd.DataFrame({c: pd.Series(dtype=t) for c, t in cols.items()})


def make_dataset(cgm=None, bolus=None, source_range=""):
    """Assemble a Dataset with the given cgm/bolus frames and empty remainder."""
    return Dataset(
        cgm=cgm if cgm is not None else _empty({"time": "datetime64[ns]", "mg_dl": "float64"}),
        bolus=bolus if bolus is not None else bolus_df([]),
        basal=_empty({"time": "datetime64[ns]", "duration_min": "float64", "rate": "float64", "delivered_u": "float64"}),
        daily_totals=_empty({"time": "datetime64[ns]", "bolus_total": "float64", "insulin_total": "float64", "basal_total": "float64"}),
        manual_bg=_empty({"time": "datetime64[ns]", "mg_dl": "float64"}),
        source_range=source_range,
    )


@pytest.fixture
def sample_dir():
    return os.path.join(FIXTURES, "sample_export")


@pytest.fixture
def empty_dir():
    return os.path.join(FIXTURES, "empty_export")


@pytest.fixture
def sample_config(sample_dir):
    return Config(data_dir=sample_dir)

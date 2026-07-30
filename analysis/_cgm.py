"""Small CGM lookup helper shared by the meal and correction stages.

Wraps the windowed CGM series in numpy arrays for fast point/interval queries:
the nearest reading to a timestamp, and the values within an offset interval.
"""
from __future__ import annotations

import numpy as np
import pandas as pd


class CgmLookup:
    def __init__(self, cgm: pd.DataFrame):
        if len(cgm):
            self.t = cgm["time"].to_numpy(dtype="datetime64[ns]")
            self.v = cgm["mg_dl"].to_numpy(dtype=float)
        else:
            self.t = np.array([], dtype="datetime64[ns]")
            self.v = np.array([], dtype=float)

    def nearest(self, target, tol_min: int = 15):
        """Nearest reading to ``target`` within ``tol_min`` minutes, else None."""
        if len(self.t) == 0:
            return None
        target = np.datetime64(pd.Timestamp(target))
        tol = np.timedelta64(tol_min, "m")
        idx = int(np.searchsorted(self.t, target))
        best, best_d = None, None
        for j in (idx - 1, idx):
            if 0 <= j < len(self.t):
                d = abs(self.t[j] - target)
                if d <= tol and (best_d is None or d < best_d):
                    best, best_d = j, d
        return float(self.v[best]) if best is not None else None

    def at_offset(self, t0, hours: float, tol_min: int = 15):
        target = pd.Timestamp(t0) + pd.Timedelta(minutes=round(hours * 60))
        return self.nearest(target, tol_min)

    def window(self, t0, hours: float) -> np.ndarray:
        """Readings in the half-open interval (t0, t0 + hours]."""
        return self.window_between(t0, 0.0, hours)

    def window_between(self, t0, start_h: float, end_h: float) -> np.ndarray:
        """Readings in the half-open interval (t0 + start_h, t0 + end_h]."""
        if len(self.t) == 0:
            return np.array([], dtype=float)
        base = pd.Timestamp(t0)
        a = np.datetime64(base + pd.Timedelta(minutes=round(start_h * 60)))
        b = np.datetime64(base + pd.Timedelta(minutes=round(end_h * 60)))
        mask = (self.t > a) & (self.t <= b)
        return self.v[mask]

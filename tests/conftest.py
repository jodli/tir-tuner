import os

import pytest

from analysis.contracts import Config

FIXTURES = os.path.join(os.path.dirname(__file__), "fixtures")


@pytest.fixture
def sample_dir():
    return os.path.join(FIXTURES, "sample_export")


@pytest.fixture
def empty_dir():
    return os.path.join(FIXTURES, "empty_export")


@pytest.fixture
def sample_config(sample_dir):
    return Config(data_dir=sample_dir)

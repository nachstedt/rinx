"""Runs pytest the way `uv run pytest` does, as the `main` of each script test.

Bazel's `py_test` runs a script, so this is the script. It passes on its
arguments — `-c pyproject.toml` and the one test file the target names — and
pytest takes everything else (`pythonpath`, `filterwarnings`, ...) from that
configuration.
"""

import sys

import pytest

if __name__ == "__main__":
    # No cache directory: the runfiles tree Bazel runs a test in is read-only.
    sys.exit(pytest.main([*sys.argv[1:], "-p", "no:cacheprovider"]))

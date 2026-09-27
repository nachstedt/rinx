# Python in rinx

rinx is Rust, but a handful of Python scripts live in `scripts/`: the two
benchmarks (`benchmark.py`, `benchmark_entities.py`, sharing
`benchmark_common.py` and `needs_schema.py`), the crates.io publisher
(`publish_crates.py`) and the doctest runner (`doctest_runner.py`). The
examples in `examples/shared/` are Python too.

## Tooling

[uv](https://docs.astral.sh/uv/) manages the tooling. `pyproject.toml` holds all
of the configuration, and `uv.lock` pins every version, so CI and every
checkout run the same ruff, ty and pytest.

```bash
uv sync                    # install the pinned tools into .venv
uv run ruff format         # format
uv run ruff check          # lint (every rule, minus the ones pyproject.toml argues away)
uv run ty check            # type-check
uv run pytest --cov        # run the tests, failing below the coverage floor
```

The `python` job in `.github/workflows/ci.yml` runs all of these with
`uv sync --locked`, so a `pyproject.toml` change without a `uv lock` fails CI.

- **Every function is annotated**, which ruff's `ANN` rules enforce and ty then
  checks. JSON read from a file is given a `TypedDict` where it is read (the
  doctest plan, `cargo metadata`, a warning sidecar), so a misspelled key is a
  type error rather than a `KeyError` at run time.
- **Lint suppressions are a last resort.** A rule is ignored in
  `pyproject.toml` only with a comment saying why, and an inline `# noqa` names
  its reason too. A complexity or argument-count finding is a refactoring
  signal, the same as clippy's.
- **ty is pinned.** It is pre-1.0, so a new release can add diagnostics. An
  upgrade arrives as a Dependabot pull request and is fixed there.

## Two scripts are stdlib-only, one of them on Python 3.9

`pyproject.toml` declares no runtime dependencies, and two scripts depend on
that:

- `doctest_runner.py` is shipped. `rinx_doctest_tests` runs it inside
  *users'* `py_test`s, on whichever interpreter their toolchain provides. It
  therefore stays importable on **Python 3.9**. `per-file-target-version` makes
  ruff hold it and its test to that version, and CI runs its test on a 3.9
  interpreter. Typing-only names (`Self`, `CodeType`) are imported under
  `TYPE_CHECKING`, behind `from __future__ import annotations`.
- `publish_crates.py` runs as a bare `python3 scripts/publish_crates.py` in the
  release workflow, with nothing installed.

Everything else targets the Python 3.11 that `MODULE.bazel`'s toolchain and
`.python-version` both name.

## Tests run twice

Tests are pytest files named `*_test.py`. They run in two places, and the same
way in both:

- `uv run pytest`, for the edit loop and coverage;
- `bazel test //scripts:all`, one `py_test` per test file (generated from a
  `glob`, so a new `*_test.py` needs no BUILD change), each cached on its own.
  Its `main`, `scripts/pytest_main.py`, runs pytest with `-c pyproject.toml`
  and the file, so the settings are the ones uv uses (`pythonpath`,
  `filterwarnings = ["error"]`, strict markers) and a test cannot pass in one
  and fail in the other because of configuration.

Bazel resolves pytest from `scripts/requirements.txt` through the dev-only
`@pypi` hub, so a user of the rinx module never fetches it. That file is
**generated** from `uv.lock`. After changing the `test` dependency group, or
after a Dependabot bump, regenerate it:

```bash
uv export --only-group test --no-emit-project --format requirements-txt \
  -o scripts/requirements.txt
```

CI regenerates it and fails on any difference.

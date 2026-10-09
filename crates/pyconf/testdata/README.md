# `conf.py` fixtures

Real configuration files `rinx_pyconf` must read, checked by `tests/fixtures.rs`.
A hand-written test suite misses the shapes real projects write; these are
the bar.

- `cpython-3.14.2-conf.py` is CPython's `Doc/conf.py` at the `v3.14.2` tag,
  unmodified (Python Software Foundation License, as the rest of CPython's
  documentation). It holds every shape the reader must not mistake for a
  literal: an import-computed `version, release`, f-string `rst_epilog`s,
  `exclude_patterns.append(…)` inside an `if`, `nitpick_ignore += […]`, and
  a `for` loop appending to `extensions`.
- `sphinx-9.1.0-quickstart-conf.py` is what `sphinx-quickstart` 9.1.0
  writes, every setting a literal:

  ```bash
  python3 -m venv venv && ./venv/bin/pip install sphinx==9.1.0
  ./venv/bin/sphinx-quickstart -q -p Example -a "Example Author" -v 1.0 --sep qs
  cp qs/source/conf.py sphinx-9.1.0-quickstart-conf.py
  ```

The `conf.py` of `rinx_inventory`'s fixture project
(`crates/inventory/testdata/sphinx-9.1.0-src/conf.py`) is read in place
rather than copied.

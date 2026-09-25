# Inventory fixtures

`sphinx-9.1.0.inv` is the `objects.inv` a real `sphinx-build -b html` 9.1.0
wrote for the project in `sphinx-9.1.0-src/` (plus an empty `logo.png`). It is
the compatibility bar for `rinx_inventory`: the reader must load it and
the writer must produce what Sphinx's own reader accepts.

Regenerate with:

```bash
python3 -m venv venv && ./venv/bin/pip install sphinx==9.1.0
cp -r sphinx-9.1.0-src src && touch src/logo.png
./venv/bin/sphinx-build -q -b html src out
cp out/objects.inv sphinx-9.1.0.inv
```

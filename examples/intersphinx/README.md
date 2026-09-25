# Intersphinx examples

`python.inv` holds 19 entries copied verbatim — names, types, priorities, URIs
and display names untouched — from Python 3.13's real inventory, one or two of
each kind a role here can reach. It is a subset only to keep the repository
small; a real project would pin the whole file.

Regenerate with:

```bash
curl -sSfL -o python-3.13.inv https://docs.python.org/3.13/objects.inv
# then keep the lines listed in examples/intersphinx/index.rst, recompressing
# the body with zlib behind the unchanged four-line header
```

The `base_url` in `BUILD.bazel` names the same version, so every anchor the
inventory lists exists on the page it links to.

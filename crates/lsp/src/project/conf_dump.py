"""Runs a Sphinx project's ``conf.py`` and prints the settings rinx reads.

The language server reads ``conf.py`` without running it (ADR-038 §4). In a
trusted workspace that allows it, it runs this script in the project's own
interpreter instead, for the exact value of every setting the file computes
rather than spells as a literal::

    python -c <this file's text> <conf.py> <names, a JSON list>

``conf.py`` runs as Sphinx's ``eval_config_file`` runs it: in its own
directory, with ``__file__`` and ``tags`` bound. ``tags`` is empty, since
Sphinx adds a builder's tags only after it has read the configuration, and an
editor passes no ``-t``. The script then prints one line of JSON::

    {"values": {name: repr(value)}, "imported": [path, ...], "error": null}

- ``values`` holds each name asked for that ``conf.py`` binds, as ``repr``
  spells it. The server reads that spelling with its own literal reader, so a
  value no literal spells (an object, a function) is refused there, by name,
  rather than guessed at here.
- ``imported`` lists the files of the modules ``conf.py`` imported that are no
  part of the interpreter's installation, so the server runs it again when one
  of them changes.
- ``error`` is ``{"message": ..., "line": ...}`` when running ``conf.py``
  raised, ``line`` counting in ``conf.py`` itself (``null`` when the error is
  elsewhere), and ``values`` is then empty.

What ``conf.py`` prints goes to standard error, so the one line on standard
output stays the server's.

The script is embedded in the ``rinx`` binary and runs on whichever
interpreter a project uses, so it stays stdlib-only and Python 3.9-compatible.
"""

from __future__ import annotations

import importlib
import json
import os
import site
import sys
import sysconfig
import traceback
from pathlib import Path
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from collections.abc import Iterator

# The installation's own directories, whose modules are no part of a project.
INSTALLATION_PATHS = ("stdlib", "platstdlib", "purelib", "platlib")


class Tags:
    """Sphinx's ``tags``, for an interpreter without Sphinx installed."""

    def __init__(self) -> None:
        """No tags: an editor passes no ``-t``."""
        self._tags: set[str] = set()

    def has(self, tag: str) -> bool:
        """Whether ``tag`` is set."""
        return tag in self._tags

    def add(self, tag: str) -> None:
        """Sets ``tag``."""
        self._tags.add(tag)

    def remove(self, tag: str) -> None:
        """Unsets ``tag``, set or not."""
        self._tags.discard(tag)

    def __contains__(self, tag: object) -> bool:
        """Whether ``tag`` is set, as ``'html' in tags`` asks."""
        return tag in self._tags

    def __iter__(self) -> Iterator[str]:
        """The tags set."""
        return iter(self._tags)


def make_tags() -> object:
    """Sphinx's ``Tags``, when the interpreter has Sphinx, else the stand-in."""
    try:
        tags = importlib.import_module("sphinx.util.tags")
    except ImportError:
        return Tags()
    return tags.Tags()


def plain(value: object) -> object:
    """``value`` with every container and string its built-in type, and every path a string.

    Sphinx takes a path wherever it takes a string, and an ``OrderedDict``
    wherever it takes a ``dict``, whose ``repr`` no literal spells.
    """
    if isinstance(value, os.PathLike):
        return os.fspath(value)
    if isinstance(value, str):
        return str(value)
    if isinstance(value, dict):
        return {plain(key): plain(item) for key, item in value.items()}
    if isinstance(value, list):
        return [plain(item) for item in value]
    if isinstance(value, tuple):
        return tuple(plain(item) for item in value)
    return value


def spelling(value: object) -> str:
    """``repr`` of ``value``, made plain, or a spelling no literal reader takes."""
    try:
        return repr(plain(value))
    except Exception as error:
        return f"<repr failed: {error}>"


def installation_directories() -> list[Path]:
    """The directories the interpreter's own modules are installed in."""
    paths = sysconfig.get_paths()
    directories = [paths[key] for key in INSTALLATION_PATHS if key in paths]
    user_site = getattr(site, "getusersitepackages", None)
    if user_site is not None:
        directories.append(user_site())
    return [Path(directory).absolute() for directory in directories]


def is_under(path: Path, directories: list[Path]) -> bool:
    """Whether ``path`` is one of ``directories`` or lies under one."""
    return any(path == directory or directory in path.parents for directory in directories)


def imported_files(before: set[str]) -> list[str]:
    """The files of the modules imported since ``before``, outside the installation."""
    installed = installation_directories()
    files = set()
    for name, module in list(sys.modules.items()):
        file = getattr(module, "__file__", None)
        if name in before or not isinstance(file, str):
            continue
        path = Path(file).absolute()
        if not is_under(path, installed):
            files.add(str(path))
    return sorted(files)


def error_of(error: BaseException, conf: str) -> dict[str, Any]:
    """What ``error``, raised running ``conf``, says, and the line of ``conf`` it was raised on."""
    message = traceback.format_exception_only(type(error), error)[-1].strip()
    line = None
    if isinstance(error, SyntaxError) and error.filename == conf:
        line = error.lineno
    else:
        for frame in reversed(traceback.extract_tb(error.__traceback__)):
            if frame.filename == conf:
                line = frame.lineno
                break
    return {"message": message, "line": line}


def run_conf(conf: str, names: list[str]) -> dict[str, Any]:
    """Runs ``conf``, an absolute path, returning what the server reads of it."""
    namespace: dict[str, Any] = {"__file__": conf, "tags": make_tags()}
    before = set(sys.modules)
    previous = Path.cwd()
    error = None
    try:
        os.chdir(Path(conf).parent)
        source = Path(conf).read_bytes()
        exec(compile(source, conf, "exec"), namespace)
    except (Exception, SystemExit) as raised:
        error = error_of(raised, conf)
    finally:
        os.chdir(previous)
    values = {}
    if error is None:
        values = {name: spelling(namespace[name]) for name in names if name in namespace}
    return {"values": values, "imported": imported_files(before), "error": error}


def main(argv: list[str]) -> int:
    """Runs the ``conf.py`` ``argv[1]`` names, printing the names ``argv[2]`` lists."""
    conf = str(Path(argv[1]).absolute())
    names = json.loads(argv[2])
    # `python -c` puts the working directory first on the path, where
    # `sphinx-build` puts its own; `conf.py` adds what it needs itself.
    if sys.path and not sys.path[0]:
        del sys.path[0]
    sys.argv = [conf]
    output = sys.stdout
    sys.stdout = sys.stderr
    try:
        result = run_conf(conf, names)
    finally:
        sys.stdout = output
    output.write(json.dumps(result) + "\n")
    output.flush()
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))

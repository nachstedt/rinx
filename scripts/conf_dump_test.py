"""Tests for the `conf.py` runner the language server embeds (`conf_dump.py`)."""

from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path
from typing import TYPE_CHECKING, Any

import pytest

import conf_dump

if TYPE_CHECKING:
    from collections.abc import Iterator

SCRIPT = Path(conf_dump.__file__).read_text(encoding="utf-8")


@pytest.fixture(autouse=True)
def isolated_interpreter(monkeypatch: pytest.MonkeyPatch) -> Iterator[None]:
    """Restores what running a `conf.py` changes: the path, `sys.argv` and the modules imported."""
    monkeypatch.setattr(sys, "argv", list(sys.argv))
    monkeypatch.setattr(sys, "path", list(sys.path))
    before = set(sys.modules)
    yield
    for name in set(sys.modules) - before:
        del sys.modules[name]


def write_conf(directory: Path, text: str) -> Path:
    """A `conf.py` holding `text` in `directory`."""
    conf = directory / "conf.py"
    conf.write_text(text, encoding="utf-8")
    return conf


def dump(conf: Path, names: list[str], capsys: pytest.CaptureFixture[str]) -> dict[str, Any]:
    """What the runner prints for `conf`, asked for `names`."""
    status = conf_dump.main(["-c", str(conf), json.dumps(names)])
    assert status == 0
    return json.loads(capsys.readouterr().out)


def test_main_prints_each_computed_setting_as_its_repr(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    # Given
    conf = write_conf(
        tmp_path,
        "version = '3.14'\n"
        "rst_epilog = f'.. |version| replace:: {version}'\n"
        "extensions = ['sphinx.ext.' + name for name in ('intersphinx', 'todo')]\n"
        "intersphinx_mapping = {'python': ('https://docs.python.org/3', None)}\n",
    )

    # When
    output = dump(conf, ["rst_epilog", "extensions", "intersphinx_mapping", "numfig"], capsys)

    # Then
    assert output == {
        "values": {
            "rst_epilog": "'.. |version| replace:: 3.14'",
            "extensions": "['sphinx.ext.intersphinx', 'sphinx.ext.todo']",
            "intersphinx_mapping": "{'python': ('https://docs.python.org/3', None)}",
        },
        "imported": [],
        "error": None,
    }


def test_main_runs_conf_py_in_its_own_directory_with_its_file_and_tags(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    # Given
    conf = write_conf(
        tmp_path,
        "import os\n"
        "here = os.getcwd()\n"
        "file = __file__\n"
        "html = tags.has('html') or 'html' in tags\n",
    )

    # When
    values = dump(conf, ["here", "file", "html"], capsys)["values"]

    # Then
    assert values == {
        "here": repr(str(tmp_path.resolve())),
        "file": repr(str(conf)),
        "html": "False",
    }
    assert Path.cwd() != tmp_path


def test_main_sends_what_conf_py_prints_to_standard_error(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    # Given
    conf = write_conf(tmp_path, "print('configuring')\nproject = 'P'\n")

    # When
    conf_dump.main(["-c", str(conf), json.dumps(["project"])])

    # Then
    captured = capsys.readouterr()
    assert json.loads(captured.out)["values"] == {"project": "'P'"}
    assert "configuring" in captured.err


def test_main_reports_an_exception_at_its_line_in_conf_py(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    # Given
    conf = write_conf(tmp_path, "project = 'P'\n\nimport not_a_module_rinx_knows\n")

    # When
    output = dump(conf, ["project"], capsys)

    # Then
    assert output["values"] == {}
    assert output["error"] == {
        "message": "ModuleNotFoundError: No module named 'not_a_module_rinx_knows'",
        "line": 3,
    }


def test_main_reports_a_syntax_error_and_an_exit_at_their_lines(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    # Given
    syntax = write_conf(tmp_path, "project = 'P'\nproject = (\n")
    exiting = tmp_path / "exits"
    exiting.mkdir()
    exits = write_conf(exiting, "import sys\nsys.exit(2)\n")

    # When / Then
    assert dump(syntax, [], capsys)["error"]["line"] == 2
    assert dump(exits, [], capsys)["error"] == {"message": "SystemExit: 2", "line": 2}


def test_main_reports_an_error_raised_elsewhere_without_a_line(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    # Given
    (tmp_path / "helper_rinx_raises.py").write_text("raise ValueError('bad')\n", encoding="utf-8")
    conf = write_conf(tmp_path, "import sys\nsys.path.insert(0, '.')\nimport helper_rinx_raises\n")

    # When
    error = dump(conf, [], capsys)["error"]

    # Then
    assert error == {"message": "ValueError: bad", "line": 3}


def test_main_lists_the_project_modules_conf_py_imported(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    # Given
    (tmp_path / "version_rinx_reads.py").write_text("VERSION = '1.2'\n", encoding="utf-8")
    conf = write_conf(
        tmp_path,
        "import sys, os, json\n"
        "sys.path.insert(0, os.path.abspath('.'))\n"
        "from version_rinx_reads import VERSION\n"
        "release = VERSION\n",
    )

    # When
    output = dump(conf, ["release"], capsys)

    # Then
    assert output["values"] == {"release": "'1.2'"}
    assert output["imported"] == [str((tmp_path / "version_rinx_reads.py").resolve())]


def test_main_leaves_out_the_names_conf_py_does_not_bind(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    # Given
    conf = write_conf(tmp_path, "project = 'P'\n")

    # When / Then
    assert dump(conf, ["project", "numfig"], capsys)["values"] == {"project": "'P'"}


def test_plain_turns_paths_and_subclasses_into_built_in_values() -> None:
    # Given
    class Name(str):
        __slots__ = ()

    value = {Name("a"): [Path("static"), (Path("b"),)]}

    # When
    result = conf_dump.plain(value)

    # Then
    assert repr(result) == "{'a': ['static', ('b',)]}"


def test_spelling_of_a_value_whose_repr_raises_is_no_literal() -> None:
    # Given
    class Broken:
        def __repr__(self) -> str:
            raise RuntimeError

    # When / Then
    assert conf_dump.spelling(Broken()).startswith("<repr failed")
    assert conf_dump.spelling(os.sep) == repr(os.sep)


def test_tags_stand_in_holds_what_is_added() -> None:
    # Given
    tags = conf_dump.Tags()

    # When
    tags.add("draft")
    tags.add("html")
    tags.remove("html")
    tags.remove("never-set")

    # Then
    assert tags.has("draft")
    assert "draft" in tags
    assert not tags.has("html")
    assert list(tags) == ["draft"]


def test_is_under_matches_whole_path_components() -> None:
    # Given
    directory = Path("/usr/lib")

    # When / Then
    assert conf_dump.is_under(directory / "python3" / "os.py", [directory])
    assert conf_dump.is_under(directory, [directory])
    assert not conf_dump.is_under(Path("/usr/lib64/os.py"), [directory])


def test_the_script_runs_as_the_server_runs_it(tmp_path: Path) -> None:
    # Given
    conf = write_conf(tmp_path, "print('noise')\nproject = 'P' * 2\n")

    # When
    result = subprocess.run(
        [sys.executable, "-c", SCRIPT, str(conf), json.dumps(["project"])],
        capture_output=True,
        text=True,
        cwd=tmp_path,
        check=True,
    )

    # Then
    assert json.loads(result.stdout)["values"] == {"project": "'PP'"}
    assert result.stderr == "noise\n"

"""Executes the doctest plans produced by `rinx extract_doctests`.

Why this is Python rather than Rust
-----------------------------------

A ``.. doctest::`` block asserts that *CPython* produces the stated output, so
CPython is the system under test, not an implementation detail. Re-implementing
``doctest.OutputChecker`` -- ``ELLIPSIS``, ``NORMALIZE_WHITESPACE``,
``<BLANKLINE>``, ``IGNORE_EXCEPTION_DETAIL``, exception matching -- in Rust would
not remove the interpreter dependency (the code still has to be executed); it
would only add a second, silently divergent oracle. So the stdlib runs the
tests, exactly as ``sphinx.ext.doctest`` does.

This mirrors how PlantUML diagrams are compiled by ``plantuml.jar``: rinx
does not own that language either.

What this file reproduces from Sphinx
-------------------------------------

* one namespace per group, shared by its setup, tests and cleanup
* a failing setup skips the rest of its group
* ``doctest_global_setup`` prepended to every group's setup, global cleanup
  appended
* ``:pyversion:`` compared against the *running* interpreter
* ``:skipif:`` evaluated in a context built by running global setup then global
  cleanup -- deliberately not the group's own setup
* ``DONT_ACCEPT_BLANKLINE`` forced on for testcode/testoutput pairs
* Sphinx's default flags, which are *not* the stdlib's
* a block that parses to zero examples is skipped with a warning

This file runs inside users' `py_test`s, on whichever interpreter their
toolchain provides, so it stays stdlib-only and Python 3.9-compatible.
"""

from __future__ import annotations

import argparse
import doctest
import json
import os
import re
import sys
import traceback
from dataclasses import dataclass, field
from pathlib import Path
from typing import TYPE_CHECKING, Any, Literal, TypedDict, Union
from xml.etree import ElementTree as ET

if TYPE_CHECKING:
    from collections.abc import Iterable
    from types import CodeType

    from typing_extensions import Self

# Sphinx's `doctest_default_flags`, which differ from the stdlib's empty
# default. Reproduced so a block behaves the same here as under `make doctest`.
DEFAULT_OPTIONFLAGS = (
    doctest.DONT_ACCEPT_TRUE_FOR_1 | doctest.ELLIPSIS | doctest.IGNORE_EXCEPTION_DETAIL
)

# `doctest` exposes no public way to ask "does this expected output describe a
# traceback?", and Sphinx uses the private regex. Fall back to an equivalent
# pattern rather than crashing if a future release removes it.
_FALLBACK_EXCEPTION_RE = re.compile(
    r"""
    # ---- start of traceback
    (?P<hdr> Traceback\ \( (?: most\ recent\ call\ last
                           | innermost\ last ) \) : )
    \s* $                                   # toss trailing whitespace
    (?P<stack> .*?)                         # don't blink: absorb stuff
    ^ (?P<msg> \w+ .*)                      # exception message
    """,
    re.VERBOSE | re.MULTILINE | re.DOTALL,
)
EXCEPTION_RE = getattr(doctest.DocTestParser, "_EXCEPTION_RE", _FALLBACK_EXCEPTION_RE)


class DoctestPlanError(Exception):
    """A plan file could not be read or does not have the expected shape."""


# ---------------------------------------------------------------------------
# The plan format, as `crates/worker/src/doctest_plan.rs` serializes it
# ---------------------------------------------------------------------------


class Conditions(TypedDict, total=False):
    """When a block runs: `RunConditions`."""

    pyversion: str | None
    skipif: str | None


class Flag(TypedDict):
    """One `+FLAG`/`-FLAG` doctest option: `DocTestFlagSpec`."""

    name: str
    enabled: bool


class Snippet(TypedDict):
    """A block of code: `DocTestSnippet`."""

    source: str
    conditions: Conditions


class ExpectedOutput(TypedDict):
    """A ``testoutput`` block: `ExpectedOutput`."""

    text: str
    conditions: Conditions
    flags: list[Flag]


class Interactive(TypedDict):
    """A ``.. doctest::`` block of `>>>` examples."""

    source: str
    conditions: Conditions
    flags: list[Flag]


class CodeWithOutput(TypedDict):
    """A ``testcode`` block and the ``testoutput`` paired with it, if any."""

    code: Snippet
    expected: ExpectedOutput | None


class InteractiveCase(TypedDict):
    """`DocTestCase::Interactive`, as serde tags it."""

    Interactive: Interactive


class CodeWithOutputCase(TypedDict):
    """`DocTestCase::CodeWithOutput`, as serde tags it."""

    CodeWithOutput: CodeWithOutput


Case = Union[InteractiveCase, CodeWithOutputCase]


class Group(TypedDict):
    """One doctest group: `DocTestGroupPlan`."""

    name: str
    setup: list[Snippet]
    cases: list[Case]
    cleanup: list[Snippet]


class Plan(TypedDict):
    """One document's `.doctests.json`: `DocTestPlan`."""

    doc_path: str
    groups: list[Group]


# ---------------------------------------------------------------------------
# Compile mode
# ---------------------------------------------------------------------------
#
# `doctest` hard-codes `compile(..., "single", ...)`, which is right for an
# interactive example -- `>>> 1 + 1` has to echo `2` through `sys.displayhook`
# -- but rejects the multi-statement code a `testcode`, `testsetup` or
# `testcleanup` block normally contains ("multiple statements found while
# compiling a single statement").
#
# Sphinx solves this by replacing the `compile` that `doctest`'s module globals
# resolve, and so do we: it is the only way to keep doctest's comparison and
# failure reporting while executing block code. Assigning the attribute shadows
# the builtin for code inside `doctest` only.

_CURRENT_COMPILE_MODE = "single"


def _compile_in_current_mode(
    source: str,
    filename: str,
    mode: str,  # noqa: ARG001 - deliberately overridden
    flags: int = 0,
    # Positional, because `doctest` calls this with the builtin's signature.
    dont_inherit: bool = False,  # noqa: FBT001, FBT002
) -> CodeType:
    """`compile`, in the mode `compile_mode` selected rather than the one asked for."""
    return compile(source, filename, _CURRENT_COMPILE_MODE, flags, dont_inherit)


doctest.compile = _compile_in_current_mode  # ty: ignore[unresolved-attribute]


class compile_mode:  # noqa: N801 - used as a context manager, reads as one
    """Selects the mode `doctest` compiles examples in for the duration."""

    def __init__(self, mode: str) -> None:
        """Prepare to compile in `mode` ("single" or "exec") once entered."""
        self.mode = mode
        self.previous = _CURRENT_COMPILE_MODE

    def __enter__(self) -> Self:
        """Switch `doctest` to this mode."""
        global _CURRENT_COMPILE_MODE  # noqa: PLW0603 - mirrors doctest's own global state
        self.previous = _CURRENT_COMPILE_MODE
        _CURRENT_COMPILE_MODE = self.mode
        return self

    def __exit__(self, *_exc: object) -> None:
        """Restore the mode that was in effect before."""
        global _CURRENT_COMPILE_MODE  # noqa: PLW0603
        _CURRENT_COMPILE_MODE = self.previous


Status = Literal["passed", "failed", "skipped"]


@dataclass
class CaseResult:
    """The outcome of one runnable unit."""

    group: str
    index: int
    status: Status
    detail: str = ""

    @property
    def name(self) -> str:
        """The test case name the log and the JUnit XML show."""
        return f"{self.group}[{self.index}]"


@dataclass
class RunReport:
    """Everything one invocation produced."""

    results: list[CaseResult] = field(default_factory=list)

    @property
    def failed(self) -> list[CaseResult]:
        """The results that failed."""
        return [r for r in self.results if r.status == "failed"]

    @property
    def skipped(self) -> list[CaseResult]:
        """The results that were skipped."""
        return [r for r in self.results if r.status == "skipped"]

    @property
    def passed(self) -> list[CaseResult]:
        """The results that passed."""
        return [r for r in self.results if r.status == "passed"]


# ---------------------------------------------------------------------------
# Version specifiers
# ---------------------------------------------------------------------------


def parse_version(text: str) -> tuple[int, ...]:
    """Parses dotted release segments into a comparable tuple."""
    return tuple(int(part) for part in text.split("."))


def pad(left: tuple[int, ...], right: tuple[int, ...]) -> tuple[tuple[int, ...], tuple[int, ...]]:
    """Zero-pads the shorter of two release tuples, as PEP 440 requires.

    Without this, ``>=3.5`` would compare ``(3, 11, 2) >= (3, 5)`` on unequal
    lengths and give the right answer only by accident.
    """
    width = max(len(left), len(right))
    return (
        left + (0,) * (width - len(left)),
        right + (0,) * (width - len(right)),
    )


def clause_matches(clause: str, version: tuple[int, ...]) -> bool:
    """Evaluates one specifier clause such as ``>=3.5`` or ``!=3.7.*``."""
    for operator in (">=", "<=", "==", "!=", ">", "<"):
        if clause.startswith(operator):
            rest = clause[len(operator) :].strip()
            break
    else:
        message = f"unsupported version clause {clause!r}"
        raise DoctestPlanError(message)

    wildcard = rest.endswith(".*")
    if wildcard:
        rest = rest[: -len(".*")]
    expected = parse_version(rest)

    if wildcard:
        # A wildcard compares only the segments actually written, so `==3.7.*`
        # accepts every 3.7 patch release.
        prefix = version[: len(expected)]
        if operator == "==":
            return prefix == expected
        if operator == "!=":
            return prefix != expected
        message = f"operator {operator!r} cannot take a wildcard"
        raise DoctestPlanError(message)

    actual, expect = pad(version, expected)
    return {
        ">=": actual >= expect,
        "<=": actual <= expect,
        "==": actual == expect,
        "!=": actual != expect,
        ">": actual > expect,
        "<": actual < expect,
    }[operator]


def version_matches(spec: str, version: tuple[int, ...]) -> bool:
    """Evaluates a comma-separated specifier set; every clause must hold."""
    return all(
        clause_matches(clause.strip(), version) for clause in spec.split(",") if clause.strip()
    )


# ---------------------------------------------------------------------------
# Skip decisions
# ---------------------------------------------------------------------------


def build_skipif_context(global_setup: str, global_cleanup: str) -> dict[str, Any]:
    """Builds the namespace a ``:skipif:`` expression is evaluated in.

    Sphinx runs global setup *and then global cleanup* into a throwaway
    namespace for this, deliberately not the group's own setup — a skip has to
    be decidable before any group state exists.
    """
    context: dict[str, Any] = {}
    if global_setup:
        exec(global_setup, context)  # noqa: S102 - executing docs is the job
    if global_cleanup:
        exec(global_cleanup, context)  # noqa: S102
    return context


def should_skip(
    conditions: Conditions,
    context: dict[str, Any],
    version: tuple[int, ...],
) -> str | None:
    """Returns the reason to skip, or ``None`` to run.

    ``pyversion`` is compared against the interpreter actually running, which is
    why the extracted plan carries the specifier unevaluated.
    """
    spec = conditions.get("pyversion")
    if spec and not version_matches(spec, version):
        rendered = ".".join(str(part) for part in version)
        return f"requires Python {spec}, running {rendered}"

    condition = conditions.get("skipif")
    if condition:
        try:
            if eval(condition, dict(context)):  # noqa: S307 - authored in docs
                return f"skipif: {condition}"
        except Exception as error:  # noqa: BLE001 - report, never crash the run
            return f"skipif {condition!r} raised {error!r}"

    return None


# ---------------------------------------------------------------------------
# Execution
# ---------------------------------------------------------------------------


def optionflags_for(flags: Iterable[Flag]) -> int:
    """Folds a plan's ``+FLAG``/``-FLAG`` list into a doctest option mask."""
    mask = 0
    for flag in flags:
        bit = doctest.OPTIONFLAGS_BY_NAME.get(flag["name"])
        if bit is None:
            continue
        if flag["enabled"]:
            mask |= bit
    return mask


def option_overrides(flags: Iterable[Flag]) -> dict[int, bool]:
    """Builds the per-example ``options`` mapping doctest expects."""
    overrides: dict[int, bool] = {}
    for flag in flags:
        bit = doctest.OPTIONFLAGS_BY_NAME.get(flag["name"])
        if bit is not None:
            overrides[bit] = flag["enabled"]
    return overrides


class GroupRunner:
    """Runs one group's snippets and cases against a shared namespace."""

    def __init__(self, doc_path: str, name: str) -> None:
        """Start a group with a fresh namespace."""
        self.doc_path = doc_path
        self.name = name
        self.namespace: dict[str, Any] = {}
        self.runner = doctest.DocTestRunner(verbose=False, optionflags=DEFAULT_OPTIONFLAGS)

    def _run(self, test: doctest.DocTest, mode: str) -> tuple[bool, str]:
        """Runs one synthesized doctest, returning success and any report.

        `mode` is `"single"` for interactive examples, whose value has to be
        echoed, and `"exec"` for block code, which is normally several
        statements.
        """
        captured: list[str] = []
        before = self.runner.failures
        # `clear_globs=False` keeps the group's namespace alive across cases,
        # which is the whole point of a group.
        test.globs = self.namespace
        with compile_mode(mode):
            self.runner.run(test, out=captured.append, clear_globs=False)
        return self.runner.failures == before, "".join(captured)

    def run_snippets(self, sources: Iterable[str], what: str) -> tuple[bool, str]:
        """Runs setup or cleanup code, which must produce no output."""
        examples = [doctest.Example(source + "\n", "") for source in sources]
        if not examples:
            return True, ""
        test = doctest.DocTest(examples, {}, f"{self.name} ({what} code)", self.doc_path, 0, None)
        return self._run(test, "exec")

    def run_interactive(self, case: Interactive) -> tuple[bool, str]:
        """Runs a ``.. doctest::`` block through the ordinary doctest parser."""
        parser = doctest.DocTestParser()
        try:
            test = parser.get_doctest(case["source"] + "\n", {}, self.name, self.doc_path, 0)
        except Exception:  # noqa: BLE001 - a malformed block is a failure
            return False, traceback.format_exc()

        if not test.examples:
            # Sphinx warns and moves on rather than failing.
            return True, f"no examples found in {self.name}\n"

        overrides = option_overrides(case.get("flags", []))
        for example in test.examples:
            # The directive's options are the baseline; an example's own
            # `# doctest:` comment wins, so apply them underneath.
            merged = dict(overrides)
            merged.update(example.options)
            example.options = merged

        return self._run(test, "single")

    def run_code_with_output(self, case: CodeWithOutput) -> tuple[bool, str]:
        """Runs a ``testcode``/``testoutput`` pair as one synthesized example."""
        expected = case.get("expected")
        want = expected["text"] + "\n" if expected else ""
        flags = expected["flags"] if expected else []

        options = option_overrides(flags)
        # Sphinx forces this on: the pair states its output literally, so a
        # `<BLANKLINE>` marker would be a false positive rather than markup.
        options[doctest.DONT_ACCEPT_BLANKLINE] = True

        match = EXCEPTION_RE.match(want) if want else None
        exc_msg = match.group("msg") if match else None

        example = doctest.Example(
            case["code"]["source"] + "\n",
            want,
            exc_msg=exc_msg,
            options=options,
        )
        test = doctest.DocTest([example], {}, self.name, self.doc_path, 0, None)
        return self._run(test, "exec")


@dataclass(frozen=True)
class Environment:
    """What every group of one run shares."""

    # The namespace a `:skipif:` is evaluated in; see `build_skipif_context`.
    skipif_context: dict[str, Any]
    # The running interpreter's version, which `:pyversion:` is compared with.
    version: tuple[int, ...]
    global_setup: str
    global_cleanup: str

    def skip_reason(self, conditions: Conditions) -> str | None:
        """Why a block with these conditions is skipped, or ``None`` to run it."""
        return should_skip(conditions, self.skipif_context, self.version)

    def runnable_sources(self, snippets: Iterable[Snippet]) -> list[str]:
        """The source of each setup or cleanup snippet whose conditions hold."""
        return [s["source"] for s in snippets if self.skip_reason(s["conditions"]) is None]


def unrecognized_case(case: Case) -> DoctestPlanError:
    """The error for a serialized `DocTestCase` holding neither variant."""
    return DoctestPlanError(f"a case is neither Interactive nor CodeWithOutput: {sorted(case)}")


def case_skip_reason(case: Case, env: Environment) -> str | None:
    """Why a case is skipped, or ``None`` to run it."""
    interactive = case.get("Interactive")
    if interactive is not None:
        return env.skip_reason(interactive["conditions"])
    pair = case.get("CodeWithOutput")
    if pair is None:
        raise unrecognized_case(case)
    reason = env.skip_reason(pair["code"]["conditions"])
    if reason is None and pair["expected"]:
        # An output block can carry its own conditions; if either half is
        # skipped the pair cannot be checked.
        reason = env.skip_reason(pair["expected"]["conditions"])
    return reason


def run_case(runner: GroupRunner, case: Case) -> tuple[bool, str]:
    """Runs one case of a group, returning success and any report."""
    interactive = case.get("Interactive")
    if interactive is not None:
        return runner.run_interactive(interactive)
    pair = case.get("CodeWithOutput")
    if pair is None:
        raise unrecognized_case(case)
    return runner.run_code_with_output(pair)


def run_group(doc_path: str, group: Group, env: Environment) -> list[CaseResult]:
    """Runs one group end to end, returning a result per case."""
    name = group["name"]
    runner = GroupRunner(doc_path, name)
    results: list[CaseResult] = []

    setup = env.runnable_sources(group["setup"])
    if env.global_setup:
        # Prepended, so a group's own setup can build on it.
        setup.insert(0, env.global_setup)

    ok, report = runner.run_snippets(setup, "setup")
    if not ok:
        # A failed setup poisons every case, so report once and stop rather
        # than emitting a cascade of misleading failures.
        return [CaseResult(name, 0, "failed", f"setup failed\n{report}")]

    for index, case in enumerate(group["cases"]):
        reason = case_skip_reason(case, env)
        if reason is not None:
            results.append(CaseResult(name, index, "skipped", reason))
            continue
        ok, report = run_case(runner, case)
        results.append(CaseResult(name, index, "passed" if ok else "failed", report))

    cleanup = env.runnable_sources(group["cleanup"])
    if env.global_cleanup:
        # Appended, mirroring Sphinx.
        cleanup.append(env.global_cleanup)

    ok, report = runner.run_snippets(cleanup, "cleanup")
    if not ok:
        results.append(CaseResult(name, len(group["cases"]), "failed", f"cleanup failed\n{report}"))

    return results


def run_plan(plan: Plan, global_setup: str, global_cleanup: str) -> list[CaseResult]:
    """Runs every group in one document's plan."""
    env = Environment(
        skipif_context=build_skipif_context(global_setup, global_cleanup),
        version=tuple(sys.version_info[:3]),
        global_setup=global_setup,
        global_cleanup=global_cleanup,
    )
    results: list[CaseResult] = []
    for group in plan["groups"]:
        results.extend(run_group(plan["doc_path"], group, env))
    return results


# ---------------------------------------------------------------------------
# Reporting
# ---------------------------------------------------------------------------


def build_testsuite(doc_path: str, report: RunReport) -> ET.Element:
    """Builds the ``<testsuite>`` element describing one document's run."""
    suite = ET.Element(
        "testsuite",
        name=doc_path,
        tests=str(len(report.results)),
        failures=str(len(report.failed)),
        skipped=str(len(report.skipped)),
    )
    for result in report.results:
        case = ET.SubElement(suite, "testcase", name=result.name, classname=doc_path)
        if result.status == "failed":
            failure = ET.SubElement(case, "failure", message="doctest failed")
            failure.text = result.detail
        elif result.status == "skipped":
            ET.SubElement(case, "skipped", message=result.detail)
    return suite


def write_junit_xml(path: str, suites: list[ET.Element]) -> None:
    """Writes Bazel's ``$XML_OUTPUT_FILE`` so failures surface in the test UI.

    One invocation runs every document of a library, so the root carries one
    suite per document and the test UI keeps naming the document a failure came
    from.
    """
    root = ET.Element("testsuites")
    root.extend(suites)
    ET.ElementTree(root).write(path, encoding="unicode", xml_declaration=True)


def print_report(doc_path: str, report: RunReport) -> None:
    """Prints the human-readable test log."""
    for result in report.skipped:
        print(f"SKIP  {doc_path}::{result.name} — {result.detail}")
    for result in report.failed:
        print(f"FAIL  {doc_path}::{result.name}")
        print(result.detail.rstrip())
        print()
    print(
        f"{doc_path}: {len(report.passed)} passed, "
        f"{len(report.failed)} failed, {len(report.skipped)} skipped"
    )


def read_optional_file(path: str | None) -> str:
    """Reads a global setup/cleanup file, or returns empty text."""
    if not path:
        return ""
    return Path(path).read_text(encoding="utf-8")


def main(argv: list[str] | None = None) -> int:
    """Run every plan named on the command line; exit non-zero if any case failed."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("plans", nargs="+", help="`.doctests.json` files to run")
    parser.add_argument("--global-setup", help="Python file run before every group")
    parser.add_argument("--global-cleanup", help="Python file run after every group")
    args = parser.parse_args(argv)

    global_setup = read_optional_file(args.global_setup)
    global_cleanup = read_optional_file(args.global_cleanup)

    failed = 0
    suites: list[ET.Element] = []
    for plan_path in args.plans:
        plan: Plan = json.loads(Path(plan_path).read_text(encoding="utf-8"))

        report = RunReport(run_plan(plan, global_setup, global_cleanup))
        print_report(plan["doc_path"], report)
        failed += len(report.failed)
        suites.append(build_testsuite(plan["doc_path"], report))

    xml_path = os.environ.get("XML_OUTPUT_FILE")
    if xml_path:
        write_junit_xml(xml_path, suites)

    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())

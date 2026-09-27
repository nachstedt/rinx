"""Tests for the doctest plan runner.

The plans here are written by hand rather than produced by `extract_doctests`,
so a change to the Rust extractor's wire format shows up as a failure here
rather than as a silently skipped test.
"""

from __future__ import annotations

import doctest
import json
import os
from pathlib import Path
from typing import cast
from xml.etree import ElementTree as ET

import pytest

import doctest_runner


def conditions(
    pyversion: str | None = None, skipif: str | None = None
) -> doctest_runner.Conditions:
    return {"pyversion": pyversion, "skipif": skipif}


def snippet(
    source: str, pyversion: str | None = None, skipif: str | None = None
) -> doctest_runner.Snippet:
    return {"source": source, "conditions": conditions(pyversion, skipif)}


def interactive(
    source: str,
    flags: list[doctest_runner.Flag] | None = None,
    pyversion: str | None = None,
    skipif: str | None = None,
) -> doctest_runner.InteractiveCase:
    return {
        "Interactive": {
            "source": source,
            "conditions": conditions(pyversion, skipif),
            "flags": flags or [],
        }
    }


def code_with_output(
    code: str,
    output: str | None = None,
    *,
    flags: list[doctest_runner.Flag] | None = None,
    code_conditions: doctest_runner.Conditions | None = None,
    output_conditions: doctest_runner.Conditions | None = None,
) -> doctest_runner.CodeWithOutputCase:
    expected: doctest_runner.ExpectedOutput | None = None
    if output is not None:
        expected = {
            "text": output,
            "conditions": output_conditions or conditions(),
            "flags": flags or [],
        }
    return {
        "CodeWithOutput": {
            "code": {"source": code, "conditions": code_conditions or conditions()},
            "expected": expected,
        }
    }


def group(
    name: str = "default",
    setup: list[doctest_runner.Snippet] | None = None,
    cases: list[doctest_runner.Case] | None = None,
    cleanup: list[doctest_runner.Snippet] | None = None,
) -> doctest_runner.Group:
    return {
        "name": name,
        "setup": setup or [],
        "cases": cases or [],
        "cleanup": cleanup or [],
    }


def plan(*groups: doctest_runner.Group, doc_path: str = "test.rst") -> doctest_runner.Plan:
    return {"doc_path": doc_path, "groups": list(groups)}


def run(
    plan_dict: doctest_runner.Plan, global_setup: str = "", global_cleanup: str = ""
) -> doctest_runner.RunReport:
    return doctest_runner.RunReport(
        doctest_runner.run_plan(plan_dict, global_setup, global_cleanup)
    )


def found(element: ET.Element, path: str) -> ET.Element:
    """The first match of `path` under `element`, which must exist."""
    match = element.find(path)
    assert match is not None, path
    return match


def environment(version: tuple[int, ...] = (3, 11, 0)) -> doctest_runner.Environment:
    return doctest_runner.Environment(
        skipif_context={}, version=version, global_setup="", global_cleanup=""
    )


class TestVersionSpecifier:
    def test_matches_a_greater_or_equal_clause(self) -> None:
        # Given / When / Then
        assert doctest_runner.version_matches(">=3.5", (3, 11, 2))
        assert not doctest_runner.version_matches(">=3.12", (3, 11, 2))

    def test_pads_release_segments_before_comparing(self) -> None:
        # Given — PEP 440 pads the shorter side with zeros.
        # When / Then — (3, 5) must not compare as shorter-therefore-smaller.
        assert doctest_runner.version_matches(">=3.5", (3, 5, 0))
        assert doctest_runner.version_matches("<=3.5", (3, 5))

    def test_requires_every_clause_of_a_set(self) -> None:
        # Given
        spec = ">=3.5,<3.9"

        # When / Then
        assert doctest_runner.version_matches(spec, (3, 8, 1))
        assert not doctest_runner.version_matches(spec, (3, 9, 0))

    def test_matches_a_wildcard_on_the_written_segments_only(self) -> None:
        # Given
        # When / Then
        assert doctest_runner.version_matches("==3.7.*", (3, 7, 9))
        assert not doctest_runner.version_matches("==3.7.*", (3, 8, 0))
        assert doctest_runner.version_matches("!=3.7.*", (3, 8, 0))

    def test_rejects_an_unsupported_operator(self) -> None:
        # Given — the Rust side rejects these too; this is the second gate.
        with pytest.raises(doctest_runner.DoctestPlanError):
            doctest_runner.version_matches("~=3.5", (3, 11, 0))


class TestSkipDecision:
    def test_runs_a_block_with_no_conditions(self) -> None:
        # Given / When
        reason = doctest_runner.should_skip(conditions(), {}, (3, 11, 0))

        # Then
        assert reason is None

    def test_skips_a_block_the_interpreter_is_too_old_for(self) -> None:
        # Given
        # When
        reason = doctest_runner.should_skip(conditions(pyversion=">=3.99"), {}, (3, 11, 0))

        # Then
        assert reason is not None
        assert "requires Python" in reason

    def test_skips_when_the_skipif_expression_is_true(self) -> None:
        # Given
        # When
        reason = doctest_runner.should_skip(conditions(skipif="True"), {}, (3, 11, 0))

        # Then
        assert reason is not None
        assert "skipif" in reason

    def test_evaluates_skipif_against_the_given_context(self) -> None:
        # Given — the context comes from global setup, not the group's setup.
        context = {"PLATFORM": "win32"}

        # When
        reason = doctest_runner.should_skip(
            conditions(skipif="PLATFORM == 'win32'"), context, (3, 11, 0)
        )

        # Then
        assert reason is not None

    def test_reports_rather_than_crashes_on_a_broken_skipif(self) -> None:
        # Given — a typo must not take the whole run down.
        # When
        reason = doctest_runner.should_skip(conditions(skipif="undefined_name"), {}, (3, 11, 0))

        # Then
        assert reason is not None
        assert "raised" in reason

    def test_builds_the_skipif_context_from_global_setup_and_cleanup(self) -> None:
        # Given
        # When
        context = doctest_runner.build_skipif_context("A = 1", "B = 2")

        # Then
        assert context["A"] == 1
        assert context["B"] == 2


class TestInteractiveCase:
    def test_passes_a_correct_example(self) -> None:
        # Given
        report = run(plan(group(cases=[interactive(">>> 1 + 1\n2")])))

        # Then
        assert len(report.passed) == 1

    def test_fails_a_wrong_example(self) -> None:
        # Given
        report = run(plan(group(cases=[interactive(">>> 1 + 1\n3")])))

        # Then
        assert len(report.failed) == 1
        assert "Expected" in report.failed[0].detail

    def test_applies_sphinx_default_flags(self) -> None:
        # Given — ELLIPSIS is on by default in Sphinx but not in the stdlib,
        # so this passes only if the default mask is reproduced.
        report = run(plan(group(cases=[interactive(">>> [1, 2, 3]\n[1, ...]")])))

        # Then
        assert len(report.passed) == 1

    def test_honours_a_directive_flag(self) -> None:
        # Given — NORMALIZE_WHITESPACE is not on by default.
        case = interactive(
            ">>> print('a  b')\na b",
            flags=[{"name": "NORMALIZE_WHITESPACE", "enabled": True}],
        )

        # When
        report = run(plan(group(cases=[case])))

        # Then
        assert len(report.passed) == 1

    def test_a_disabled_flag_overrides_the_default(self) -> None:
        # Given — turning ELLIPSIS off makes the `...` literal again.
        case = interactive(
            ">>> [1, 2, 3]\n[1, ...]",
            flags=[{"name": "ELLIPSIS", "enabled": False}],
        )

        # When
        report = run(plan(group(cases=[case])))

        # Then
        assert len(report.failed) == 1

    def test_matches_an_expected_exception(self) -> None:
        # Given
        source = (
            ">>> raise ValueError('boom')\nTraceback (most recent call last):\nValueError: boom"
        )

        # When
        report = run(plan(group(cases=[interactive(source)])))

        # Then
        assert len(report.passed) == 1

    def test_skips_a_block_with_no_examples(self) -> None:
        # Given — Sphinx warns and moves on rather than failing.
        report = run(plan(group(cases=[interactive("not an example")])))

        # Then
        assert len(report.failed) == 0


class TestCodeWithOutput:
    def test_passes_when_the_output_matches(self) -> None:
        # Given
        report = run(plan(group(cases=[code_with_output("print(42)", "42")])))

        # Then
        assert len(report.passed) == 1

    def test_fails_when_the_output_differs(self) -> None:
        # Given
        report = run(plan(group(cases=[code_with_output("print(1)", "2")])))

        # Then
        assert len(report.failed) == 1

    def test_runs_multi_statement_code(self) -> None:
        # Given — the case `doctest`'s hard-coded "single" compile mode
        # rejects, and the reason `compile_mode` exists.
        source = "def f():\n    return 41 + 1\n\nprint(f())"

        # When
        report = run(plan(group(cases=[code_with_output(source, "42")])))

        # Then
        assert len(report.passed) == 1, report.results[0].detail

    def test_accepts_code_that_prints_nothing(self) -> None:
        # Given — a testcode with no testoutput answering it.
        report = run(plan(group(cases=[code_with_output("x = 1")])))

        # Then
        assert len(report.passed) == 1

    def test_fails_when_unexpected_output_is_printed(self) -> None:
        # Given
        report = run(plan(group(cases=[code_with_output("print('noise')")])))

        # Then
        assert len(report.failed) == 1

    def test_treats_a_blankline_marker_literally(self) -> None:
        # Given — Sphinx forces DONT_ACCEPT_BLANKLINE for pairs, because the
        # expected text is stated literally rather than as doctest markup.
        report = run(plan(group(cases=[code_with_output("print('<BLANKLINE>')", "<BLANKLINE>")])))

        # Then
        assert len(report.passed) == 1, report.results[0].detail

    def test_matches_an_expected_traceback(self) -> None:
        # Given
        output = "Traceback (most recent call last):\nValueError: boom"

        # When
        report = run(plan(group(cases=[code_with_output("raise ValueError('boom')", output)])))

        # Then
        assert len(report.passed) == 1, report.results[0].detail


class TestNamespace:
    def test_shares_one_namespace_across_a_groups_cases(self) -> None:
        # Given — the defining property of a group.
        cases: list[doctest_runner.Case] = [
            code_with_output("shared = 7"),
            code_with_output("print(shared)", "7"),
        ]

        # When
        report = run(plan(group(cases=cases)))

        # Then
        assert len(report.passed) == 2

    def test_setup_state_is_visible_to_cases(self) -> None:
        # Given
        report = run(
            plan(
                group(
                    setup=[snippet("import math")],
                    cases=[code_with_output("print(int(math.pi))", "3")],
                )
            )
        )

        # Then
        assert len(report.passed) == 1, report.results[0].detail

    def test_groups_do_not_share_state(self) -> None:
        # Given — separate namespaces, so `leaked` must not resolve.
        report = run(
            plan(
                group(name="one", cases=[code_with_output("leaked = 1")]),
                group(name="two", cases=[code_with_output("print(leaked)", "1")]),
            )
        )

        # Then
        assert len(report.failed) == 1

    def test_a_failing_setup_skips_the_group(self) -> None:
        # Given — running the cases would produce a cascade of misleading
        # failures, so Sphinx abandons the group.
        report = run(
            plan(
                group(
                    setup=[snippet("raise RuntimeError('setup broke')")],
                    cases=[code_with_output("print(1)", "1")],
                )
            )
        )

        # Then — exactly one failure, naming the setup.
        assert len(report.failed) == 1
        assert "setup failed" in report.failed[0].detail

    def test_cleanup_runs_after_the_cases(self) -> None:
        # Given — cleanup sees state the cases created.
        report = run(
            plan(
                group(
                    cases=[code_with_output("created = 1")],
                    cleanup=[snippet("del created")],
                )
            )
        )

        # Then — a failing `del` would be reported, so passing proves ordering.
        assert len(report.failed) == 0


class TestGlobalSetup:
    def test_global_setup_runs_before_every_group(self) -> None:
        # Given
        report = run(
            plan(
                group(name="one", cases=[code_with_output("print(TOKEN)", "1")]),
                group(name="two", cases=[code_with_output("print(TOKEN)", "1")]),
            ),
            global_setup="TOKEN = 1",
        )

        # Then
        assert len(report.passed) == 2

    def test_group_setup_can_build_on_global_setup(self) -> None:
        # Given — global setup is prepended, so ordering matters.
        report = run(
            plan(
                group(
                    setup=[snippet("DERIVED = BASE + 1")],
                    cases=[code_with_output("print(DERIVED)", "2")],
                )
            ),
            global_setup="BASE = 1",
        )

        # Then
        assert len(report.passed) == 1, report.results[0].detail


class TestConditionalExecution:
    def test_skips_a_case_whose_pyversion_does_not_match(self) -> None:
        # Given
        case = code_with_output(
            "raise AssertionError()", code_conditions=conditions(pyversion=">=3.99")
        )

        # When
        report = run(plan(group(cases=[case])))

        # Then — skipped, so the body never ran.
        assert len(report.skipped) == 1
        assert len(report.failed) == 0

    def test_skips_a_case_whose_skipif_is_true(self) -> None:
        # Given
        case = code_with_output("raise AssertionError()", code_conditions=conditions(skipif="True"))

        # When
        report = run(plan(group(cases=[case])))

        # Then
        assert len(report.skipped) == 1

    def test_skips_a_pair_when_only_the_output_block_is_conditional(self) -> None:
        # Given — the pair cannot be checked if either half is skipped.
        case = code_with_output(
            "raise AssertionError()",
            "unused",
            output_conditions=conditions(skipif="True"),
        )

        # When
        report = run(plan(group(cases=[case])))

        # Then
        assert len(report.skipped) == 1
        assert len(report.failed) == 0

    def test_skips_a_conditional_setup_snippet(self) -> None:
        # Given
        report = run(
            plan(
                group(
                    setup=[snippet("raise AssertionError()", skipif="True")],
                    cases=[code_with_output("print(1)", "1")],
                )
            )
        )

        # Then — the setup was skipped, so the case still ran.
        assert len(report.passed) == 1


class TestEnvironment:
    def test_runnable_sources_drops_the_snippets_whose_conditions_fail(self) -> None:
        # Given one unconditional snippet and one skipped by `:skipif:`
        snippets = [snippet("a = 1"), snippet("b = 2", skipif="True")]

        # When
        sources = environment().runnable_sources(snippets)

        # Then
        assert sources == ["a = 1"]

    def test_skip_reason_compares_pyversion_with_the_environment_version(self) -> None:
        # Given / When / Then
        assert environment((3, 9, 0)).skip_reason(conditions(pyversion=">=3.10")) is not None
        assert environment((3, 10, 0)).skip_reason(conditions(pyversion=">=3.10")) is None


class TestCaseDispatch:
    def test_an_interactive_case_skips_on_its_own_conditions(self) -> None:
        # Given
        case = interactive(">>> 1", skipif="True")

        # When / Then
        assert doctest_runner.case_skip_reason(case, environment()) == "skipif: True"

    def test_a_pair_runs_when_neither_half_is_conditional(self) -> None:
        # Given
        case = code_with_output("print(1)", "1")

        # When / Then
        assert doctest_runner.case_skip_reason(case, environment()) is None

    def test_a_case_holding_neither_variant_is_a_plan_error(self) -> None:
        # Given a case shape the extractor never writes
        case = cast("doctest_runner.Case", {"Unexpected": {}})

        # When / Then
        with pytest.raises(doctest_runner.DoctestPlanError, match="Unexpected"):
            doctest_runner.case_skip_reason(case, environment())
        with pytest.raises(doctest_runner.DoctestPlanError, match="Unexpected"):
            doctest_runner.run_case(doctest_runner.GroupRunner("d.rst", "g"), case)


class TestReporting:
    def test_exit_code_is_zero_when_everything_passes(self, tmp_path: Path) -> None:
        # Given
        path = tmp_path / "p.json"
        path.write_text(json.dumps(plan(group(cases=[code_with_output("print(1)", "1")]))))

        # When
        code = doctest_runner.main([str(path)])

        # Then
        assert code == 0

    def test_exit_code_is_nonzero_when_a_case_fails(self, tmp_path: Path) -> None:
        # Given
        path = tmp_path / "p.json"
        path.write_text(json.dumps(plan(group(cases=[code_with_output("print(1)", "2")]))))

        # When
        code = doctest_runner.main([str(path)])

        # Then
        assert code == 1

    def test_writes_junit_xml_when_bazel_asks_for_it(self, tmp_path: Path) -> None:
        # Given
        plan_path = tmp_path / "p.json"
        xml_path = tmp_path / "out.xml"
        plan_path.write_text(json.dumps(plan(group(cases=[code_with_output("print(1)", "2")]))))
        os.environ["XML_OUTPUT_FILE"] = str(xml_path)
        try:
            # When
            doctest_runner.main([str(plan_path)])
        finally:
            del os.environ["XML_OUTPUT_FILE"]

        # Then
        content = xml_path.read_text()

        assert "<failure" in content
        assert 'name="default[0]"' in content

    def test_writes_one_junit_suite_per_document(self, tmp_path: Path) -> None:
        # Given — one target now runs every document of a library.
        xml_path = tmp_path / "out.xml"
        paths = []
        for doc in ("first.rst", "second.rst"):
            plan_path = tmp_path / (doc + ".json")
            plan_path.write_text(
                json.dumps(
                    plan(
                        group(cases=[code_with_output("print(1)", "1")]),
                        doc_path=doc,
                    )
                )
            )
            paths.append(str(plan_path))
        os.environ["XML_OUTPUT_FILE"] = str(xml_path)
        try:
            # When
            code = doctest_runner.main(paths)
        finally:
            del os.environ["XML_OUTPUT_FILE"]

        # Then — both documents are named, under one root.
        root = ET.parse(xml_path).getroot()

        assert code == 0
        assert [suite.get("name") for suite in root] == ["first.rst", "second.rst"]

    def test_empty_plan_passes(self) -> None:
        # Given — every document gets a plan, most have no tests.
        report = run(plan())

        # Then
        assert report.results == []


class TestJUnitXml:
    def test_a_suite_counts_each_outcome(self) -> None:
        # Given — one of each status.
        report = doctest_runner.RunReport(
            [
                doctest_runner.CaseResult("default", 0, "passed"),
                doctest_runner.CaseResult("default", 1, "failed", "boom"),
                doctest_runner.CaseResult("default", 2, "skipped", "too old"),
            ]
        )

        # When
        suite = doctest_runner.build_testsuite("doc.rst", report)

        # Then
        assert suite.get("name") == "doc.rst"
        assert suite.get("tests") == "3"
        assert suite.get("failures") == "1"
        assert suite.get("skipped") == "1"
        assert found(suite, "testcase/failure").text == "boom"
        assert found(suite, "testcase[@name='default[2]']/skipped").get("message") == "too old"

    def test_every_case_is_classified_by_its_document(self) -> None:
        # Given — one process runs several documents, so the class name is
        # what tells their cases apart in the test UI.
        report = doctest_runner.RunReport([doctest_runner.CaseResult("default", 0, "passed")])

        # When
        suite = doctest_runner.build_testsuite("doc.rst", report)

        # Then
        assert found(suite, "testcase").get("classname") == "doc.rst"

    def test_all_suites_share_one_root(self, tmp_path: Path) -> None:
        # Given
        suites = [ET.Element("testsuite", name="a"), ET.Element("testsuite", name="b")]

        path = tmp_path / "out.xml"

        # When
        doctest_runner.write_junit_xml(str(path), suites)

        # Then
        root = ET.parse(path).getroot()

        assert root.tag == "testsuites"
        assert [suite.get("name") for suite in root] == ["a", "b"]


class TestCompileMode:
    def test_restores_the_previous_mode_on_exit(self) -> None:
        # Given — nesting must not leave doctest compiling in the wrong mode.
        with doctest_runner.compile_mode("exec"):
            with doctest_runner.compile_mode("single"):
                pass
            inner = doctest_runner._CURRENT_COMPILE_MODE

        # Then
        assert inner == "exec"

    def test_doctest_compile_is_patched(self) -> None:
        # Given — the mechanism the multi-statement support depends on.
        assert vars(doctest)["compile"] is doctest_runner._compile_in_current_mode

"""Tests for the doctest plan runner.

The plans here are written by hand rather than produced by `extract_doctests`,
so a change to the Rust extractor's wire format shows up as a failure here
rather than as a silently skipped test.
"""

import doctest
import json
import os
import tempfile
import unittest
from pathlib import Path
from xml.etree import ElementTree as ET

import doctest_runner


def conditions(pyversion=None, skipif=None):
    return {"pyversion": pyversion, "skipif": skipif}


def snippet(source, **kwargs):
    return {"source": source, "conditions": conditions(**kwargs)}


def interactive(source, flags=None, **kwargs):
    return {
        "Interactive": {
            "source": source,
            "conditions": conditions(**kwargs),
            "flags": flags or [],
        }
    }


def code_with_output(code, output=None, flags=None, output_conditions=None, **kwargs):
    expected = None
    if output is not None:
        expected = {
            "text": output,
            "conditions": output_conditions or conditions(),
            "flags": flags or [],
        }
    return {
        "CodeWithOutput": {
            "code": snippet(code, **kwargs),
            "expected": expected,
        }
    }


def group(name="default", setup=None, cases=None, cleanup=None):
    return {
        "name": name,
        "setup": setup or [],
        "cases": cases or [],
        "cleanup": cleanup or [],
    }


def plan(*groups, doc_path="test.rst"):
    return {"doc_path": doc_path, "groups": list(groups)}


def run(plan_dict, global_setup="", global_cleanup=""):
    return doctest_runner.RunReport(
        doctest_runner.run_plan(plan_dict, global_setup, global_cleanup)
    )


class VersionSpecifierTest(unittest.TestCase):
    def test_matches_a_greater_or_equal_clause(self):
        # Given / When / Then
        self.assertTrue(doctest_runner.version_matches(">=3.5", (3, 11, 2)))
        self.assertFalse(doctest_runner.version_matches(">=3.12", (3, 11, 2)))

    def test_pads_release_segments_before_comparing(self):
        # Given — PEP 440 pads the shorter side with zeros.
        # When / Then — (3, 5) must not compare as shorter-therefore-smaller.
        self.assertTrue(doctest_runner.version_matches(">=3.5", (3, 5, 0)))
        self.assertTrue(doctest_runner.version_matches("<=3.5", (3, 5)))

    def test_requires_every_clause_of_a_set(self):
        # Given
        spec = ">=3.5,<3.9"

        # When / Then
        self.assertTrue(doctest_runner.version_matches(spec, (3, 8, 1)))
        self.assertFalse(doctest_runner.version_matches(spec, (3, 9, 0)))

    def test_matches_a_wildcard_on_the_written_segments_only(self):
        # Given
        # When / Then
        self.assertTrue(doctest_runner.version_matches("==3.7.*", (3, 7, 9)))
        self.assertFalse(doctest_runner.version_matches("==3.7.*", (3, 8, 0)))
        self.assertTrue(doctest_runner.version_matches("!=3.7.*", (3, 8, 0)))

    def test_rejects_an_unsupported_operator(self):
        # Given — the Rust side rejects these too; this is the second gate.
        with self.assertRaises(doctest_runner.DoctestPlanError):
            doctest_runner.version_matches("~=3.5", (3, 11, 0))


class SkipDecisionTest(unittest.TestCase):
    def test_runs_a_block_with_no_conditions(self):
        # Given / When
        reason = doctest_runner.should_skip(conditions(), {}, (3, 11, 0))

        # Then
        self.assertIsNone(reason)

    def test_skips_a_block_the_interpreter_is_too_old_for(self):
        # Given
        # When
        reason = doctest_runner.should_skip(
            conditions(pyversion=">=3.99"), {}, (3, 11, 0)
        )

        # Then
        self.assertIn("requires Python", reason)

    def test_skips_when_the_skipif_expression_is_true(self):
        # Given
        # When
        reason = doctest_runner.should_skip(conditions(skipif="True"), {}, (3, 11, 0))

        # Then
        self.assertIn("skipif", reason)

    def test_evaluates_skipif_against_the_given_context(self):
        # Given — the context comes from global setup, not the group's setup.
        context = {"PLATFORM": "win32"}

        # When
        reason = doctest_runner.should_skip(
            conditions(skipif="PLATFORM == 'win32'"), context, (3, 11, 0)
        )

        # Then
        self.assertIsNotNone(reason)

    def test_reports_rather_than_crashes_on_a_broken_skipif(self):
        # Given — a typo must not take the whole run down.
        # When
        reason = doctest_runner.should_skip(
            conditions(skipif="undefined_name"), {}, (3, 11, 0)
        )

        # Then
        self.assertIn("raised", reason)

    def test_builds_the_skipif_context_from_global_setup_and_cleanup(self):
        # Given
        # When
        context = doctest_runner.build_skipif_context("A = 1", "B = 2")

        # Then
        self.assertEqual(context["A"], 1)
        self.assertEqual(context["B"], 2)


class InteractiveCaseTest(unittest.TestCase):
    def test_passes_a_correct_example(self):
        # Given
        report = run(plan(group(cases=[interactive(">>> 1 + 1\n2")])))

        # Then
        self.assertEqual(len(report.passed), 1)

    def test_fails_a_wrong_example(self):
        # Given
        report = run(plan(group(cases=[interactive(">>> 1 + 1\n3")])))

        # Then
        self.assertEqual(len(report.failed), 1)
        self.assertIn("Expected", report.failed[0].detail)

    def test_applies_sphinx_default_flags(self):
        # Given — ELLIPSIS is on by default in Sphinx but not in the stdlib,
        # so this passes only if the default mask is reproduced.
        report = run(plan(group(cases=[interactive(">>> [1, 2, 3]\n[1, ...]")])))

        # Then
        self.assertEqual(len(report.passed), 1)

    def test_honours_a_directive_flag(self):
        # Given — NORMALIZE_WHITESPACE is not on by default.
        case = interactive(
            ">>> print('a  b')\na b",
            flags=[{"name": "NORMALIZE_WHITESPACE", "enabled": True}],
        )

        # When
        report = run(plan(group(cases=[case])))

        # Then
        self.assertEqual(len(report.passed), 1)

    def test_a_disabled_flag_overrides_the_default(self):
        # Given — turning ELLIPSIS off makes the `...` literal again.
        case = interactive(
            ">>> [1, 2, 3]\n[1, ...]",
            flags=[{"name": "ELLIPSIS", "enabled": False}],
        )

        # When
        report = run(plan(group(cases=[case])))

        # Then
        self.assertEqual(len(report.failed), 1)

    def test_matches_an_expected_exception(self):
        # Given
        source = ">>> raise ValueError('boom')\nTraceback (most recent call last):\nValueError: boom"

        # When
        report = run(plan(group(cases=[interactive(source)])))

        # Then
        self.assertEqual(len(report.passed), 1)

    def test_skips_a_block_with_no_examples(self):
        # Given — Sphinx warns and moves on rather than failing.
        report = run(plan(group(cases=[interactive("not an example")])))

        # Then
        self.assertEqual(len(report.failed), 0)


class CodeWithOutputTest(unittest.TestCase):
    def test_passes_when_the_output_matches(self):
        # Given
        report = run(plan(group(cases=[code_with_output("print(42)", "42")])))

        # Then
        self.assertEqual(len(report.passed), 1)

    def test_fails_when_the_output_differs(self):
        # Given
        report = run(plan(group(cases=[code_with_output("print(1)", "2")])))

        # Then
        self.assertEqual(len(report.failed), 1)

    def test_runs_multi_statement_code(self):
        # Given — the case `doctest`'s hard-coded "single" compile mode
        # rejects, and the reason `compile_mode` exists.
        source = "def f():\n    return 41 + 1\n\nprint(f())"

        # When
        report = run(plan(group(cases=[code_with_output(source, "42")])))

        # Then
        self.assertEqual(len(report.passed), 1, report.results[0].detail)

    def test_accepts_code_that_prints_nothing(self):
        # Given — a testcode with no testoutput answering it.
        report = run(plan(group(cases=[code_with_output("x = 1")])))

        # Then
        self.assertEqual(len(report.passed), 1)

    def test_fails_when_unexpected_output_is_printed(self):
        # Given
        report = run(plan(group(cases=[code_with_output("print('noise')")])))

        # Then
        self.assertEqual(len(report.failed), 1)

    def test_treats_a_blankline_marker_literally(self):
        # Given — Sphinx forces DONT_ACCEPT_BLANKLINE for pairs, because the
        # expected text is stated literally rather than as doctest markup.
        report = run(
            plan(group(cases=[code_with_output("print('<BLANKLINE>')", "<BLANKLINE>")]))
        )

        # Then
        self.assertEqual(len(report.passed), 1, report.results[0].detail)

    def test_matches_an_expected_traceback(self):
        # Given
        output = "Traceback (most recent call last):\nValueError: boom"

        # When
        report = run(
            plan(group(cases=[code_with_output("raise ValueError('boom')", output)]))
        )

        # Then
        self.assertEqual(len(report.passed), 1, report.results[0].detail)


class NamespaceTest(unittest.TestCase):
    def test_shares_one_namespace_across_a_groups_cases(self):
        # Given — the defining property of a group.
        cases = [
            code_with_output("shared = 7"),
            code_with_output("print(shared)", "7"),
        ]

        # When
        report = run(plan(group(cases=cases)))

        # Then
        self.assertEqual(len(report.passed), 2)

    def test_setup_state_is_visible_to_cases(self):
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
        self.assertEqual(len(report.passed), 1, report.results[0].detail)

    def test_groups_do_not_share_state(self):
        # Given — separate namespaces, so `leaked` must not resolve.
        report = run(
            plan(
                group(name="one", cases=[code_with_output("leaked = 1")]),
                group(name="two", cases=[code_with_output("print(leaked)", "1")]),
            )
        )

        # Then
        self.assertEqual(len(report.failed), 1)

    def test_a_failing_setup_skips_the_group(self):
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
        self.assertEqual(len(report.failed), 1)
        self.assertIn("setup failed", report.failed[0].detail)

    def test_cleanup_runs_after_the_cases(self):
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
        self.assertEqual(len(report.failed), 0)


class GlobalSetupTest(unittest.TestCase):
    def test_global_setup_runs_before_every_group(self):
        # Given
        report = run(
            plan(
                group(name="one", cases=[code_with_output("print(TOKEN)", "1")]),
                group(name="two", cases=[code_with_output("print(TOKEN)", "1")]),
            ),
            global_setup="TOKEN = 1",
        )

        # Then
        self.assertEqual(len(report.passed), 2)

    def test_group_setup_can_build_on_global_setup(self):
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
        self.assertEqual(len(report.passed), 1, report.results[0].detail)


class ConditionalExecutionTest(unittest.TestCase):
    def test_skips_a_case_whose_pyversion_does_not_match(self):
        # Given
        case = code_with_output("raise AssertionError()", pyversion=">=3.99")

        # When
        report = run(plan(group(cases=[case])))

        # Then — skipped, so the body never ran.
        self.assertEqual(len(report.skipped), 1)
        self.assertEqual(len(report.failed), 0)

    def test_skips_a_case_whose_skipif_is_true(self):
        # Given
        case = code_with_output("raise AssertionError()", skipif="True")

        # When
        report = run(plan(group(cases=[case])))

        # Then
        self.assertEqual(len(report.skipped), 1)

    def test_skips_a_pair_when_only_the_output_block_is_conditional(self):
        # Given — the pair cannot be checked if either half is skipped.
        case = code_with_output(
            "raise AssertionError()",
            "unused",
            output_conditions=conditions(skipif="True"),
        )

        # When
        report = run(plan(group(cases=[case])))

        # Then
        self.assertEqual(len(report.skipped), 1)
        self.assertEqual(len(report.failed), 0)

    def test_skips_a_conditional_setup_snippet(self):
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
        self.assertEqual(len(report.passed), 1)


class ReportingTest(unittest.TestCase):
    def test_exit_code_is_zero_when_everything_passes(self):
        # Given
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "p.json"
            path.write_text(
                json.dumps(
                    plan(group(cases=[code_with_output("print(1)", "1")]))
                )
            )

            # When
            code = doctest_runner.main([str(path)])

        # Then
        self.assertEqual(code, 0)

    def test_exit_code_is_nonzero_when_a_case_fails(self):
        # Given
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "p.json"
            path.write_text(
                json.dumps(
                    plan(group(cases=[code_with_output("print(1)", "2")]))
                )
            )

            # When
            code = doctest_runner.main([str(path)])

        # Then
        self.assertEqual(code, 1)

    def test_writes_junit_xml_when_bazel_asks_for_it(self):
        # Given
        with tempfile.TemporaryDirectory() as tmp:
            plan_path = Path(tmp) / "p.json"
            xml_path = Path(tmp) / "out.xml"
            plan_path.write_text(
                json.dumps(
                    plan(group(cases=[code_with_output("print(1)", "2")]))
                )
            )
            os.environ["XML_OUTPUT_FILE"] = str(xml_path)
            try:
                # When
                doctest_runner.main([str(plan_path)])
            finally:
                del os.environ["XML_OUTPUT_FILE"]

            # Then
            content = xml_path.read_text()

        self.assertIn("<failure", content)
        self.assertIn('name="default[0]"', content)

    def test_writes_one_junit_suite_per_document(self):
        # Given — one target now runs every document of a library.
        with tempfile.TemporaryDirectory() as tmp:
            xml_path = Path(tmp) / "out.xml"
            paths = []
            for doc in ("first.rst", "second.rst"):
                plan_path = Path(tmp) / (doc + ".json")
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

        self.assertEqual(code, 0)
        self.assertEqual(
            [suite.get("name") for suite in root], ["first.rst", "second.rst"]
        )

    def test_empty_plan_passes(self):
        # Given — every document gets a plan, most have no tests.
        report = run(plan())

        # Then
        self.assertEqual(report.results, [])


class JUnitXmlTest(unittest.TestCase):
    def test_a_suite_counts_each_outcome(self):
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
        self.assertEqual(suite.get("name"), "doc.rst")
        self.assertEqual(suite.get("tests"), "3")
        self.assertEqual(suite.get("failures"), "1")
        self.assertEqual(suite.get("skipped"), "1")
        self.assertEqual(suite.find("testcase/failure").text, "boom")
        self.assertEqual(
            suite.find("testcase[@name='default[2]']/skipped").get("message"),
            "too old",
        )

    def test_every_case_is_classified_by_its_document(self):
        # Given — one process runs several documents, so the class name is
        # what tells their cases apart in the test UI.
        report = doctest_runner.RunReport(
            [doctest_runner.CaseResult("default", 0, "passed")]
        )

        # When
        suite = doctest_runner.build_testsuite("doc.rst", report)

        # Then
        self.assertEqual(suite.find("testcase").get("classname"), "doc.rst")

    def test_all_suites_share_one_root(self):
        # Given
        suites = [ET.Element("testsuite", name="a"), ET.Element("testsuite", name="b")]

        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "out.xml"

            # When
            doctest_runner.write_junit_xml(str(path), suites)

            # Then
            root = ET.parse(path).getroot()

        self.assertEqual(root.tag, "testsuites")
        self.assertEqual([suite.get("name") for suite in root], ["a", "b"])


class CompileModeTest(unittest.TestCase):
    def test_restores_the_previous_mode_on_exit(self):
        # Given — nesting must not leave doctest compiling in the wrong mode.
        with doctest_runner.compile_mode("exec"):
            with doctest_runner.compile_mode("single"):
                pass
            inner = doctest_runner._CURRENT_COMPILE_MODE

        # Then
        self.assertEqual(inner, "exec")

    def test_doctest_compile_is_patched(self):
        # Given — the mechanism the multi-statement support depends on.
        self.assertIs(doctest.compile, doctest_runner._compile_in_current_mode)


if __name__ == "__main__":
    unittest.main()

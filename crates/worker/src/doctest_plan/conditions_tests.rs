//! Run-conditions/comparison-flags carry-through, problem-reporting, and the
//! presentation-firewall (byte-stable serialization) tests for
//! [`super::build_doctest_plan`] — split out from `doctest_plan.rs`'s own
//! `mod tests` purely to keep file size down.

use super::*;
use rusty_sphinx_parser::parse;

/// Parses `rst` and builds its plan, expecting success.
fn plan_of(rst: &str) -> DocTestPlan {
    build_doctest_plan(&parse("test.rst", rst)).expect("plan should build")
}

/// Parses `rst` and builds its plan, expecting failure.
fn problems_of(rst: &str) -> String {
    build_doctest_plan(&parse("test.rst", rst)).expect_err("plan should fail")
}

/// Unwraps a code/output pair, failing the test on any other case.
fn code_pair(case: &DocTestCase) -> (&DocTestSnippet, Option<&ExpectedOutput>) {
    match case {
        DocTestCase::CodeWithOutput { code, expected } => (code, expected.as_ref()),
        DocTestCase::Interactive { .. } => {
            panic!("expected a code/output pair, got an interactive case")
        }
    }
}

/// Unwraps an interactive case's flags, failing the test on any other case.
fn interactive_flags(case: &DocTestCase) -> &[DocTestFlagSpec] {
    match case {
        DocTestCase::Interactive { flags, .. } => flags,
        DocTestCase::CodeWithOutput { .. } => {
            panic!("expected an interactive case, got a code/output pair")
        }
    }
}

#[test]
fn test_carries_run_conditions_through_unevaluated() {
    // Given
    let rst = concat!(
        ".. testcode::\n",
        "   :pyversion: >= 3.5\n",
        "   :skipif: sys.platform == 'win32'\n",
        "\n   print(1)\n",
    );

    // When
    let plan = plan_of(rst);

    // Then — rendered canonically, decided by the runner.
    let (code, _) = code_pair(&plan.groups[0].cases[0]);
    assert_eq!(code.conditions.pyversion.as_deref(), Some(">=3.5"));
    assert_eq!(
        code.conditions.skipif.as_deref(),
        Some("sys.platform == 'win32'")
    );
}

#[test]
fn test_carries_comparison_flags_in_doctest_spelling() {
    // Given
    let rst = concat!(
        ".. doctest::\n",
        "   :options: +ELLIPSIS, -NORMALIZE_WHITESPACE\n",
        "\n   >>> f()\n",
    );

    // When
    let plan = plan_of(rst);

    // Then
    assert_eq!(
        interactive_flags(&plan.groups[0].cases[0]),
        &[
            DocTestFlagSpec {
                name: "ELLIPSIS".to_string(),
                enabled: true
            },
            DocTestFlagSpec {
                name: "NORMALIZE_WHITESPACE".to_string(),
                enabled: false
            },
        ]
    );
}

#[test]
fn test_reports_a_testoutput_with_no_preceding_testcode() {
    // Given — Sphinx drops this silently; the author never learns their
    // expected output is not checked.
    let rst = ".. testoutput::\n\n   42\n";

    // When
    let problems = problems_of(rst);

    // Then
    assert!(problems.contains("no preceding testcode"));
}

#[test]
fn test_reports_a_second_testoutput_for_the_same_testcode() {
    // Given
    let rst = concat!(
        ".. testcode::\n\n   print(1)\n\n",
        ".. testoutput::\n\n   1\n\n",
        ".. testoutput::\n\n   2\n",
    );

    // When
    let problems = problems_of(rst);

    // Then
    assert!(problems.contains("already answers this testcode"));
}

#[test]
fn test_reports_a_testoutput_following_a_doctest_block() {
    // Given — a doctest states its own output, so this can never run.
    let rst = concat!(
        ".. doctest::\n\n   >>> 1\n   1\n\n",
        ".. testoutput::\n\n   1\n",
    );

    // When
    let problems = problems_of(rst);

    // Then
    assert!(problems.contains("states its own output"));
}

#[test]
fn test_reports_every_problem_at_once() {
    // Given — two independent mistakes in two groups.
    let rst = concat!(
        ".. testoutput:: a\n\n   1\n\n",
        ".. testoutput:: b\n\n   2\n",
    );

    // When
    let problems = problems_of(rst);

    // Then — both reported, so one build shows the author everything.
    assert_eq!(problems.lines().count(), 2);
}

#[test]
fn test_plan_ignores_presentation_only_differences() {
    // Given — two documents whose *tests* are identical but whose prose,
    // `:hide:` and trim options differ.
    let plain = concat!(
        "Title\n=====\n\nSome prose.\n\n",
        ".. testcode::\n\n   print(1)\n\n",
        ".. testoutput::\n\n   1\n",
    );
    let decorated = concat!(
        "Title\n=====\n\nCompletely different prose, and more of it.\n\n",
        ".. testcode::\n   :hide:\n\n   print(1)\n\n",
        ".. testoutput::\n   :no-trim-doctest-flags:\n\n   1\n",
    );

    // When — serialized exactly as the extract step writes them.
    let left = serde_json::to_string(&plan_of(plain)).expect("serialize");
    let right = serde_json::to_string(&plan_of(decorated)).expect("serialize");

    // Then — byte-identical, so Bazel's action cache stops the cascade
    // here and the Python test does not re-run. This is the property the
    // whole design rests on; asserting structural equality would let a
    // presentation-only field creep into the wire format unnoticed.
    assert_eq!(left, right);
}

#[test]
fn test_plan_changes_when_the_test_code_changes() {
    // Given — the other half of the firewall: a real change must not be
    // cached away.
    let before = ".. testcode::\n\n   print(1)\n";
    let after = ".. testcode::\n\n   print(2)\n";

    // When
    let left = serde_json::to_string(&plan_of(before)).expect("serialize");
    let right = serde_json::to_string(&plan_of(after)).expect("serialize");

    // Then
    assert_ne!(left, right);
}

#[test]
fn test_plan_serialization_roundtrip() {
    // Given
    let plan = plan_of(".. testcode:: g\n\n   print(1)\n\n.. testoutput:: g\n\n   1\n");

    // When
    let json = serde_json::to_string(&plan).expect("serialize");
    let restored: DocTestPlan = serde_json::from_str(&json).expect("deserialize");

    // Then
    assert_eq!(plan, restored);
}

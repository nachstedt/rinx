//! Helpers shared by [`super::grouping_tests`] and
//! [`super::conditions_tests`]: building a plan from RST source, and
//! unwrapping a [`DocTestCase`] to the variant a test expects.

use super::*;
use rinx_parser::parse;

/// Parses `rst` and builds its plan, expecting success.
pub(super) fn plan_of(rst: &str) -> DocTestPlan {
    build_doctest_plan(&parse("test.rst", rst)).expect("plan should build")
}

/// Parses `rst` and builds its plan, expecting failure.
pub(super) fn problems_of(rst: &str) -> String {
    build_doctest_plan(&parse("test.rst", rst)).expect_err("plan should fail")
}

/// Unwraps a code/output pair, failing the test on any other case.
pub(super) fn code_pair(case: &DocTestCase) -> (&DocTestSnippet, Option<&ExpectedOutput>) {
    match case {
        DocTestCase::CodeWithOutput { code, expected } => (code, expected.as_ref()),
        DocTestCase::Interactive { .. } => {
            panic!("expected a code/output pair, got an interactive case")
        }
    }
}

/// Unwraps an interactive case, failing the test on any other case.
pub(super) fn interactive_case(case: &DocTestCase) -> (&str, &RunConditions, &[DocTestFlagSpec]) {
    match case {
        DocTestCase::Interactive {
            source,
            conditions,
            flags,
        } => (source, conditions, flags),
        DocTestCase::CodeWithOutput { .. } => {
            panic!("expected an interactive case, got a code/output pair")
        }
    }
}

/// Unwraps an interactive case's flags, failing the test on any other case.
pub(super) fn interactive_flags(case: &DocTestCase) -> &[DocTestFlagSpec] {
    match case {
        DocTestCase::Interactive { flags, .. } => flags,
        DocTestCase::CodeWithOutput { .. } => {
            panic!("expected an interactive case, got a code/output pair")
        }
    }
}

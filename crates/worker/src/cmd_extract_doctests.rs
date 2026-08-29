//! The `extract_doctests` subcommand: projects a document's doctest blocks
//! into the runnable plan the Python runner consumes.

use anyhow::{Context, Result, anyhow};
use rusty_sphinx_ast as ast;
use rusty_sphinx_worker::doctest_plan;
use std::fs;

use crate::cli_args::flag_value;

/// Deliberately cheap and deliberately *lossy*: the plan drops everything
/// presentational, so a prose edit re-runs this step but leaves its output
/// bytes unchanged — which is what stops Bazel from re-running the tests. See
/// [`rusty_sphinx_worker::doctest_plan`] for the full reasoning.
pub(super) fn process_extract_doctests(ast_json: &str) -> Result<String> {
    let doc: ast::Document = serde_json::from_str(ast_json).context("Failed to deserialize AST")?;
    let plan = doctest_plan::build_doctest_plan(&doc)
        .map_err(|problems| anyhow!("Doctest extraction failed:\n{problems}"))?;
    serde_json::to_string(&plan).context("Serialization error")
}

pub(super) fn cmd_extract_doctests(args: &[String]) -> Result<()> {
    let input = flag_value(args, "--input")?;
    let output = flag_value(args, "--output")?;

    let ast_json =
        fs::read_to_string(&input).with_context(|| format!("Error reading '{input}'"))?;
    let plan_json = process_extract_doctests(&ast_json)?;
    fs::write(&output, plan_json).with_context(|| format!("Error writing '{output}'"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_sphinx_parser as parser;

    #[test]
    fn test_process_extract_doctests_emits_a_plan() {
        // Given
        let doc = parser::parse(
            "test.rst",
            ".. testcode::\n\n   print(1)\n\n.. testoutput::\n\n   1\n",
        );
        let ast_json = serde_json::to_string(&doc).unwrap();

        // When
        let plan_json = process_extract_doctests(&ast_json).expect("should extract");

        // Then
        let plan: doctest_plan::DocTestPlan = serde_json::from_str(&plan_json).unwrap();
        assert_eq!(plan.doc_path, "test.rst");
        assert_eq!(plan.groups.len(), 1);
    }

    #[test]
    fn test_process_extract_doctests_emits_an_empty_plan_without_doctests() {
        // Given — every document gets a plan, so the Bazel action can be
        // declared unconditionally like the diagram extraction is.
        let doc = parser::parse("test.rst", "Title\n=====\n\nProse.");
        let ast_json = serde_json::to_string(&doc).unwrap();

        // When
        let plan_json = process_extract_doctests(&ast_json).expect("should extract");

        // Then
        let plan: doctest_plan::DocTestPlan = serde_json::from_str(&plan_json).unwrap();
        assert!(plan.groups.is_empty());
    }

    #[test]
    fn test_process_extract_doctests_fails_on_an_orphan_testoutput() {
        // Given
        let doc = parser::parse("test.rst", ".. testoutput::\n\n   42\n");
        let ast_json = serde_json::to_string(&doc).unwrap();

        // When
        let result = process_extract_doctests(&ast_json);

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn test_process_extract_doctests_returns_error_for_invalid_ast_json() {
        // Given
        let ast_json = "{not json";

        // When
        let result = process_extract_doctests(ast_json);

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn test_process_extract_doctests_is_unchanged_by_a_prose_edit() {
        // Given — the same tests, different prose around them. This is the
        // cache firewall as the subcommand actually emits it.
        let before = parser::parse(
            "test.rst",
            "Title\n=====\n\nOriginal prose.\n\n.. testcode::\n\n   print(1)\n",
        );
        let after = parser::parse(
            "test.rst",
            "Title\n=====\n\nRewritten, much longer prose.\n\n.. testcode::\n\n   print(1)\n",
        );

        // When
        let left = process_extract_doctests(&serde_json::to_string(&before).unwrap()).unwrap();
        let right = process_extract_doctests(&serde_json::to_string(&after).unwrap()).unwrap();

        // Then — byte-identical, so Bazel does not re-run the tests.
        assert_eq!(left, right);
    }
}

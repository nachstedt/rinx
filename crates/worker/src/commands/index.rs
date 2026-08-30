//! The `index` subcommand: merges every document's local analysis into one
//! global `rusty_sphinx_index::ProjectIndex`.

use anyhow::{Context, Result};
use rusty_sphinx_analyzer as analyzer;
use rusty_sphinx_ast as ast;
use std::fs;

use super::cli_args::{flag_value, flag_values};

pub(super) fn process_index(ast_jsons: &[String]) -> Result<String> {
    let docs: Vec<ast::Document> = ast_jsons
        .iter()
        .map(|json| serde_json::from_str(json).context("Failed to deserialize AST"))
        .collect::<Result<_>>()?;

    let index = analyzer::build_project_index(&docs);
    serde_json::to_string(&index).context("Serialization error")
}

pub(crate) fn cmd_index(args: &[String]) -> Result<()> {
    let output = flag_value(args, "--output")?;
    let inputs = flag_values(args, "--inputs")?;

    let files: Vec<String> = inputs
        .iter()
        .map(|p| fs::read_to_string(p).with_context(|| format!("Error reading '{p}'")))
        .collect::<Result<_>>()?;

    let json = process_index(&files)?;
    fs::write(&output, json).with_context(|| format!("Error writing '{output}'"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_index_returns_serialized_project_index() {
        // Given
        let docs = vec![
            r#"{"path":"test.rst","nodes":[{"Heading":{"level":1,"text":[{"Text":"Title"}]}}]}"#
                .to_string(),
        ];

        // When
        let index = process_index(&docs).unwrap();

        // Then
        assert_eq!(
            index,
            r#"{"targets":{},"document_titles":{"test.rst":"Title"},"nav_tree":[{"title":"Title","path":"test.rst","children":[]}],"glossary_terms":{},"domain_objects":{},"genindex_entries":[]}"#
        );
    }
}

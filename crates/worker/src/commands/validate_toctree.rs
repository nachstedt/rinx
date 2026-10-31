//! The `validate_toctree` subcommand: checks toctree entries only reference
//! declared docs.
//!
//! The check itself is [`rinx_toctree::undeclared_toctree_entries`], which
//! the language server asks too, so the editor underlines exactly what fails
//! here. Not subject to `.. noqa:`: a missing `deps` entry is a fault of the
//! build file, which no comment in a document should be able to hide.

use anyhow::{Context, Result, anyhow};
use rinx_ast as ast;
use rinx_toctree::{DeclaredDocuments, undeclared_toctree_entries};
use std::fs;

use super::cli_args::{flag_value, flag_values_opt};
use super::diagnostics::{WarningOrigin, format_error_diagnostic};

/// Checks the document serialized as `ast_json` against `declared`.
///
/// # Errors
///
/// Fails when the JSON is no document, or naming every undeclared entry, one
/// `error:` line each.
fn process_validate_toctree(ast_json: &str, declared: &DeclaredDocuments) -> Result<()> {
    let doc: ast::Document = serde_json::from_str(ast_json).context("Failed to deserialize AST")?;
    let undeclared = undeclared_toctree_entries(&doc, declared);
    if undeclared.is_empty() {
        return Ok(());
    }
    let origin = WarningOrigin::new(&doc.path, &doc.source_files);
    let lines: Vec<String> = undeclared
        .iter()
        .map(|diagnostic| format_error_diagnostic(&origin, diagnostic))
        .collect();
    Err(anyhow!("Toctree validation failed:\n{}", lines.join("\n")))
}

pub(crate) fn cmd_validate_toctree(args: &[String]) -> Result<()> {
    let input = flag_value(args, "--input")?;
    let output = flag_value(args, "--output")?;
    let allowed = flag_values_opt(args, "--allowed");
    let declared = DeclaredDocuments::from_names(allowed.iter().map(String::as_str));

    let ast_json =
        fs::read_to_string(&input).with_context(|| format!("Error reading '{input}'"))?;
    process_validate_toctree(&ast_json, &declared)?;
    fs::write(&output, &ast_json).with_context(|| format!("Error writing '{output}'"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The JSON of `docs/index.rst` listing `team_a/index` on line 4.
    fn index_listing_team_a() -> String {
        let doc = ast::Document::new(
            "docs/index.rst".to_string(),
            vec![ast::Node::directive(ast::Directive::Toctree(
                ast::Toctree {
                    entries: vec![ast::TocEntry::Document {
                        title: None,
                        docname: "team_a/index".to_string(),
                        span: Some(ast::Span::new(
                            ast::Position::new(4, 4),
                            ast::Position::new(4, 16),
                        )),
                    }],
                    options: ast::ToctreeOptions::default(),
                },
            ))],
        );
        serde_json::to_string(&doc).unwrap()
    }

    #[test]
    fn test_process_validate_toctree_accepts_a_declared_entry() {
        // Given
        let declared = DeclaredDocuments::from_names(["docs/team_a/index"]);

        // When
        let result = process_validate_toctree(&index_listing_team_a(), &declared);

        // Then
        assert!(result.is_ok(), "{result:?}");
    }

    #[test]
    fn test_process_validate_toctree_fails_with_an_error_line_per_entry() {
        // Given / When
        let result =
            process_validate_toctree(&index_listing_team_a(), &DeclaredDocuments::default());

        // Then
        let message = result.expect_err("an undeclared entry fails").to_string();
        assert!(
            message.contains(
                "error: docs/index.rst:4:4: toctree.undeclared-dependency: toctree entry \
                 'team_a/index'"
            ),
            "{message}"
        );
    }

    #[test]
    fn test_process_validate_toctree_ignores_a_noqa() {
        // Given — a suppression covering the entry's line.
        let mut doc: ast::Document = serde_json::from_str(&index_listing_team_a()).unwrap();
        doc.suppressions.push(ast::Suppression {
            start_line: 1,
            end_line: 100,
            codes: ast::SuppressionCodes::All,
            file: None,
        });
        let json = serde_json::to_string(&doc).unwrap();

        // When
        let result = process_validate_toctree(&json, &DeclaredDocuments::default());

        // Then
        assert!(result.is_err());
    }
}

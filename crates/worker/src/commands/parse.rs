//! The `parse` subcommand: RST text to a serialized `ast::Document`.

use anyhow::{Context, Result, anyhow};
use rusty_sphinx_ast as ast;
use rusty_sphinx_parser as parser;
use std::fs;

use super::cli_args::{flag_value, flag_value_opt};
use super::diagnostics::{WarningOrigin, report_diagnostic};
use super::entity_schema::load_entity_schema;
use super::parse_files::DocumentRelativeFiles;
use super::parse_inputs::ParseInputs;
use super::suppression::retain_reportable;

/// `parse_files` is injected rather than built here so this stays the pure,
/// I/O-free half of the subcommand: a test can hand in a loader that reads
/// nothing, while `cmd_parse` hands in the real filesystem one.
pub(super) fn process_parse(
    path: &str,
    rst_content: &str,
    inputs: &ParseInputs<'_>,
) -> Result<String> {
    let mut doc = parser::parse_with_ctx(path, rst_content, &inputs.ctx());
    // Stamped here rather than inside the parser: it identifies the *build's*
    // schema, and only the worker knows which file that came from. The index
    // phase compares it against its own.
    doc.entity_schema_hash = inputs.schema_hash();
    // The document's own `.. noqa:` comments decide what is worth showing.
    let origin = WarningOrigin::new(path, &doc.source_files);
    for diagnostic in retain_reportable(&doc.diagnostics, &doc.suppressions) {
        report_diagnostic(&origin, diagnostic);
    }
    serde_json::to_string(&doc).context("Serialization error")
}

/// Parses the optional `--default-domain` flag, defaulting to `py` — this is
/// how `rusty_sphinx_library`'s Bazel attribute reaches the `parse`/`preview`
/// subcommands (see `rules/library.bzl`, whose `default_domain` attribute
/// restricts to the same two values via `values = ["py", "c"]`).
///
/// Deliberately narrower than `ast::Domain::FromStr`, which also accepts
/// `"std"` (needed so `ObjectType`'s `"std:cmdoption:..."` keys round-trip):
/// `default_domain` is the domain a *bare* directive/role resolves to, and
/// several bare-role code paths (e.g. `handle_func_match`'s
/// `.expect("every domain defines a 'func' role")`) assume it is always `py`
/// or `c` — `std`-domain constructs (`.. option::`, `:option:`, ...) are
/// recognized unconditionally instead, never via `default_domain` (see
/// `resolve_domain_object_type`/`try_parse_scope_directive` in
/// `rusty_sphinx_parser`), so accepting `"std"` here would only invite a
/// runtime panic with no corresponding feature.
pub(super) fn parse_default_domain_flag(args: &[String]) -> Result<ast::Domain> {
    match flag_value_opt(args, "--default-domain") {
        Some(s) => match s.parse::<ast::Domain>() {
            Ok(domain @ (ast::Domain::Py | ast::Domain::C)) => Ok(domain),
            _ => Err(anyhow!(
                "Invalid --default-domain '{s}', expected 'py' or 'c'"
            )),
        },
        None => Ok(ast::Domain::Py),
    }
}

pub(crate) fn cmd_parse(args: &[String]) -> Result<()> {
    let input = flag_value(args, "--input")?;
    let output = flag_value(args, "--output")?;
    let default_domain = parse_default_domain_flag(args)?;

    let rst = fs::read_to_string(&input).with_context(|| format!("Error reading '{input}'"))?;
    // Paths a directive names resolve against the document's own directory,
    // which under Bazel is the sandbox location of the declared source — see
    // `parse_files`.
    let parse_files = DocumentRelativeFiles::for_document(&input);
    let schema = load_entity_schema(args)?;
    let json = process_parse(
        &input,
        &rst,
        &ParseInputs {
            default_domain,
            files: &parse_files,
            schema: &schema,
        },
    )?;

    // A file that could not be read means a whole table or section is missing
    // from the page, so the build fails rather than shipping the gap — the
    // same stance `validate_images` takes on a missing diagram. The parser
    // itself stays resilient (it degrades the directive and carries on), which
    // is what the live preview needs; only this subcommand is strict.
    let failures = parse_files.failures();
    if !failures.is_empty() {
        for failure in &failures {
            eprintln!("error: {input}: {failure}");
        }
        return Err(anyhow!(
            "{input}: {} file(s) named by a directive could not be read",
            failures.len()
        ));
    }

    fs::write(&output, json).with_context(|| format!("Error writing '{output}'"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A loader rooted at a directory holding no CSV files, for the tests
    /// whose input has no `:file:` option.
    fn no_parse_files() -> DocumentRelativeFiles {
        DocumentRelativeFiles::for_document("index.rst")
    }

    #[test]
    fn test_process_parse_returns_serialized_ast() {
        // Given
        let rst = "Title\n=====";

        // When
        let json = process_parse(
            "team_a/index.rst",
            rst,
            &ParseInputs {
                default_domain: ast::Domain::Py,
                files: &no_parse_files(),
                schema: &rusty_sphinx_entity::EntitySchema::empty(),
            },
        )
        .unwrap();

        // Then
        assert!(json.contains("Title"));
        assert!(json.contains(r#""path":"team_a/index.rst""#));
    }

    #[test]
    fn test_process_parse_resolves_bare_directive_via_default_domain() {
        // Given
        let rst = ".. function:: greet(name)\n\n   Greets the given name.";

        // When
        let json = process_parse(
            "api.rst",
            rst,
            &ParseInputs {
                default_domain: ast::Domain::C,
                files: &no_parse_files(),
                schema: &rusty_sphinx_entity::EntitySchema::empty(),
            },
        )
        .unwrap();

        // Then
        assert!(json.contains(r#""CFunction""#));
    }

    /// Writes `rst` into a fresh directory and returns the `cmd_parse` flags
    /// for it, so the `:file:`-resolution tests exercise the real I/O path.
    fn parse_args_for(dir_name: &str, rst: &str) -> Vec<String> {
        let dir = std::env::temp_dir().join(dir_name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let input = dir.join("doc.rst");
        std::fs::write(&input, rst).expect("write rst");
        vec![
            "--input".to_string(),
            input.to_str().expect("utf-8 temp path").to_string(),
            "--output".to_string(),
            dir.join("doc.ast")
                .to_str()
                .expect("utf-8 temp path")
                .to_string(),
        ]
    }

    #[test]
    fn test_cmd_parse_reads_a_csv_table_file_beside_the_document() {
        // Given
        let args = parse_args_for(
            "rusty_sphinx_cmd_parse_csv_ok",
            ".. csv-table::\n   :file: fruits.csv\n",
        );
        let dir = std::path::Path::new(&args[1])
            .parent()
            .expect("input has a directory");
        std::fs::write(dir.join("fruits.csv"), "Apple, Red\n").expect("write csv");

        // When
        let result = cmd_parse(&args);

        // Then
        assert!(result.is_ok(), "{result:?}");
        let ast = std::fs::read_to_string(&args[3]).expect("read ast");
        assert!(ast.contains("Apple"), "{ast}");
    }

    #[test]
    fn test_cmd_parse_fails_when_a_csv_table_file_is_missing() {
        // Given — no `fruits.csv` beside the document, which is what an
        // undeclared `csv_data` file looks like inside a Bazel sandbox.
        let args = parse_args_for(
            "rusty_sphinx_cmd_parse_csv_missing",
            ".. csv-table::\n   :file: fruits.csv\n",
        );

        // When
        let result = cmd_parse(&args);

        // Then
        let error = result.expect_err("a missing :file: must fail the parse");
        assert!(error.to_string().contains("could not be read"), "{error}");
        assert!(
            !std::path::Path::new(&args[3]).exists(),
            "no .ast should be written when the parse fails"
        );
    }

    #[test]
    fn test_parse_default_domain_flag_defaults_to_py_when_absent() {
        // Given
        let args: Vec<String> = vec![];

        // When
        let result = parse_default_domain_flag(&args).unwrap();

        // Then
        assert_eq!(result, ast::Domain::Py);
    }

    #[test]
    fn test_parse_default_domain_flag_parses_explicit_c() {
        // Given
        let args = vec!["--default-domain".to_string(), "c".to_string()];

        // When
        let result = parse_default_domain_flag(&args).unwrap();

        // Then
        assert_eq!(result, ast::Domain::C);
    }

    #[test]
    fn test_parse_default_domain_flag_rejects_invalid_value() {
        // Given
        let args = vec!["--default-domain".to_string(), "rust".to_string()];

        // When
        let result = parse_default_domain_flag(&args);

        // Then
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid --default-domain")
        );
    }

    #[test]
    fn test_parse_default_domain_flag_rejects_std() {
        // Given — `std` is a valid `ast::Domain` (needed for `ObjectType`
        // keys), but not a valid *default* domain: several bare-role code
        // paths assume `default_domain` is always `py` or `c`.
        let args = vec!["--default-domain".to_string(), "std".to_string()];

        // When
        let result = parse_default_domain_flag(&args);

        // Then
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid --default-domain")
        );
    }
}

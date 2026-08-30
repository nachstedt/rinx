//! The `parse` subcommand: RST text to a serialized `ast::Document`.

use anyhow::{Context, Result, anyhow};
use rusty_sphinx_ast as ast;
use rusty_sphinx_parser as parser;
use std::fs;

use super::cli_args::{flag_value, flag_value_opt};

pub(super) fn process_parse(
    path: &str,
    rst_content: &str,
    default_domain: ast::Domain,
) -> Result<String> {
    let doc = parser::parse_with_domain(path, rst_content, default_domain);
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
    let json = process_parse(&input, &rst, default_domain)?;
    fs::write(&output, json).with_context(|| format!("Error writing '{output}'"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_parse_returns_serialized_ast() {
        // Given
        let rst = "Title\n=====";

        // When
        let json = process_parse("team_a/index.rst", rst, ast::Domain::Py).unwrap();

        // Then
        assert!(json.contains("Title"));
        assert!(json.contains(r#""path":"team_a/index.rst""#));
    }

    #[test]
    fn test_process_parse_resolves_bare_directive_via_default_domain() {
        // Given
        let rst = ".. function:: greet(name)\n\n   Greets the given name.";

        // When
        let json = process_parse("api.rst", rst, ast::Domain::C).unwrap();

        // Then
        assert!(json.contains(r#""CFunction""#));
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

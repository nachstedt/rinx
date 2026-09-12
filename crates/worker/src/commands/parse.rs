//! The `parse` subcommand: RST text to a serialized `ast::Document`.

use anyhow::{Context, Result, anyhow};
use rusty_sphinx_ast as ast;
use rusty_sphinx_parser as parser;
use std::fs;

use super::cli_args::{flag_value, flag_value_opt};
use super::diagnostics::{WarningOrigin, format_error_diagnostic, report_diagnostic};
use super::entity_schema::load_entity_schema;
use super::parse_files::DocumentRelativeFiles;
use super::parse_inputs::{ParseInputs, jinja_from_args};
use super::suppression::retain_reportable;

/// Whether the library a document belongs to opted in to diagrams.
///
/// An enum rather than a `bool` so a call site reads as the decision it is.
/// Diagram compilation is opt-in because Bazel cannot know which documents
/// hold a diagram before reading them: without the opt-in, every document of
/// every project would pay for a pipeline most of them never use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DiagramSupport {
    Enabled,
    Disabled,
}

impl DiagramSupport {
    /// Reads the `--diagrams` flag `rusty_sphinx_library` passes when its
    /// `diagrams` attribute is set. Absent means disabled, matching the
    /// attribute's default.
    pub(super) fn from_args(args: &[String]) -> Self {
        if args.iter().any(|arg| arg == "--diagrams") {
            Self::Enabled
        } else {
            Self::Disabled
        }
    }
}

/// One diagnostic per diagram directive in `doc`, however deeply nested.
///
/// Walks the parsed document rather than asking the parser to refuse the
/// directive, so the parser stays unaware of build configuration — the live
/// preview, which has no library at all, parses a diagram like any other
/// construct. An `.. include::`d fragment is covered too, since by now it is
/// part of the document's own tree.
pub(super) fn find_disabled_diagrams(doc: &ast::Document) -> Vec<ast::Diagnostic> {
    let mut found = Vec::new();
    ast::walk_nodes(&doc.nodes, &mut |node| {
        let (directive, code, span) = match node {
            ast::Node::Directive(ast::Directive::Uml(uml)) => (
                uml.source.as_str(),
                ast::DiagnosticCode::UmlDiagramsDisabled,
                uml.span,
            ),
            // A flowchart draws no less of a picture for having generated it,
            // so it needs the same opt-in — under its own code, because a code
            // names the construct.
            ast::Node::Directive(ast::Directive::EntityFlow(flow)) => (
                flow.source.as_str(),
                ast::DiagnosticCode::EntityFlowDiagramsDisabled,
                flow.span,
            ),
            _ => return,
        };
        found.push(ast::Diagnostic::at(
            code,
            format!(
                "{directive}: this library does not compile diagrams; set `diagrams = True` on \
                 its rusty_sphinx_library to enable them"
            ),
            span,
        ));
    });
    found
}

/// `parse_files` is injected rather than built here so this stays the pure,
/// I/O-free half of the subcommand: a test can hand in a loader that reads
/// nothing, while `cmd_parse` hands in the real filesystem one.
///
/// # Errors
///
/// Fails when `diagrams` is [`DiagramSupport::Disabled`] and the document holds
/// a diagram. That is strict where the parser is resilient, for the reason an
/// unreadable file is: the alternative ships a page with a picture missing.
pub(super) fn process_parse(
    path: &str,
    rst_content: &str,
    inputs: &ParseInputs<'_>,
    diagrams: DiagramSupport,
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
    // Not subject to `.. noqa:`: a suppressed opt-in check would still leave
    // the page pointing at an SVG nothing compiled.
    if diagrams == DiagramSupport::Disabled {
        let disabled = find_disabled_diagrams(&doc);
        if !disabled.is_empty() {
            for diagnostic in &disabled {
                eprintln!("{}", format_error_diagnostic(&origin, diagnostic));
            }
            return Err(anyhow!(
                "{path}: {} diagram(s) in a library without `diagrams = True`",
                disabled.len()
            ));
        }
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

/// Writes the Jinja-rendered source to `--dump-rendered`, when asked.
///
/// A debugging aid, passed by no Bazel rule: the rendered text is otherwise
/// never materialized — it lives inside the parse action and dies with it — so
/// this is how somebody reproduces a surprising page by hand. `-` writes to
/// standard output.
///
/// # Errors
///
/// Fails when the named file cannot be written.
fn dump_rendered_source(args: &[String], rst: &str, inputs: &ParseInputs<'_>) -> Result<()> {
    let Some(path) = flag_value_opt(args, "--dump-rendered") else {
        return Ok(());
    };
    let rendered = parser::rendered_source(rst, &inputs.ctx()).unwrap_or_else(|| rst.to_string());
    if path == "-" {
        print!("{rendered}");
        return Ok(());
    }
    fs::write(&path, rendered).with_context(|| format!("Error writing '{path}'"))
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
    let jinja = jinja_from_args(args)?;
    let inputs = ParseInputs {
        default_domain,
        files: &parse_files,
        schema: &schema,
        jinja: jinja.as_deref(),
    };
    dump_rendered_source(args, &rst, &inputs)?;
    let json = process_parse(&input, &rst, &inputs, DiagramSupport::from_args(args))?;

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
                jinja: None,
            },
            DiagramSupport::Enabled,
        )
        .unwrap();

        // Then
        assert!(json.contains("Title"));
        assert!(json.contains(r#""path":"team_a/index.rst""#));
    }

    /// Parses `rst` with diagrams switched as `diagrams` says.
    fn parse_with_diagrams(rst: &str, diagrams: DiagramSupport) -> Result<String> {
        process_parse(
            "index.rst",
            rst,
            &ParseInputs {
                default_domain: ast::Domain::Py,
                files: &no_parse_files(),
                schema: &rusty_sphinx_entity::EntitySchema::empty(),
                jinja: None,
            },
            diagrams,
        )
    }

    #[test]
    fn test_diagram_support_is_enabled_only_by_the_flag() {
        // Given / When / Then — absent means disabled, matching the library
        // attribute's default
        let enabled = vec!["--input".to_string(), "--diagrams".to_string()];
        let disabled = vec!["--input".to_string()];
        assert_eq!(DiagramSupport::from_args(&enabled), DiagramSupport::Enabled);
        assert_eq!(
            DiagramSupport::from_args(&disabled),
            DiagramSupport::Disabled
        );
    }

    #[test]
    fn test_a_diagram_in_a_library_without_diagrams_fails_the_parse() {
        // Given
        let rst = ".. plantuml::\n\n   A -> B\n";

        // When
        let result = parse_with_diagrams(rst, DiagramSupport::Disabled);

        // Then — failing here, rather than shipping a page whose picture no
        // action ever compiled
        let error = result.expect_err("the opt-in is enforced");
        assert!(error.to_string().contains("diagrams = True"), "{error}");
    }

    #[test]
    fn test_a_diagram_in_a_library_with_diagrams_parses() {
        // Given
        let rst = ".. plantuml::\n\n   A -> B\n";

        // When / Then
        assert!(parse_with_diagrams(rst, DiagramSupport::Enabled).is_ok());
    }

    #[test]
    fn test_a_document_without_diagrams_parses_whatever_the_setting() {
        // Given — the whole point: a library that draws nothing pays nothing,
        // including no obligation to opt in
        let rst = "Title\n=====\n\nProse.\n";

        // When / Then
        assert!(parse_with_diagrams(rst, DiagramSupport::Disabled).is_ok());
    }

    #[test]
    fn test_find_disabled_diagrams_reports_each_diagram_at_its_own_line() {
        // Given — one at top level and one nested in an admonition
        let rst = ".. plantuml::\n\n   A -> B\n\n.. note::\n\n   .. uml::\n\n      C -> D\n";
        let doc = parser::parse("index.rst", rst);

        // When
        let found = find_disabled_diagrams(&doc);

        // Then
        let lines: Vec<u32> = found
            .iter()
            .map(|diagnostic| {
                diagnostic
                    .span
                    .expect("a diagram has a position")
                    .start
                    .line
            })
            .collect();
        assert_eq!(lines, [1, 7]);
        assert!(
            found
                .iter()
                .all(|d| d.code == ast::DiagnosticCode::UmlDiagramsDisabled)
        );
    }

    #[test]
    fn test_a_flowchart_needs_the_same_opt_in_under_its_own_code() {
        // Given — a generated picture is compiled by the same action a written
        // one is, so a library that creates no diagram actions has nowhere to
        // put it either
        let rst = ".. entity-flow::\n";
        let doc = parser::parse("index.rst", rst);

        // When
        let found = find_disabled_diagrams(&doc);

        // Then
        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0].code,
            ast::DiagnosticCode::EntityFlowDiagramsDisabled
        );
        assert!(found[0].message.contains("diagrams = True"), "{found:?}");
        assert!(
            parse_with_diagrams(rst, DiagramSupport::Disabled).is_err(),
            "a flowchart in a library without the opt-in must fail the parse"
        );
        assert!(parse_with_diagrams(rst, DiagramSupport::Enabled).is_ok());
    }

    #[test]
    fn test_find_disabled_diagrams_finds_nothing_in_a_document_without_one() {
        // Given
        let doc = parser::parse("index.rst", "Just prose.\n");

        // When / Then
        assert!(find_disabled_diagrams(&doc).is_empty());
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
                jinja: None,
            },
            DiagramSupport::Enabled,
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
    fn test_cmd_parse_renders_the_source_as_a_template() {
        // Given a document opening the way the sphinx-needs demo's do.
        // The `{% include %}` half is exercised where it can be: against a
        // stub loader in `rusty_sphinx_parser::templating`, and against a real
        // source root in `tests/test_jinja.sh` — a template name resolves from
        // the source root, which for an ad-hoc run is the working directory,
        // and a unit test must not depend on that.
        let args = parse_args_for(
            "rusty_sphinx_cmd_parse_jinja_ok",
            "{% set page=\"doc.rst\" %}\nSource of {{ page }}, release {{ release }}.\n",
        );
        let args = [
            args,
            vec![
                "--jinja".to_string(),
                "--jinja-context".to_string(),
                "release=0.1".to_string(),
            ],
        ]
        .concat();

        // When
        let result = cmd_parse(&args);

        // Then
        assert!(result.is_ok(), "{result:?}");
        let ast = std::fs::read_to_string(&args[3]).expect("read ast");
        assert!(ast.contains("Source of doc.rst, release 0.1."), "{ast}");
    }

    #[test]
    fn test_cmd_parse_leaves_a_template_alone_without_the_opt_in() {
        // Given the same document, in a library that did not ask for Jinja
        let args = parse_args_for(
            "rusty_sphinx_cmd_parse_jinja_off",
            "{% set page=\"doc.rst\" %}\n\nBody\n",
        );

        // When
        let result = cmd_parse(&args);

        // Then the markup is the text it literally is, and nothing is read
        assert!(result.is_ok(), "{result:?}");
        let ast = std::fs::read_to_string(&args[3]).expect("read ast");
        assert!(ast.contains("{% set page="), "{ast}");
    }

    #[test]
    fn test_cmd_parse_writes_the_rendered_source_when_asked() {
        // Given
        let args = parse_args_for(
            "rusty_sphinx_cmd_parse_jinja_dump",
            "{% set page=\"doc.rst\" %}\nSource of {{ page }}.\n",
        );
        let dump = std::path::Path::new(&args[1])
            .parent()
            .expect("input has a directory")
            .join("rendered.rst");
        let args = [
            args,
            vec![
                "--jinja".to_string(),
                "--dump-rendered".to_string(),
                dump.to_str().expect("utf-8 temp path").to_string(),
            ],
        ]
        .concat();

        // When
        cmd_parse(&args).expect("parses");

        // Then the file holds exactly what the parser saw
        let rendered = std::fs::read_to_string(&dump).expect("read dump");
        assert_eq!(rendered, "\nSource of doc.rst.\n");
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

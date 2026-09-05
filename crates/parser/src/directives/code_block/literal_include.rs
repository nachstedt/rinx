//! `.. literalinclude::` — a code block whose text is read from a file.
//!
//! Lowers to the same [`Directive::CodeBlock`] the two inline directives
//! produce, because every option it adds is a *source transform*: by the time
//! parsing is done the file has been read, the lines selected, the tabs
//! expanded, the prologue and epilogue spliced on and the dedent applied.
//! What is left is content and presentation options, which is exactly what a
//! code block is. The renderer needs no knowledge of this directive at all.
//!
//! The option vocabulary is Sphinx's, shared with `.. code-block::` for the
//! presentation half ([`super::options`]) and with `.. include::` for the
//! selection half ([`super::selection`]). What is genuinely its own is here:
//! naming a file, `:diff:`, `:prepend:`/`:append:`, and `:lineno-match:`.

use std::num::NonZeroU32;

use rusty_sphinx_ast::{
    CodeBlock, CodeBlockSource, CodeLanguage, Diagnostic, DiagnosticCode, Directive, Span,
};

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::directives::body::body_span;
use crate::directives::options::{OptionLine, report_unknown_options, scan_option_lines};
use crate::indent::unindent_body_lines;

use super::dedent::apply_dedent;
use super::diff::unified_diff;
use super::emphasize::resolve_emphasize_lines;
use super::options::parse_code_block_options;
use super::selection::{Selection, check_encoding, expand_tabs};

/// The directive name, used throughout this module's diagnostics.
const DIRECTIVE: &str = "literalinclude";

/// The options only this directive has, as written.
#[derive(Default)]
struct LiteralIncludeOptions {
    language: Option<String>,
    encoding: Option<String>,
    tab_width: Option<i32>,
    prepend: Option<String>,
    append: Option<String>,
    diff: Option<String>,
    lineno_match: bool,
    /// A `:caption:` written with no value, which means "use the filename".
    /// Distinct from an absent one, which means no caption at all.
    bare_caption: bool,
    pyobject: Option<String>,
    selection: Selection,
}

/// Parses a `.. literalinclude::` into a [`Directive::CodeBlock`], reading the
/// file it names through `ctx`.
///
/// Degrades to [`Directive::Unknown`] whenever the content cannot be
/// established — an unreadable file, a selection that matches nothing — having
/// reported why. The parser stays resilient, and the `parse` subcommand turns
/// the loader's recorded failure into a failed build.
pub(in crate::directives) fn parse_literal_include(
    argument: &str,
    body_lines: &[&str],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Directive {
    let span = body_span(body_lines, ctx).or_else(|| ctx.line_span(0, ""));

    let path = argument.trim();
    if path.is_empty() {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::IncludeMissingPath,
            format!("{DIRECTIVE}: needs the path of a file to show"),
            span,
        ));
        return unknown(argument, body_lines);
    }

    let unindented = unindent_body_lines(body_lines);
    let (option_lines, _) = scan_option_lines(&unindented);
    let (shared, unrecognized) = parse_code_block_options(
        CodeBlockSource::LiteralInclude,
        &option_lines,
        diagnostics,
        ctx,
    );
    let (own, unclaimed) = parse_own_options(&unrecognized, diagnostics, ctx);
    report_unknown_options(
        &unclaimed,
        DIRECTIVE,
        DiagnosticCode::IncludeUnknownOption,
        diagnostics,
        ctx,
    );

    if own.pyobject.is_some() {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::LiteralIncludePyObjectUnsupported,
            format!(
                "{DIRECTIVE}: :pyobject: is unsupported; select the object's lines with \
                 :start-after:/:end-before: or :lines: instead"
            ),
            span,
        ));
        return unknown(argument, body_lines);
    }
    if !check_encoding(own.encoding.as_deref(), DIRECTIVE, diagnostics, span) {
        return unknown(argument, body_lines);
    }

    let Some(read) = read_content(path, &own, diagnostics, ctx, span) else {
        return unknown(argument, body_lines);
    };

    let lineno_start = resolve_lineno_start(&own, &read, shared.lineno_start, diagnostics, span);
    let content = apply_dedent(&read.text.lines().collect::<Vec<&str>>(), shared.dedent);
    let content = splice(&own, content);

    let line_count = if content.is_empty() {
        0
    } else {
        content.lines().count()
    };
    let emphasize_lines = resolve_emphasize_lines(
        shared.emphasize_lines_raw.as_ref(),
        line_count,
        DIRECTIVE,
        diagnostics,
        ctx,
    );

    Directive::CodeBlock(CodeBlock {
        source: CodeBlockSource::LiteralInclude,
        // A `:diff:` is a patch whatever the file's own language is, so it
        // names its own rather than inheriting one that would mis-highlight it.
        language: if own.diff.is_some() {
            CodeLanguage::parse("diff")
        } else {
            own.language
                .as_deref()
                .map_or(CodeLanguage::Inherit, CodeLanguage::parse)
        },
        content,
        caption: shared
            .caption
            .clone()
            .or_else(|| own.bare_caption.then(|| path.to_string())),
        name: shared.name.clone(),
        classes: shared.classes.clone(),
        linenos: shared.linenos || lineno_start.is_some(),
        lineno_start,
        emphasize_lines,
        force: shared.force,
        span,
    })
}

/// Consumes the options only this directive has, returning them with the lines
/// nothing claimed.
fn parse_own_options<'a>(
    option_lines: &[&'a OptionLine],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> (LiteralIncludeOptions, Vec<&'a OptionLine>) {
    let mut options = LiteralIncludeOptions::default();
    let mut unclaimed = Vec::new();

    for line in option_lines {
        match line.name.as_str() {
            "language" => options.language = Some(line.value.clone()),
            "encoding" => options.encoding = Some(line.value.clone()),
            "prepend" => options.prepend = Some(line.value.clone()),
            "append" => options.append = Some(line.value.clone()),
            "diff" => options.diff = Some(line.value.clone()),
            "pyobject" => options.pyobject = Some(line.value.clone()),
            "lineno-match" => options.lineno_match = true,
            // Reaches here only with an empty value: the shared parser claims
            // a `:caption:` that has one.
            "caption" => options.bare_caption = true,
            "lines" => options.selection.lines = Some(line.value.clone()),
            "start-after" => options.selection.start_after = Some(line.value.clone()),
            "end-before" => options.selection.end_before = Some(line.value.clone()),
            "start-at" => options.selection.start_at = Some(line.value.clone()),
            "end-at" => options.selection.end_at = Some(line.value.clone()),
            "tab-width" => {
                options.tab_width = parse_tab_width(line, diagnostics, ctx);
            }
            _ => unclaimed.push(*line),
        }
    }
    (options, unclaimed)
}

/// Reads a `:tab-width:`, which docutils allows to be negative — that is how
/// it spells "leave tabs alone" — so this is an `i32` rather than a count.
fn parse_tab_width(
    line: &OptionLine,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<i32> {
    let Ok(width) = line.value.trim().parse::<i32>() else {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::IncludeInvalidTabWidth,
            format!(
                "{DIRECTIVE}: :tab-width: needs an integer, got '{}'",
                line.value
            ),
            ctx.line_span(line.line_index, &line.raw),
        ));
        return None;
    };
    Some(width)
}

/// What this directive read out of the file it named.
struct ReadContent {
    /// The text to show, before dedenting and splicing.
    text: String,
    /// The 1-based line, in the file, that `text` starts at — what
    /// `:lineno-match:` numbers from. `None` when there is no single such
    /// line: a `:diff:`, or a discontinuous `:lines:` selection.
    first_line: Option<NonZeroU32>,
}

/// The text this directive shows: the selected part of the named file, or the
/// patch between it and a `:diff:` file.
fn read_content(
    path: &str,
    options: &LiteralIncludeOptions,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
    span: Option<Span>,
) -> Option<ReadContent> {
    let file = match ctx.files.load(path, ctx.current_file()) {
        Ok(file) => file,
        Err(message) => {
            diagnostics.push(Diagnostic::at(
                DiagnosticCode::IncludeFileUnreadable,
                format!("{DIRECTIVE}: {message}"),
                span,
            ));
            return None;
        }
    };

    if let Some(other_path) = &options.diff {
        let other = match ctx.files.load(other_path, ctx.current_file()) {
            Ok(other) => other,
            Err(message) => {
                diagnostics.push(Diagnostic::at(
                    DiagnosticCode::LiteralIncludeDiffUnreadable,
                    format!("{DIRECTIVE}: :diff: {message}"),
                    span,
                ));
                return None;
            }
        };
        // Sphinx diffs *from* the `:diff:` file *to* the argument, so the
        // argument reads as "what the file looks like now".
        return Some(ReadContent {
            text: unified_diff(&other.id, &other.text, &file.id, &file.text),
            first_line: None,
        });
    }

    let text = options
        .tab_width
        .map_or_else(|| file.text.clone(), |width| expand_tabs(&file.text, width));
    if options.selection.is_empty() {
        return Some(ReadContent {
            text: text.trim_end_matches('\n').to_string(),
            first_line: NonZeroU32::new(1),
        });
    }
    options
        .selection
        .apply(&text, DIRECTIVE, diagnostics, span)
        .map(|selected| ReadContent {
            text: selected.text,
            first_line: selected.first_line,
        })
}

/// Resolves `:lineno-match:` against the selection, or keeps an explicit
/// `:lineno-start:`.
///
/// The two are alternative ways of saying the same thing, so a document that
/// writes both gets the explicit one — an author who typed a number meant it.
fn resolve_lineno_start(
    options: &LiteralIncludeOptions,
    read: &ReadContent,
    explicit: Option<NonZeroU32>,
    diagnostics: &mut Diagnostics,
    span: Option<Span>,
) -> Option<NonZeroU32> {
    if explicit.is_some() || !options.lineno_match {
        return explicit;
    }
    // A selection that is one contiguous run knows the line it began at; a
    // `:diff:` or a `:lines: 1,5` does not, and there is no honest number to
    // invent for it.
    if read.first_line.is_none() {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::LiteralIncludeLinenoMatchUnusable,
            format!(
                "{DIRECTIVE}: :lineno-match: needs one continuous run of lines to number from, \
                 which a :diff: or a discontinuous :lines: does not give it"
            ),
            span,
        ));
    }
    read.first_line
}

/// Adds `:prepend:` above and `:append:` below the selected text.
fn splice(options: &LiteralIncludeOptions, content: String) -> String {
    let mut parts = Vec::new();
    if let Some(prologue) = &options.prepend {
        parts.push(prologue.clone());
    }
    parts.push(content);
    if let Some(epilogue) = &options.append {
        parts.push(epilogue.clone());
    }
    parts.join("\n")
}

/// The degraded node for a directive that could not produce content.
fn unknown(argument: &str, body_lines: &[&str]) -> Directive {
    Directive::Unknown {
        name: DIRECTIVE.to_string(),
        argument: argument.to_string(),
        body: body_lines
            .iter()
            .map(|line| (*line).to_string())
            .collect::<Vec<String>>()
            .join("\n"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{LoadedFile, ParseFileLoader};
    use rusty_sphinx_ast::{Domain, LanguageName};
    use std::collections::HashMap;

    /// An in-memory stand-in for the worker's filesystem loader.
    struct FakeFiles(HashMap<String, String>);

    impl FakeFiles {
        fn with(files: &[(&str, &str)]) -> Self {
            Self(
                files
                    .iter()
                    .map(|(path, text)| ((*path).to_string(), (*text).to_string()))
                    .collect(),
            )
        }
    }

    impl ParseFileLoader for FakeFiles {
        fn load(&self, path: &str, _relative_to: Option<&str>) -> Result<LoadedFile, String> {
            self.0
                .get(path)
                .map(|text| LoadedFile {
                    id: path.to_string(),
                    text: text.clone(),
                })
                .ok_or_else(|| format!("cannot read '{path}': no such file"))
        }
    }

    const EXAMPLE: &str = "import os\n\n\ndef main():\n    return os.getcwd()\n";

    /// Parses a `.. literalinclude::` over a one-file fake filesystem.
    fn parse(argument: &str, body_lines: &[&str]) -> (Directive, Diagnostics) {
        parse_over(&[("example.py", EXAMPLE)], argument, body_lines)
    }

    /// The same, over whichever files a test needs.
    fn parse_over(
        files: &[(&str, &str)],
        argument: &str,
        body_lines: &[&str],
    ) -> (Directive, Diagnostics) {
        let loader = FakeFiles::with(files);
        let ctx = ParseCtx::new(Domain::Py, &loader);
        let mut diagnostics = Diagnostics::default();
        let directive = parse_literal_include(argument, body_lines, &mut diagnostics, &ctx);
        (directive, diagnostics)
    }

    /// The block a successful parse produced.
    fn block(directive: Directive) -> CodeBlock {
        let Directive::CodeBlock(block) = directive else {
            panic!("Expected a CodeBlock, got {directive:?}");
        };
        block
    }

    fn codes(diagnostics: &Diagnostics) -> Vec<DiagnosticCode> {
        diagnostics.iter().map(|d| d.code).collect()
    }

    #[test]
    fn test_the_whole_file_is_shown_when_nothing_selects_part_of_it() {
        // Given / When
        let (directive, diagnostics) = parse("example.py", &[]);

        // Then
        assert_eq!(block(directive).content, EXAMPLE.trim_end_matches('\n'));
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_the_block_records_which_directive_wrote_it() {
        // Given / When
        let (directive, _) = parse("example.py", &[]);

        // Then
        assert_eq!(block(directive).source, CodeBlockSource::LiteralInclude);
    }

    #[test]
    fn test_a_missing_argument_is_reported() {
        // Given / When
        let (directive, diagnostics) = parse("", &[]);

        // Then
        assert!(matches!(directive, Directive::Unknown { .. }));
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::IncludeMissingPath]
        );
    }

    #[test]
    fn test_an_unreadable_file_is_reported_and_degrades_the_directive() {
        // Given a file that is not there — an undeclared `parse_data` entry
        let (directive, diagnostics) = parse("nowhere.py", &[]);

        // Then
        assert!(matches!(directive, Directive::Unknown { .. }));
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::IncludeFileUnreadable]
        );
        assert!(
            diagnostics[0].message.starts_with("literalinclude: "),
            "{}",
            diagnostics[0].message
        );
    }

    #[test]
    fn test_language_is_read_from_an_option_not_the_argument() {
        // Given — unlike `.. code-block::`, the argument is a path
        let (directive, _) = parse("example.py", &["   :language: python"]);

        // Then
        assert_eq!(
            block(directive).language,
            CodeLanguage::Named(LanguageName::new("python").expect("a known language"))
        );
    }

    #[test]
    fn test_no_language_option_inherits_from_the_enclosing_highlight() {
        // Given / When
        let (directive, _) = parse("example.py", &[]);

        // Then — the renderer resolves this against `.. highlight::`
        assert_eq!(block(directive).language, CodeLanguage::Inherit);
    }

    #[test]
    fn test_lines_selects_part_of_the_file() {
        // Given / When
        let (directive, _) = parse("example.py", &["   :lines: 4-5"]);

        // Then
        assert_eq!(
            block(directive).content,
            "def main():\n    return os.getcwd()"
        );
    }

    #[test]
    fn test_start_after_and_end_before_bracket_a_region() {
        // Given / When
        let (directive, _) = parse(
            "example.py",
            &["   :start-after: import os", "   :end-before: return"],
        );

        // Then — the blank lines between the marker and the def
        assert_eq!(block(directive).content, "\n\ndef main():");
    }

    #[test]
    fn test_lineno_match_numbers_from_the_files_own_lines() {
        // Given / When
        let (directive, diagnostics) =
            parse("example.py", &["   :lines: 4-5", "   :lineno-match:"]);

        // Then — line 4 of the file is line 4 on the page
        let block = block(directive);
        assert_eq!(block.lineno_start, NonZeroU32::new(4));
        assert!(block.linenos, "a start implies numbering");
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_lineno_match_on_a_discontinuous_selection_is_reported() {
        // Given a selection with a gap, which has no single starting line
        let (directive, diagnostics) =
            parse("example.py", &["   :lines: 1,4", "   :lineno-match:"]);

        // Then — reported rather than silently numbering from one of the runs
        assert_eq!(block(directive).lineno_start, None);
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::LiteralIncludeLinenoMatchUnusable]
        );
    }

    #[test]
    fn test_an_explicit_lineno_start_wins_over_lineno_match() {
        // Given both, which say the same thing two ways
        let (directive, _) = parse(
            "example.py",
            &[
                "   :lines: 4-5",
                "   :lineno-match:",
                "   :lineno-start: 100",
            ],
        );

        // Then — an author who typed a number meant it
        assert_eq!(block(directive).lineno_start, NonZeroU32::new(100));
    }

    #[test]
    fn test_a_bare_caption_defaults_to_the_filename() {
        // Given / When
        let (directive, _) = parse("example.py", &["   :caption:"]);

        // Then
        assert_eq!(block(directive).caption.as_deref(), Some("example.py"));
    }

    #[test]
    fn test_a_caption_with_a_value_keeps_that_value() {
        // Given / When
        let (directive, _) = parse("example.py", &["   :caption: The entry point"]);

        // Then
        assert_eq!(block(directive).caption.as_deref(), Some("The entry point"));
    }

    #[test]
    fn test_prepend_and_append_wrap_the_selected_text() {
        // Given / When
        let (directive, _) = parse(
            "example.py",
            &[
                "   :lines: 4",
                "   :prepend: # before",
                "   :append: # after",
            ],
        );

        // Then
        assert_eq!(block(directive).content, "# before\ndef main():\n# after");
    }

    #[test]
    fn test_dedent_is_applied_to_the_selected_text() {
        // Given the indented body of a function
        let (directive, _) = parse("example.py", &["   :lines: 5", "   :dedent: 4"]);

        // Then
        assert_eq!(block(directive).content, "return os.getcwd()");
    }

    #[test]
    fn test_tab_width_expands_tabs_before_anything_else_sees_them() {
        // Given a file indented with tabs
        let (directive, _) = parse_over(
            &[("tabbed.py", "def f():\n\treturn 1\n")],
            "tabbed.py",
            &["   :tab-width: 4"],
        );

        // Then
        assert_eq!(block(directive).content, "def f():\n    return 1");
    }

    #[test]
    fn test_a_non_numeric_tab_width_is_reported() {
        // Given / When
        let (_, diagnostics) = parse("example.py", &["   :tab-width: wide"]);

        // Then
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::IncludeInvalidTabWidth]
        );
    }

    #[test]
    fn test_emphasize_lines_counts_the_selected_text_not_the_file() {
        // Given a two-line selection with its second line emphasized
        let (directive, diagnostics) =
            parse("example.py", &["   :lines: 4-5", "   :emphasize-lines: 2"]);

        // Then
        assert_eq!(
            block(directive).emphasize_lines,
            vec![NonZeroU32::new(2).unwrap()]
        );
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_a_diff_option_shows_a_patch_in_the_diff_language() {
        // Given two versions of a file
        let (directive, diagnostics) = parse_over(
            &[("old.py", "x = 1\n"), ("new.py", "x = 2\n")],
            "new.py",
            &["   :diff: old.py"],
        );

        // Then
        let block = block(directive);
        assert_eq!(
            block.language,
            CodeLanguage::Named(LanguageName::new("diff").expect("diff is a known language"))
        );
        assert!(block.content.contains("-x = 1"), "{}", block.content);
        assert!(block.content.contains("+x = 2"), "{}", block.content);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_an_unreadable_diff_file_is_reported_separately() {
        // Given — its own code, so a document can suppress one and not the other
        let (directive, diagnostics) = parse("example.py", &["   :diff: nowhere.py"]);

        // Then
        assert!(matches!(directive, Directive::Unknown { .. }));
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::LiteralIncludeDiffUnreadable]
        );
    }

    #[test]
    fn test_pyobject_is_refused_with_an_actionable_message() {
        // Given / When
        let (directive, diagnostics) = parse("example.py", &["   :pyobject: main"]);

        // Then — refused rather than ignored, so the page never quietly shows
        // the whole file where one function was meant
        assert!(matches!(directive, Directive::Unknown { .. }));
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::LiteralIncludePyObjectUnsupported]
        );
        assert!(
            diagnostics[0].message.contains(":start-after:"),
            "the message should name what to use instead: {}",
            diagnostics[0].message
        );
    }

    #[test]
    fn test_a_non_utf8_encoding_is_refused() {
        // Given / When
        let (directive, diagnostics) = parse("example.py", &["   :encoding: latin-1"]);

        // Then
        assert!(matches!(directive, Directive::Unknown { .. }));
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::IncludeEncodingUnsupported]
        );
    }

    #[test]
    fn test_an_unknown_option_is_reported_under_the_include_family() {
        // Given / When
        let (_, diagnostics) = parse("example.py", &["   :nonsense: 1"]);

        // Then
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::IncludeUnknownOption]
        );
    }

    #[test]
    fn test_a_selection_matching_nothing_is_reported() {
        // Given a marker that is not in the file
        let (directive, diagnostics) = parse("example.py", &["   :start-after: nowhere"]);

        // Then
        assert!(matches!(directive, Directive::Unknown { .. }));
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::IncludeTextNotFound]
        );
    }

    #[test]
    fn test_name_and_class_are_read_from_the_shared_vocabulary() {
        // Given / When
        let (directive, _) = parse(
            "example.py",
            &["   :name: entry-point", "   :class: highlight-me"],
        );

        // Then
        let block = block(directive);
        assert!(block.name.is_some());
        assert_eq!(block.classes, vec!["highlight-me".to_string()]);
    }

    #[test]
    fn test_the_block_carries_a_span_for_render_time_diagnostics() {
        // Given / When
        let (directive, _) = parse("example.py", &["   :language: python"]);

        // Then — an unknown language is reported at render time against this
        assert!(block(directive).span.is_some());
    }
}

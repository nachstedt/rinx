//! `.. code-block::` and `.. code::` — code blocks with presentation options.
//!
//! Both lower to one [`Directive::CodeBlock`]; [`CodeBlockSource`] records
//! which spelling wrote it. The two differ only in how they say the same
//! things, and that difference is consumed here rather than left for the
//! renderer: `.. code::` writes `:number-lines:` (optionally with a start
//! value) where `.. code-block::` writes `:linenos:` plus `:lineno-start:`.
//!
//! Parsing is shallow in the same way `super::math` is: the body is collected
//! verbatim and never inspected for meaning. What this module *does* decide is
//! the structure RST expresses around it — which options were set, and what
//! `:dedent:` leaves of each line. Whether the language has a grammar behind
//! it is the renderer's question, since only it knows the highlighting
//! backend.

use std::num::NonZeroU32;

use rusty_sphinx_ast::{
    CodeBlock, CodeBlockSource, CodeLanguage, Diagnostic, DiagnosticCode, Directive,
    ResolvedLanguage, TargetName,
};

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::directives::body::body_span;
use crate::directives::options::{OptionLine, report_unknown_options, scan_option_lines};
use crate::indent::{strip_common_indent, unindent_body_lines};

/// The options recognized on a code block, before validation against the body.
struct CodeBlockOptions {
    caption: Option<String>,
    name: Option<TargetName>,
    classes: Vec<String>,
    linenos: bool,
    lineno_start: Option<NonZeroU32>,
    emphasize_lines_raw: Option<(String, usize, String)>,
    dedent: Option<Dedent>,
    force: bool,
}

/// What a `:dedent:` asked for.
///
/// A separate type rather than an `Option<usize>` because "dedent fully" and
/// "dedent by zero columns" are different instructions that an `Option` would
/// have to spell the same way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dedent {
    /// `:dedent:` with no value — strip whatever indent the lines share.
    Full,
    /// `:dedent: N` — remove exactly `N` columns from every line.
    Columns(usize),
}

impl CodeBlockOptions {
    fn empty() -> Self {
        Self {
            caption: None,
            name: None,
            classes: Vec::new(),
            linenos: false,
            lineno_start: None,
            emphasize_lines_raw: None,
            dedent: None,
            force: false,
        }
    }
}

/// Parses either code-block directive into a [`Directive::CodeBlock`].
pub(in crate::directives) fn parse_code_block(
    source: CodeBlockSource,
    argument: &str,
    body_lines: &[&str],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Directive {
    let directive = source.directive_name();
    let language = parse_language_argument(argument);

    let unindented_lines = unindent_body_lines(body_lines);
    let (option_lines, opt_idx) = scan_option_lines(&unindented_lines);
    let (options, unrecognized) = parse_code_block_options(source, &option_lines, diagnostics, ctx);
    report_unknown_options(
        &unrecognized,
        directive,
        DiagnosticCode::DirectiveUnknownOption,
        diagnostics,
        ctx,
    );

    let body: Vec<&str> = unindented_lines[opt_idx..]
        .iter()
        .map(String::as_str)
        .collect();
    let content = apply_dedent(&body, options.dedent);

    let line_count = if content.is_empty() {
        0
    } else {
        content.lines().count()
    };
    let emphasize_lines = resolve_emphasize_lines(
        options.emphasize_lines_raw.as_ref(),
        line_count,
        directive,
        diagnostics,
        ctx,
    );

    Directive::CodeBlock(CodeBlock {
        source,
        language,
        content,
        caption: options.caption,
        name: options.name,
        classes: options.classes,
        // `:lineno-start:` implies numbering: an author who says where the
        // numbers begin has already said they want them.
        linenos: options.linenos || options.lineno_start.is_some(),
        lineno_start: options.lineno_start,
        emphasize_lines,
        force: options.force,
        span: body_span(body_lines, ctx),
    })
}

/// Reads the directive's argument as a language.
///
/// Infallible: an empty argument is [`CodeLanguage::Inherit`], a meaningful
/// answer rather than a mistake, so unlike `.. highlight::` there is nothing
/// here to diagnose.
fn parse_language_argument(argument: &str) -> CodeLanguage {
    CodeLanguage::parse(argument)
}

/// Consumes the options a code block knows, returning them with the lines it
/// did not recognize, in source order.
fn parse_code_block_options<'a>(
    source: CodeBlockSource,
    option_lines: &'a [OptionLine],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> (CodeBlockOptions, Vec<&'a OptionLine>) {
    let directive = source.directive_name();
    let mut options = CodeBlockOptions::empty();
    let mut unrecognized = Vec::new();

    for line in option_lines {
        match (source, line.name.as_str()) {
            // Shared by both spellings.
            (_, "name") => {
                if line.value.is_empty() {
                    diagnostics.push(Diagnostic::at(
                        DiagnosticCode::CodeBlockEmptyName,
                        format!(
                            "{directive}: :name: needs a value, so nothing can reference this block"
                        ),
                        ctx.line_span(line.line_index, &line.raw),
                    ));
                } else {
                    options.name = Some(TargetName::new(&line.value));
                }
            }
            (_, "class") => {
                options.classes = line.value.split_whitespace().map(str::to_string).collect();
            }
            (_, "caption") => {
                if line.value.is_empty() {
                    unrecognized.push(line);
                } else {
                    options.caption = Some(line.value.clone());
                }
            }
            // `.. code-block::`'s spelling of line numbering.
            (CodeBlockSource::CodeBlock, "linenos") => options.linenos = true,
            (CodeBlockSource::CodeBlock, "lineno-start") => {
                options.lineno_start = parse_positive_integer(
                    &line.value,
                    "lineno-start",
                    directive,
                    line,
                    diagnostics,
                    ctx,
                );
            }
            (CodeBlockSource::CodeBlock, "emphasize-lines") => {
                options.emphasize_lines_raw =
                    Some((line.value.clone(), line.line_index, line.raw.clone()));
            }
            (CodeBlockSource::CodeBlock, "dedent") => {
                options.dedent = parse_dedent(&line.value, directive, line, diagnostics, ctx);
            }
            (CodeBlockSource::CodeBlock, "force") => options.force = true,
            // `.. code::`'s spelling: one option carrying both facts.
            (CodeBlockSource::Code, "number-lines") => {
                options.linenos = true;
                if !line.value.is_empty() {
                    options.lineno_start = parse_positive_integer(
                        &line.value,
                        "number-lines",
                        directive,
                        line,
                        diagnostics,
                        ctx,
                    );
                }
            }
            _ => unrecognized.push(line),
        }
    }

    (options, unrecognized)
}

/// Reads an option value that must be a positive integer.
///
/// Returns `None` on a bad value *after* reporting it, so a mistyped option is
/// dropped rather than silently becoming a plausible-looking default.
fn parse_positive_integer(
    value: &str,
    option: &str,
    directive: &str,
    line: &OptionLine,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<NonZeroU32> {
    let Ok(parsed) = value.trim().parse::<NonZeroU32>() else {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::CodeBlockInvalidInteger,
            format!(
                "{directive}: :{option}: needs a positive integer, got '{}'",
                line.value
            ),
            ctx.line_span(line.line_index, &line.raw),
        ));
        return None;
    };
    Some(parsed)
}

/// Reads a `:dedent:` value: absent means "all shared indent", a number means
/// exactly that many columns.
fn parse_dedent(
    value: &str,
    directive: &str,
    line: &OptionLine,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<Dedent> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Some(Dedent::Full);
    }
    let Ok(columns) = trimmed.parse::<usize>() else {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::CodeBlockInvalidInteger,
            format!("{directive}: :dedent: needs a non-negative integer, got '{value}'"),
            ctx.line_span(line.line_index, &line.raw),
        ));
        return None;
    };
    Some(Dedent::Columns(columns))
}

/// Applies `:dedent:` to the already-unindented body.
///
/// With no option the body is returned with its shared indent stripped, which
/// is what every literal block does. `Columns(n)` removes exactly `n` columns,
/// leaving a shorter line untouched rather than erroring — docutils' own
/// behaviour, and the only choice that keeps a blank line blank.
fn apply_dedent(body: &[&str], dedent: Option<Dedent>) -> String {
    match dedent {
        None | Some(Dedent::Full) => strip_common_indent(body),
        Some(Dedent::Columns(columns)) => {
            let stripped: Vec<String> = body
                .iter()
                .map(|line| {
                    let leading = line
                        .chars()
                        .take_while(|c| c.is_whitespace() && *c != '\n')
                        .count();
                    let cut = leading.min(columns);
                    line.chars().skip(cut).collect()
                })
                .collect();
            trim_blank_edges(&stripped)
        }
    }
}

/// Drops leading and trailing blank lines and joins the rest, matching what
/// [`strip_common_indent`] does so both `:dedent:` paths agree.
fn trim_blank_edges(lines: &[String]) -> String {
    let start = lines.iter().position(|line| !line.trim().is_empty());
    let Some(start) = start else {
        return String::new();
    };
    let end = lines
        .iter()
        .rposition(|line| !line.trim().is_empty())
        .unwrap_or(start);
    lines[start..=end].join("\n")
}

/// Expands an `:emphasize-lines:` value into sorted, deduplicated line
/// numbers, dropping and reporting any that the block does not have.
fn resolve_emphasize_lines(
    raw: Option<&(String, usize, String)>,
    line_count: usize,
    directive: &str,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Vec<NonZeroU32> {
    let Some((value, line_index, raw_line)) = raw else {
        return Vec::new();
    };

    let mut report = |message: String| {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::CodeBlockEmphasizeLinesInvalid,
            format!("{directive}: :emphasize-lines: {message}"),
            ctx.line_span(*line_index, raw_line),
        ));
    };

    let mut lines = Vec::new();
    for part in value.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        match parse_line_range(part) {
            Some(range) => lines.extend(range),
            None => report(format!("could not read '{part}'")),
        }
    }

    lines.sort_unstable();
    lines.dedup();

    let limit = u32::try_from(line_count).unwrap_or(u32::MAX);
    let (within, beyond): (Vec<NonZeroU32>, Vec<NonZeroU32>) =
        lines.into_iter().partition(|line| line.get() <= limit);
    for line in &beyond {
        report(format!(
            "line {line} is past the end of a {line_count}-line block"
        ));
    }
    within
}

/// Reads one comma-separated entry: either `4` or a `3-5` range.
fn parse_line_range(part: &str) -> Option<Vec<NonZeroU32>> {
    if let Some((first, last)) = part.split_once('-') {
        let first: NonZeroU32 = first.trim().parse().ok()?;
        let last: NonZeroU32 = last.trim().parse().ok()?;
        if last < first {
            return None;
        }
        return Some(
            (first.get()..=last.get())
                .filter_map(NonZeroU32::new)
                .collect(),
        );
    }
    part.parse::<NonZeroU32>().ok().map(|line| vec![line])
}

/// Parses a `.. highlight::`, which sets the language following blocks
/// inherit.
pub(in crate::directives) fn parse_highlight(
    argument: &str,
    body_lines: &[&str],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Directive {
    const DIRECTIVE: &str = "highlight";

    let language = ResolvedLanguage::parse(argument).unwrap_or_else(|_| {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::CodeBlockEmptyLanguage,
            format!(
                "{DIRECTIVE}: needs a language argument; following blocks keep the current one"
            ),
            body_span(body_lines, ctx).or_else(|| ctx.line_span(0, "")),
        ));
        // Degrading to the Sphinx default rather than dropping the directive
        // keeps the document renderable, which is the parser's job; the
        // diagnostic is what tells the author.
        ResolvedLanguage::default()
    });

    let unindented_lines = unindent_body_lines(body_lines);
    let (option_lines, _) = scan_option_lines(&unindented_lines);
    let mut linenothreshold = None;
    let mut force = false;
    let mut unrecognized = Vec::new();

    for line in &option_lines {
        match line.name.as_str() {
            "linenothreshold" => {
                linenothreshold = parse_positive_integer(
                    &line.value,
                    "linenothreshold",
                    DIRECTIVE,
                    line,
                    diagnostics,
                    ctx,
                );
            }
            "force" => force = true,
            _ => unrecognized.push(line),
        }
    }
    report_unknown_options(
        &unrecognized,
        DIRECTIVE,
        DiagnosticCode::DirectiveUnknownOption,
        diagnostics,
        ctx,
    );

    Directive::Highlight {
        language,
        linenothreshold,
        force,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_sphinx_ast::{Domain, LanguageName};

    /// Parses a `.. code-block::` body, returning the block and any
    /// diagnostics. `body_lines` are indented as `collect_directive_body`
    /// hands them over.
    fn parse(argument: &str, body_lines: &[&str]) -> (CodeBlock, Diagnostics) {
        parse_as(CodeBlockSource::CodeBlock, argument, body_lines)
    }

    fn parse_as(
        source: CodeBlockSource,
        argument: &str,
        body_lines: &[&str],
    ) -> (CodeBlock, Diagnostics) {
        let mut diagnostics = Diagnostics::default();
        let directive = parse_code_block(
            source,
            argument,
            body_lines,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );
        let Directive::CodeBlock(block) = directive else {
            panic!("Expected a CodeBlock, got {directive:?}");
        };
        (block, diagnostics)
    }

    fn highlight(argument: &str, body_lines: &[&str]) -> (Directive, Diagnostics) {
        let mut diagnostics = Diagnostics::default();
        let directive = parse_highlight(
            argument,
            body_lines,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );
        (directive, diagnostics)
    }

    fn codes(diagnostics: &Diagnostics) -> Vec<DiagnosticCode> {
        diagnostics.iter().map(|d| d.code).collect()
    }

    #[test]
    fn test_parse_code_block_keeps_the_body_verbatim() {
        // Given — a body whose relative indentation carries meaning
        let body = ["   def f():", "       return 1"];

        // When
        let (block, diagnostics) = parse("python", &body);

        // Then
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(block.content, "def f():\n    return 1");
    }

    #[test]
    fn test_parse_code_block_reads_the_language_argument() {
        // Given / When
        let (block, _) = parse("Python", &["   x = 1"]);

        // Then — normalized on the way in
        assert_eq!(
            block.language,
            CodeLanguage::Named(LanguageName::new("python").unwrap())
        );
    }

    #[test]
    fn test_parse_code_block_treats_a_missing_argument_as_inherit() {
        // Given / When
        let (block, diagnostics) = parse("", &["   x = 1"]);

        // Then — no diagnostic: "nothing written" is meaningful here
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(block.language, CodeLanguage::Inherit);
    }

    #[test]
    fn test_parse_code_block_keeps_option_lines_out_of_the_content() {
        // Given — the bug this whole change exists to fix: option lines used
        // to be rendered as code text
        let body = ["   :linenos:", "   :caption: Example", "", "   x = 1"];

        // When
        let (block, diagnostics) = parse("python", &body);

        // Then
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(block.content, "x = 1");
        assert!(block.linenos);
        assert_eq!(block.caption.as_deref(), Some("Example"));
    }

    #[test]
    fn test_parse_code_block_reads_name_and_class() {
        // Given
        let body = [
            "   :name: my-block",
            "   :class: boxed wide",
            "",
            "   x = 1",
        ];

        // When
        let (block, diagnostics) = parse("python", &body);

        // Then
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(block.name, Some(TargetName::new("my-block")));
        assert_eq!(block.classes, vec!["boxed", "wide"]);
    }

    #[test]
    fn test_parse_code_block_reports_a_valueless_name() {
        // Given — a `:name:` nothing could ever reference
        let body = ["   :name:", "", "   x = 1"];

        // When
        let (block, diagnostics) = parse("python", &body);

        // Then
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::CodeBlockEmptyName]
        );
        assert_eq!(block.name, None);
    }

    #[test]
    fn test_parse_code_block_lineno_start_implies_line_numbers() {
        // Given — an author who says where numbering starts has asked for it
        let body = ["   :lineno-start: 10", "", "   x = 1"];

        // When
        let (block, diagnostics) = parse("python", &body);

        // Then
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert!(block.linenos);
        assert_eq!(block.lineno_start, NonZeroU32::new(10));
    }

    #[test]
    fn test_parse_code_block_reports_a_non_numeric_lineno_start() {
        // Given
        let body = ["   :lineno-start: ten", "", "   x = 1"];

        // When
        let (block, diagnostics) = parse("python", &body);

        // Then — dropped rather than becoming a plausible-looking default
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::CodeBlockInvalidInteger]
        );
        assert_eq!(block.lineno_start, None);
    }

    #[test]
    fn test_parse_code_block_rejects_a_zero_lineno_start() {
        // Given — a listing starting at line zero is not a thing
        let body = ["   :lineno-start: 0", "", "   x = 1"];

        // When
        let (_, diagnostics) = parse("python", &body);

        // Then — the type refuses it, so no special case was needed
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::CodeBlockInvalidInteger]
        );
    }

    #[test]
    fn test_parse_code_block_expands_an_emphasize_lines_range() {
        // Given
        let body = [
            "   :emphasize-lines: 1,3-5",
            "",
            "   a",
            "   b",
            "   c",
            "   d",
            "   e",
        ];

        // When
        let (block, diagnostics) = parse("python", &body);

        // Then
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        let emphasized: Vec<u32> = block.emphasize_lines.iter().map(|l| l.get()).collect();
        assert_eq!(emphasized, vec![1, 3, 4, 5]);
    }

    #[test]
    fn test_parse_code_block_sorts_and_deduplicates_emphasized_lines() {
        // Given — written out of order, with an overlap
        let body = ["   :emphasize-lines: 3,1,2-3", "", "   a", "   b", "   c"];

        // When
        let (block, _) = parse("python", &body);

        // Then
        let emphasized: Vec<u32> = block.emphasize_lines.iter().map(|l| l.get()).collect();
        assert_eq!(emphasized, vec![1, 2, 3]);
    }

    #[test]
    fn test_parse_code_block_reports_an_emphasized_line_past_the_end() {
        // Given — a two-line block asked to emphasize line nine
        let body = ["   :emphasize-lines: 1,9", "", "   a", "   b"];

        // When
        let (block, diagnostics) = parse("python", &body);

        // Then — the valid one survives, the impossible one is reported
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::CodeBlockEmphasizeLinesInvalid]
        );
        let emphasized: Vec<u32> = block.emphasize_lines.iter().map(|l| l.get()).collect();
        assert_eq!(emphasized, vec![1]);
    }

    #[test]
    fn test_parse_code_block_reports_an_unreadable_emphasize_lines_entry() {
        // Given
        let body = ["   :emphasize-lines: 1,two", "", "   a", "   b"];

        // When
        let (block, diagnostics) = parse("python", &body);

        // Then
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::CodeBlockEmphasizeLinesInvalid]
        );
        let emphasized: Vec<u32> = block.emphasize_lines.iter().map(|l| l.get()).collect();
        assert_eq!(emphasized, vec![1]);
    }

    #[test]
    fn test_parse_code_block_rejects_a_backwards_emphasize_range() {
        // Given — `5-3` names no lines at all
        let body = ["   :emphasize-lines: 5-3", "", "   a", "   b", "   c"];

        // When
        let (block, diagnostics) = parse("python", &body);

        // Then
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::CodeBlockEmphasizeLinesInvalid]
        );
        assert!(block.emphasize_lines.is_empty());
    }

    #[test]
    fn test_parse_code_block_dedents_by_an_explicit_column_count() {
        // Given — a body whose author wants two columns removed
        let body = ["   :dedent: 2", "", "     x = 1", "       y = 2"];

        // When
        let (block, diagnostics) = parse("python", &body);

        // Then — relative indentation survives, two columns lighter
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(block.content, "x = 1\n  y = 2");
    }

    #[test]
    fn test_parse_code_block_dedent_without_a_value_strips_all_shared_indent() {
        // Given
        let body = ["   :dedent:", "", "     x = 1", "       y = 2"];

        // When
        let (block, _) = parse("python", &body);

        // Then
        assert_eq!(block.content, "x = 1\n  y = 2");
    }

    #[test]
    fn test_apply_dedent_leaves_a_shorter_line_alone() {
        // Given — a line with less indent than the dedent asks to remove.
        // Tested against `apply_dedent` directly rather than through a
        // document: a directive body line indented less than its own option
        // lines would have ended the directive, so this shape only reaches
        // here from an explicit `:dedent:` wider than some line's indent.
        let body = ["    x = 1", "  y = 2"];

        // When
        let content = apply_dedent(&body, Some(Dedent::Columns(4)));

        // Then — it loses its indent, and no content is cut off with it
        assert_eq!(content, "x = 1\ny = 2");
    }

    #[test]
    fn test_apply_dedent_keeps_a_blank_line_blank() {
        // Given — a blank line has no indent to remove
        let body = ["    x = 1", "", "    y = 2"];

        // When
        let content = apply_dedent(&body, Some(Dedent::Columns(4)));

        // Then
        assert_eq!(content, "x = 1\n\ny = 2");
    }

    #[test]
    fn test_parse_code_block_reports_a_non_numeric_dedent() {
        // Given
        let body = ["   :dedent: lots", "", "   x = 1"];

        // When
        let (_, diagnostics) = parse("python", &body);

        // Then
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::CodeBlockInvalidInteger]
        );
    }

    #[test]
    fn test_parse_code_block_reads_force() {
        // Given / When
        let (block, diagnostics) = parse("python", &["   :force:", "", "   x = 1"]);

        // Then
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert!(block.force);
    }

    #[test]
    fn test_parse_code_block_reports_an_unknown_option() {
        // Given
        let body = ["   :nonsense: yes", "", "   x = 1"];

        // When
        let (_, diagnostics) = parse("python", &body);

        // Then
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::DirectiveUnknownOption]
        );
    }

    #[test]
    fn test_parse_code_block_carries_a_span_for_render_time_diagnostics() {
        // Given — the renderer must be able to point at an unknown language
        let body = ["   x = 1"];

        // When
        let (block, _) = parse("nonesuch", &body);

        // Then
        assert!(block.span.is_some());
    }

    #[test]
    fn test_parse_code_directive_reads_number_lines_as_line_numbering() {
        // Given — docutils' spelling of `:linenos:`
        let body = ["   :number-lines:", "", "   x = 1"];

        // When
        let (block, diagnostics) = parse_as(CodeBlockSource::Code, "python", &body);

        // Then
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert!(block.linenos);
        assert_eq!(block.lineno_start, None);
    }

    #[test]
    fn test_parse_code_directive_reads_the_start_value_of_number_lines() {
        // Given — the one option carrying both facts
        let body = ["   :number-lines: 7", "", "   x = 1"];

        // When
        let (block, diagnostics) = parse_as(CodeBlockSource::Code, "python", &body);

        // Then
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert!(block.linenos);
        assert_eq!(block.lineno_start, NonZeroU32::new(7));
    }

    #[test]
    fn test_parse_code_directive_does_not_accept_sphinx_only_options() {
        // Given — `:linenos:` is not docutils' spelling, so `.. code::`
        // must not silently honour it
        let body = ["   :linenos:", "", "   x = 1"];

        // When
        let (block, diagnostics) = parse_as(CodeBlockSource::Code, "python", &body);

        // Then
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::DirectiveUnknownOption]
        );
        assert!(!block.linenos);
    }

    #[test]
    fn test_parse_code_block_does_not_accept_the_docutils_only_option() {
        // Given — and the reverse, so neither directive drifts into the other
        let body = ["   :number-lines:", "", "   x = 1"];

        // When
        let (block, diagnostics) = parse("python", &body);

        // Then
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::DirectiveUnknownOption]
        );
        assert!(!block.linenos);
    }

    #[test]
    fn test_parse_code_block_records_which_directive_wrote_it() {
        // Given / When
        let (sphinx, _) = parse_as(CodeBlockSource::CodeBlock, "python", &["   x = 1"]);
        let (docutils, _) = parse_as(CodeBlockSource::Code, "python", &["   x = 1"]);

        // Then
        assert_eq!(sphinx.source, CodeBlockSource::CodeBlock);
        assert_eq!(docutils.source, CodeBlockSource::Code);
    }

    #[test]
    fn test_parse_highlight_reads_the_language() {
        // Given / When
        let (directive, diagnostics) = highlight("Rust", &[]);

        // Then
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        let Directive::Highlight { language, .. } = directive else {
            panic!("Expected Highlight, got {directive:?}");
        };
        assert_eq!(
            language,
            ResolvedLanguage::Named(LanguageName::new("rust").unwrap())
        );
    }

    #[test]
    fn test_parse_highlight_reports_a_missing_language() {
        // Given — unlike a code block, this has no "inherit" to fall back to
        let (directive, diagnostics) = highlight("", &[]);

        // Then — reported, but the document stays renderable
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::CodeBlockEmptyLanguage]
        );
        let Directive::Highlight { language, .. } = directive else {
            panic!("Expected Highlight, got {directive:?}");
        };
        assert_eq!(language, ResolvedLanguage::default());
    }

    #[test]
    fn test_parse_highlight_reads_linenothreshold() {
        // Given / When
        let (directive, diagnostics) = highlight("python", &["   :linenothreshold: 5"]);

        // Then
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        let Directive::Highlight {
            linenothreshold, ..
        } = directive
        else {
            panic!("Expected Highlight, got {directive:?}");
        };
        assert_eq!(linenothreshold, NonZeroU32::new(5));
    }

    #[test]
    fn test_parse_highlight_reports_a_non_numeric_linenothreshold() {
        // Given / When
        let (directive, diagnostics) = highlight("python", &["   :linenothreshold: many"]);

        // Then
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::CodeBlockInvalidInteger]
        );
        let Directive::Highlight {
            linenothreshold, ..
        } = directive
        else {
            panic!("Expected Highlight, got {directive:?}");
        };
        assert_eq!(linenothreshold, None);
    }

    #[test]
    fn test_parse_highlight_accepts_none_as_a_language() {
        // Given — turning highlighting off for what follows
        let (directive, diagnostics) = highlight("none", &[]);

        // Then
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        let Directive::Highlight { language, .. } = directive else {
            panic!("Expected Highlight, got {directive:?}");
        };
        assert_eq!(language, ResolvedLanguage::None);
    }

    #[test]
    fn test_parse_highlight_reports_an_unknown_option() {
        // Given / When
        let (_, diagnostics) = highlight("python", &["   :nonsense:"]);

        // Then
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::DirectiveUnknownOption]
        );
    }

    #[test]
    fn test_apply_dedent_returns_an_empty_string_for_a_blank_body() {
        // Given — a directive with nothing but blank lines under it
        let body = ["   ", "   "];

        // When
        let content = apply_dedent(&body, Some(Dedent::Columns(2)));

        // Then
        assert_eq!(content, "");
    }

    #[test]
    fn test_parse_line_range_reads_a_single_line() {
        // Given / When
        let parsed = parse_line_range("4");

        // Then
        assert_eq!(parsed, Some(vec![NonZeroU32::new(4).unwrap()]));
    }

    #[test]
    fn test_parse_line_range_rejects_zero() {
        // Given — line numbering is 1-based
        // When / Then
        assert_eq!(parse_line_range("0"), None);
    }

    #[test]
    fn test_parse_line_range_reads_an_inclusive_range() {
        // Given / When
        let parsed = parse_line_range("2-4").expect("a well-formed range");

        // Then — inclusive at both ends
        let lines: Vec<u32> = parsed.iter().map(|l| l.get()).collect();
        assert_eq!(lines, vec![2, 3, 4]);
    }
}

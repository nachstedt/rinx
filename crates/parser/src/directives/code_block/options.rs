//! The options a code block's directives accept, and the option values whose
//! parsing several of them share.
//!
//! Split from the block parser itself because the three directives that reach
//! here — `.. code-block::`, `.. code::` and `.. literalinclude::` — agree on
//! what a code block *is* and disagree only on how their options spell it.
//! Keeping the vocabulary in one place is what makes that difference small.

use std::num::NonZeroU32;

use rusty_sphinx_ast::{CodeBlockSource, CodeLanguage, Diagnostic, DiagnosticCode, TargetName};

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::directives::options::OptionLine;

use super::dedent::{Dedent, parse_dedent};

/// The options recognized on a code block, before validation against the body.
pub(super) struct CodeBlockOptions {
    pub(super) caption: Option<String>,
    pub(super) name: Option<TargetName>,
    pub(super) classes: Vec<String>,
    pub(super) linenos: bool,
    pub(super) lineno_start: Option<NonZeroU32>,
    pub(super) emphasize_lines_raw: Option<(String, usize, String)>,
    pub(super) dedent: Option<Dedent>,
    pub(super) force: bool,
}

impl CodeBlockOptions {
    pub(super) fn empty() -> Self {
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

/// Consumes the options a code block knows, returning them with the lines it
/// did not recognize, in source order.
pub(super) fn parse_code_block_options<'a>(
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
            // Sphinx's spelling of line numbering, shared by `.. code-block::`
            // and `.. literalinclude::`.
            (CodeBlockSource::CodeBlock | CodeBlockSource::LiteralInclude, "linenos") => {
                options.linenos = true;
            }
            (CodeBlockSource::CodeBlock | CodeBlockSource::LiteralInclude, "lineno-start") => {
                options.lineno_start = parse_positive_integer(
                    &line.value,
                    "lineno-start",
                    directive,
                    line,
                    diagnostics,
                    ctx,
                );
            }
            (CodeBlockSource::CodeBlock | CodeBlockSource::LiteralInclude, "emphasize-lines") => {
                options.emphasize_lines_raw =
                    Some((line.value.clone(), line.line_index, line.raw.clone()));
            }
            (CodeBlockSource::CodeBlock | CodeBlockSource::LiteralInclude, "dedent") => {
                options.dedent = parse_dedent(&line.value, directive, line, diagnostics, ctx);
            }
            (CodeBlockSource::CodeBlock | CodeBlockSource::LiteralInclude, "force") => {
                options.force = true;
            }
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
pub(super) fn parse_positive_integer(
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

/// Reads the directive's argument as a language.
///
/// Infallible: an empty argument is [`CodeLanguage::Inherit`], a meaningful
/// answer rather than a mistake, so unlike `.. highlight::` there is nothing
/// here to diagnose.
pub(super) fn parse_language_argument(argument: &str) -> CodeLanguage {
    CodeLanguage::parse(argument)
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{codes, parse, parse_as};
    use rusty_sphinx_ast::{CodeBlockSource, DiagnosticCode, TargetName};
    use std::num::NonZeroU32;

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
}

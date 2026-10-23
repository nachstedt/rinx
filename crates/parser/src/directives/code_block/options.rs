//! The options a code block's directives accept, and the option values whose
//! parsing several of them share.
//!
//! Split from the block parser itself because the three directives that reach
//! here — `.. code-block::`, `.. code::` and `.. literalinclude::` — agree on
//! what a code block *is* and disagree only on how their options spell it.
//! Keeping the vocabulary in one place is what makes that difference small.

use std::num::NonZeroU32;

use rinx_ast::{CodeBlockSource, CodeLanguage, TargetName};

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::directives::option_value::{Classes, Located, OptionValueType};
use crate::directives::options::{
    OptionLine, classify_options, directive_options, report_unknown_options,
};
use crate::directives::spec::OptionValue;

use super::dedent::Dedent;

/// The options recognized on a code block, before validation against the body.
pub(super) struct CodeBlockOptions {
    pub(super) caption: Option<String>,
    pub(super) name: Option<TargetName>,
    pub(super) classes: Vec<String>,
    pub(super) linenos: bool,
    pub(super) lineno_start: Option<NonZeroU32>,
    pub(super) emphasize_lines_raw: Option<Located<String>>,
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

directive_options! {
    /// The options every code block directive takes.
    pub(super) enum CommonCodeOption {
        /// `:name:`.
        Name(TargetName) => (["name"], "A label to reference the block by."),
        /// `:class:`.
        Class(Classes) => (["class"], "Classes for the block."),
        /// `:caption:`.
        Caption => (["caption"], Text, "A caption shown above the block."),
    }
}

directive_options! {
    /// Sphinx's spelling of numbering, emphasis, dedent and highlighting,
    /// shared by `.. code-block::` and `.. literalinclude::`.
    pub(super) enum SphinxCodeOption {
        /// `:linenos:`.
        Linenos => (["linenos"], Flag, "Numbers the lines."),
        /// `:lineno-start:`.
        LinenoStart(NonZeroU32) => (["lineno-start"], CodeBlockInvalidInteger, "The number the first line gets, which numbers the lines."),
        /// `:emphasize-lines:`.
        EmphasizeLines(Located<String>) => (["emphasize-lines"], "The lines to emphasize, as numbers and ranges."),
        /// `:dedent:`.
        Dedent(Dedent) => (["dedent"], CodeBlockInvalidInteger, "Strips this much indentation, or the common indentation with no value."),
        /// `:force:`.
        Force => (["force"], Flag, "Highlights even where the language does not fit."),
    }
}

directive_options! {
    /// The options `.. code-block::` takes: Sphinx's eight.
    pub(super) enum CodeBlockOption {
        ;
        Common(CommonCodeOption),
        Sphinx(SphinxCodeOption),
    }
}

directive_options! {
    /// The options docutils' `.. code::` takes, with Sphinx's `:force:`.
    pub(super) enum CodeOption {
        /// `:number-lines:`.
        NumberLines(LineNumbering) => (["number-lines"], CodeBlockInvalidInteger, "Numbers the lines, from the number given if any."),
        /// `:force:`.
        Force => (["force"], Flag, "Highlights even where the language does not fit.");
        Common(CommonCodeOption),
    }
}

/// Reads the options of an inline code block, spelled `source`: `.. code::`
/// numbers its lines docutils' way, `.. code-block::` Sphinx's.
pub(super) fn parse_code_block_options(
    source: CodeBlockSource,
    option_lines: &[OptionLine],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> CodeBlockOptions {
    let directive = source.directive_name();
    let mut options = CodeBlockOptions::empty();
    if source == CodeBlockSource::Code {
        for (option, line) in
            classify_options::<CodeOption>(option_lines, directive, diagnostics, ctx)
        {
            match option {
                // One option carrying both facts.
                CodeOption::NumberLines(numbering) => {
                    options.linenos = true;
                    options.lineno_start = numbering.0;
                }
                CodeOption::Force => options.force = true,
                CodeOption::Common(common) => {
                    read_common_option(common, line, &mut options, directive, diagnostics, ctx);
                }
            }
        }
        return options;
    }
    for (option, line) in
        classify_options::<CodeBlockOption>(option_lines, directive, diagnostics, ctx)
    {
        match option {
            CodeBlockOption::Common(common) => {
                read_common_option(common, line, &mut options, directive, diagnostics, ctx);
            }
            CodeBlockOption::Sphinx(sphinx) => {
                read_sphinx_option(sphinx, line, &mut options, directive, diagnostics, ctx);
            }
        }
    }
    options
}

/// Records one of the options every code block takes. A `:caption:` with no
/// value is reported, since the block would show an empty one; the caller
/// that gives a bare caption a meaning of its own handles it before this.
pub(super) fn read_common_option(
    option: CommonCodeOption,
    line: &OptionLine,
    options: &mut CodeBlockOptions,
    directive: &str,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) {
    match option {
        CommonCodeOption::Name(name) => options.name = Some(name),
        CommonCodeOption::Class(classes) => options.classes = classes.0,
        CommonCodeOption::Caption => {
            if line.value.is_empty() {
                report_unknown_options(&[line], directive, diagnostics, ctx);
            } else {
                options.caption = Some(line.value.clone());
            }
        }
    }
}

/// Records one of Sphinx's numbering, emphasis, dedent and highlighting
/// options.
pub(super) fn read_sphinx_option(
    option: SphinxCodeOption,
    _line: &OptionLine,
    options: &mut CodeBlockOptions,
    _directive: &str,
    _diagnostics: &mut Diagnostics,
    _ctx: &ParseCtx<'_>,
) {
    match option {
        SphinxCodeOption::Linenos => options.linenos = true,
        SphinxCodeOption::LinenoStart(start) => options.lineno_start = Some(start),
        SphinxCodeOption::EmphasizeLines(lines) => options.emphasize_lines_raw = Some(lines),
        SphinxCodeOption::Dedent(dedent) => options.dedent = Some(dedent),
        SphinxCodeOption::Force => options.force = true,
    }
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
    use rinx_ast::{CodeBlockSource, DiagnosticCode, TargetName};
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
            vec![DiagnosticCode::DirectiveEmptyOptionValue]
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

/// A `:number-lines:`: line numbers, from the number given if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct LineNumbering(Option<NonZeroU32>);

impl OptionValueType for LineNumbering {
    const VALUE: OptionValue = OptionValue::Integer;

    fn read(line: &OptionLine, ctx: &ParseCtx<'_>) -> Result<Self, String> {
        if line.value.trim().is_empty() {
            return Ok(Self(None));
        }
        NonZeroU32::read(line, ctx).map(|start| Self(Some(start)))
    }
}

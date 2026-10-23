//! The `sphinx.ext.doctest` family's shared option sub-syntax: scanning the
//! contiguous `:option:` block at the top of a directive's body and resolving
//! each option's value, independent of which directive kind is asking (see
//! [`super`] for the per-kind acceptance rules and directive assembly
//! this scanner feeds into).

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::directives::options::{OptionLine, classify_options, directive_options};
use rinx_ast::{
    Diagnostic, DiagnosticCode, DocTestFlag, DocTestFlagName, DocTestTrim, PyVersionSpec, Span,
};

use super::kind::DocTestDirectiveKind;

directive_options! {
    /// `:skipif:`, which every directive of the family takes.
    pub(super) enum SkipOption {
        /// `:skipif:`.
        SkipIf(String) => (["skipif"], "A Python expression; the block is skipped when it is true."),
    }
}

directive_options! {
    /// The options of the directives whose code is shown.
    pub(super) enum ShownOption {
        /// `:hide:`.
        Hide => (["hide"], Flag, "Runs the block without showing it."),
        /// `:pyversion:`.
        PyVersion(PyVersionSpec) => (["pyversion"], DoctestPyVersionInvalid, "The Python versions the block runs on, as `> 3.9`."),
        /// `:trim-doctest-flags:`.
        TrimDoctestFlags => (["trim-doctest-flags"], Flag, "Hides the block's doctest flags."),
        /// `:no-trim-doctest-flags:`.
        NoTrimDoctestFlags => (["no-trim-doctest-flags"], Flag, "Shows the block's doctest flags."),
    }
}

directive_options! {
    /// The options `.. doctest::` and `.. testoutput::` take: every one, since
    /// both state output a run is compared against.
    pub(super) enum DocTestOption {
        /// `:options:`.
        Options => (["options"], Text, "Doctest flags the comparison runs with, as `+ELLIPSIS`.");
        Shown(ShownOption),
        Skip(SkipOption),
    }
}

directive_options! {
    /// The options `.. testcode::` takes: no `:options:`, since it states no
    /// output of its own — its companion `.. testoutput::` carries them.
    pub(super) enum TestCodeOption {
        ;
        Shown(ShownOption),
        Skip(SkipOption),
    }
}

directive_options! {
    /// The options `.. testsetup::` and `.. testcleanup::` take: only
    /// `:skipif:`, since they are never shown and never compared.
    pub(super) enum TestSetupOption {
        ;
        Skip(SkipOption),
    }
}

/// Every option the shared scanner can produce, before each kind takes the
/// subset it accepts.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct DocTestOptions {
    pub(super) hide: bool,
    pub(super) flags: Vec<DocTestFlag>,
    pub(super) pyversion: Option<PyVersionSpec>,
    pub(super) skipif: Option<String>,
    pub(super) trim: DocTestTrim,
}

/// Reads the option lines the reader took off the directive's block, each
/// positioned by `ctx`, reporting every one `kind` does not take.
pub(super) fn scan_options(
    kind: DocTestDirectiveKind,
    option_lines: &[OptionLine],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> DocTestOptions {
    let mut options = DocTestOptions {
        hide: false,
        flags: Vec::new(),
        pyversion: None,
        skipif: None,
        trim: DocTestTrim::Unset,
    };
    let directive = kind.as_name();
    let span = |line: &OptionLine| ctx.line_span(line.line_index, &line.raw);
    match kind {
        DocTestDirectiveKind::Doctest | DocTestDirectiveKind::TestOutput => {
            for (option, line) in
                classify_options::<DocTestOption>(option_lines, directive, diagnostics, ctx)
            {
                match option {
                    DocTestOption::Options => {
                        options.flags = parse_flag_list(kind, &line.value, diagnostics, span(line));
                    }
                    DocTestOption::Shown(option) => read_shown(option, &mut options),
                    DocTestOption::Skip(SkipOption::SkipIf(condition)) => {
                        options.skipif = Some(condition);
                    }
                }
            }
        }
        DocTestDirectiveKind::TestCode => {
            for (option, _) in
                classify_options::<TestCodeOption>(option_lines, directive, diagnostics, ctx)
            {
                match option {
                    TestCodeOption::Shown(option) => read_shown(option, &mut options),
                    TestCodeOption::Skip(SkipOption::SkipIf(condition)) => {
                        options.skipif = Some(condition);
                    }
                }
            }
        }
        DocTestDirectiveKind::TestSetup | DocTestDirectiveKind::TestCleanup => {
            for (option, _) in
                classify_options::<TestSetupOption>(option_lines, directive, diagnostics, ctx)
            {
                match option {
                    TestSetupOption::Skip(SkipOption::SkipIf(condition)) => {
                        options.skipif = Some(condition);
                    }
                }
            }
        }
    }
    options
}

/// Records one of the options of a directive whose code is shown.
fn read_shown(option: ShownOption, options: &mut DocTestOptions) {
    match option {
        ShownOption::Hide => options.hide = true,
        // A bad one was reported, not silently dropped: ignoring a
        // `:pyversion:` would run a block that Sphinx would have skipped.
        ShownOption::PyVersion(spec) => options.pyversion = Some(spec),
        ShownOption::TrimDoctestFlags => options.trim = DocTestTrim::Trim,
        ShownOption::NoTrimDoctestFlags => options.trim = DocTestTrim::NoTrim,
    }
}

/// Parses an `:options:` value such as `+ELLIPSIS, -NORMALIZE_WHITESPACE`.
///
/// Commas are treated as whitespace, matching Sphinx. Each token must carry a
/// `+`/`-` sign and name a flag `doctest` knows; anything else is reported and
/// skipped rather than aborting the whole block.
fn parse_flag_list(
    kind: DocTestDirectiveKind,
    value: &str,
    diagnostics: &mut Diagnostics,
    span: Option<Span>,
) -> Vec<DocTestFlag> {
    let mut flags = Vec::new();

    for token in value.replace(',', " ").split_whitespace() {
        let (sign, flag_name) = token.split_at(1);
        let enabled = match sign {
            "+" => true,
            "-" => false,
            _ => {
                diagnostics.push(Diagnostic::at(
                    DiagnosticCode::DoctestFlagMissingSign,
                    format!(
                        "{}: doctest option '{token}' must start with '+' or '-'",
                        kind.as_name()
                    ),
                    span,
                ));
                continue;
            }
        };

        match DocTestFlagName::from_doctest_name(flag_name) {
            Some(name) => flags.push(DocTestFlag { name, enabled }),
            None => diagnostics.push(Diagnostic::at(
                DiagnosticCode::DoctestUnknownFlag,
                format!("{}: unknown doctest option '{flag_name}'", kind.as_name()),
                span,
            )),
        }
    }

    flags
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::{Directive, DocTestBlock, DocTestTrim};

    use super::super::block::tests::{block_of, parse};

    #[test]
    fn test_reads_the_hide_flag() {
        // Given
        let body = ["   :hide:", "", "   print(1)"];

        // When
        let (directive, diagnostics) = parse(DocTestDirectiveKind::TestCode, "", &body);

        // Then
        assert!(!block_of(directive).is_rendered());
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_reads_the_skipif_expression() {
        // Given
        let body = ["   :skipif: sys.platform == 'win32'", "", "   import os"];

        // When
        let (directive, _) = parse(DocTestDirectiveKind::TestSetup, "", &body);

        // Then
        assert_eq!(
            block_of(directive).skipif().map(String::as_str),
            Some("sys.platform == 'win32'")
        );
    }

    #[test]
    fn test_reads_the_options_flag_list() {
        // Given
        let body = [
            "   :options: +ELLIPSIS, -NORMALIZE_WHITESPACE",
            "",
            "   >>> f()",
        ];

        // When
        let (directive, diagnostics) = parse(DocTestDirectiveKind::Doctest, "", &body);

        // Then
        match block_of(directive) {
            DocTestBlock::Interactive { flags, .. } => assert_eq!(
                flags,
                vec![
                    DocTestFlag::enable(DocTestFlagName::Ellipsis),
                    DocTestFlag::disable(DocTestFlagName::NormalizeWhitespace),
                ]
            ),
            other => panic!("expected an interactive block, got {other:?}"),
        }
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_reads_a_whitespace_separated_flag_list() {
        // Given — Sphinx treats commas as whitespace.
        let body = ["   :options: +ELLIPSIS +SKIP", "", "   >>> f()"];

        // When
        let (directive, diagnostics) = parse(DocTestDirectiveKind::Doctest, "", &body);

        // Then
        match block_of(directive) {
            DocTestBlock::Interactive { flags, .. } => assert_eq!(flags.len(), 2),
            other => panic!("expected an interactive block, got {other:?}"),
        }
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_reports_a_flag_without_a_sign_and_keeps_the_block() {
        // Given
        let body = ["   :options: ELLIPSIS", "", "   >>> f()"];

        // When
        let (directive, diagnostics) = parse(DocTestDirectiveKind::Doctest, "", &body);

        // Then — reported, but the block survives.
        assert!(matches!(directive, Directive::DocTest(_)));
        assert_eq!(diagnostics.len(), 1);
        assert!(
            diagnostics[0]
                .message
                .contains("must start with '+' or '-'")
        );
    }

    #[test]
    fn test_reports_an_unknown_flag_name_and_keeps_the_block() {
        // Given
        let body = ["   :options: +NOT_A_FLAG", "", "   >>> f()"];

        // When
        let (directive, diagnostics) = parse(DocTestDirectiveKind::Doctest, "", &body);

        // Then
        assert!(matches!(directive, Directive::DocTest(_)));
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("unknown doctest option"));
    }

    #[test]
    fn test_reads_a_pyversion_specifier() {
        // Given
        let body = ["   :pyversion: >= 3.5", "", "   >>> f()"];

        // When
        let (directive, diagnostics) = parse(DocTestDirectiveKind::Doctest, "", &body);

        // Then
        match block_of(directive) {
            DocTestBlock::Interactive { pyversion, .. } => {
                assert_eq!(pyversion.map(|s| s.to_string()), Some(">=3.5".to_string()));
            }
            other => panic!("expected an interactive block, got {other:?}"),
        }
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_reports_an_invalid_pyversion_rather_than_dropping_it_silently() {
        // Given — ignoring this would run a block Sphinx would have skipped.
        let body = ["   :pyversion: 3.5", "", "   >>> f()"];

        // When
        let (directive, diagnostics) = parse(DocTestDirectiveKind::Doctest, "", &body);

        // Then
        assert!(matches!(directive, Directive::DocTest(_)));
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains(":pyversion:"));
    }

    #[test]
    fn test_reads_the_trim_doctest_flags_option() {
        // Given
        let body = ["   :trim-doctest-flags:", "", "   >>> f()"];

        // When
        let (directive, _) = parse(DocTestDirectiveKind::Doctest, "", &body);

        // Then
        match block_of(directive) {
            DocTestBlock::Interactive { trim, .. } => assert_eq!(trim, DocTestTrim::Trim),
            other => panic!("expected an interactive block, got {other:?}"),
        }
    }

    #[test]
    fn test_reads_the_no_trim_doctest_flags_option() {
        // Given
        let body = ["   :no-trim-doctest-flags:", "", "   >>> f()"];

        // When
        let (directive, _) = parse(DocTestDirectiveKind::Doctest, "", &body);

        // Then
        match block_of(directive) {
            DocTestBlock::Interactive { trim, .. } => assert_eq!(trim, DocTestTrim::NoTrim),
            other => panic!("expected an interactive block, got {other:?}"),
        }
    }

    #[test]
    fn test_leaves_trim_unset_when_neither_option_is_given() {
        // Given
        let body = ["   >>> f()"];

        // When
        let (directive, _) = parse(DocTestDirectiveKind::Doctest, "", &body);

        // Then
        match block_of(directive) {
            DocTestBlock::Interactive { trim, .. } => assert_eq!(trim, DocTestTrim::Unset),
            other => panic!("expected an interactive block, got {other:?}"),
        }
    }

    #[test]
    fn test_reports_an_option_the_directive_does_not_accept() {
        // Given — `:options:` is meaningless on testsetup.
        let body = ["   :options: +ELLIPSIS", "", "   import os"];

        // When
        let (directive, diagnostics) = parse(DocTestDirectiveKind::TestSetup, "", &body);

        // Then — reported as an option it does not take, and kept.
        assert!(matches!(directive, Directive::DocTest(_)));
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, DiagnosticCode::DirectiveUnknownOption);
    }

    #[test]
    fn test_reports_an_unknown_option_and_keeps_the_block() {
        // Given
        let body = ["   :bogus: 1", "", "   import os"];

        // When
        let (directive, diagnostics) = parse(DocTestDirectiveKind::TestSetup, "", &body);

        // Then
        assert!(matches!(directive, Directive::DocTest(_)));
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, DiagnosticCode::DirectiveUnknownOption);
    }

    #[test]
    fn test_does_not_treat_a_body_line_after_a_blank_as_an_option() {
        // Given — the option block is contiguous, so this colon line is code.
        let body = ["   :hide:", "", "   d = {}", "   d[1] = 2"];

        // When
        let (directive, _) = parse(DocTestDirectiveKind::TestCode, "", &body);

        // Then
        assert_eq!(block_of(directive).content().body(), "d = {}\nd[1] = 2");
    }
}

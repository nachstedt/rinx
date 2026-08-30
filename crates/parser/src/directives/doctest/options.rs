//! The `sphinx.ext.doctest` family's shared option sub-syntax: scanning the
//! contiguous `:option:` block at the top of a directive's body and resolving
//! each option's value, independent of which directive kind is asking (see
//! [`super`] for the per-kind acceptance rules and directive assembly
//! this scanner feeds into).

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use rusty_sphinx_ast::{
    Diagnostic, DiagnosticCode, DocTestFlag, DocTestFlagName, DocTestTrim, PyVersionSpec, Span,
};

use super::kind::DocTestDirectiveKind;

/// An option name recognized somewhere in the family.
///
/// Kept separate from the per-kind acceptance rules so the shared scanner can
/// recognize every spelling and then report the ones this particular directive
/// does not take, rather than reporting them as unknown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DocTestOptionName {
    Hide,
    Options,
    PyVersion,
    SkipIf,
    TrimDoctestFlags,
    NoTrimDoctestFlags,
}

impl DocTestOptionName {
    /// Every option name, paired with its `:spelling:`.
    pub(super) const ALL: [(&'static str, Self); 6] = [
        (":hide:", Self::Hide),
        (":options:", Self::Options),
        (":pyversion:", Self::PyVersion),
        (":skipif:", Self::SkipIf),
        (":trim-doctest-flags:", Self::TrimDoctestFlags),
        (":no-trim-doctest-flags:", Self::NoTrimDoctestFlags),
    ];

    /// The option's spelling, including the surrounding colons.
    const fn as_spelling(self) -> &'static str {
        match self {
            Self::Hide => ":hide:",
            Self::Options => ":options:",
            Self::PyVersion => ":pyversion:",
            Self::SkipIf => ":skipif:",
            Self::TrimDoctestFlags => ":trim-doctest-flags:",
            Self::NoTrimDoctestFlags => ":no-trim-doctest-flags:",
        }
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

/// Reads the contiguous option block at the start of `body_lines`.
///
/// Returns the options and the index at which content begins. The block ends at
/// the first blank or non-`:` line, per RST's rule that a directive's options
/// are contiguous and immediately follow it — scanning past a blank line would
/// let a body line that happens to start with `:` be swallowed as an option.
pub(super) fn scan_options(
    kind: DocTestDirectiveKind,
    body_lines: &[&str],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> (DocTestOptions, usize) {
    let mut options = DocTestOptions {
        hide: false,
        flags: Vec::new(),
        pyversion: None,
        skipif: None,
        trim: DocTestTrim::Unset,
    };

    let mut index = 0;
    while index < body_lines.len() {
        let line = body_lines[index].trim();
        if line.is_empty() || !line.starts_with(':') {
            break;
        }

        let span = ctx.line_span(index, body_lines[index]);
        match match_option(line) {
            Some((name, value)) => {
                if kind.accepts(name) {
                    apply_option(kind, name, value, &mut options, diagnostics, span);
                } else {
                    diagnostics.push(Diagnostic::at(
                        DiagnosticCode::DoctestOptionNotSupported,
                        format!(
                            "{}: option {} is not supported by this directive",
                            kind.as_name(),
                            name.as_spelling()
                        ),
                        span,
                    ));
                }
            }
            None => diagnostics.push(Diagnostic::at(
                DiagnosticCode::DoctestUnknownOption,
                format!("{}: unknown option '{line}'", kind.as_name()),
                span,
            )),
        }
        index += 1;
    }

    (options, index)
}

/// Matches an option line against the known spellings, returning the name and
/// the text after it.
fn match_option(line: &str) -> Option<(DocTestOptionName, &str)> {
    DocTestOptionName::ALL
        .iter()
        .find_map(|(spelling, name)| line.strip_prefix(spelling).map(|rest| (*name, rest.trim())))
}

/// Records one recognized, accepted option.
fn apply_option(
    kind: DocTestDirectiveKind,
    name: DocTestOptionName,
    value: &str,
    options: &mut DocTestOptions,
    diagnostics: &mut Diagnostics,
    span: Option<Span>,
) {
    match name {
        DocTestOptionName::Hide => options.hide = true,
        DocTestOptionName::Options => {
            options.flags = parse_flag_list(kind, value, diagnostics, span);
        }
        DocTestOptionName::PyVersion => match PyVersionSpec::parse(value) {
            Ok(spec) => options.pyversion = Some(spec),
            // Reported, not silently dropped: ignoring a `:pyversion:` would
            // run a block that Sphinx would have skipped.
            Err(message) => diagnostics.push(Diagnostic::at(
                DiagnosticCode::DoctestPyVersionInvalid,
                format!("{}: invalid :pyversion: value — {message}", kind.as_name()),
                span,
            )),
        },
        DocTestOptionName::SkipIf => options.skipif = Some(value.to_string()),
        DocTestOptionName::TrimDoctestFlags => options.trim = DocTestTrim::Trim,
        DocTestOptionName::NoTrimDoctestFlags => options.trim = DocTestTrim::NoTrim,
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
    use rusty_sphinx_ast::{Directive, DocTestBlock, DocTestTrim};

    use super::super::block::tests::{block_of, parse};

    #[test]
    fn test_option_spellings_match_the_lookup_table() {
        // Given — ALL and as_spelling are maintained by hand.
        for (spelling, option) in DocTestOptionName::ALL {
            // When / Then
            assert_eq!(option.as_spelling(), spelling);
        }
    }

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
        assert!(diagnostics[0].message.contains("invalid :pyversion:"));
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

        // Then — reported as unsupported-here, not as unknown, and kept.
        assert!(matches!(directive, Directive::DocTest(_)));
        assert_eq!(diagnostics.len(), 1);
        assert!(
            diagnostics[0]
                .message
                .contains("not supported by this directive")
        );
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
        assert!(diagnostics[0].message.contains("unknown option"));
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

    #[test]
    fn test_match_option_returns_the_value_after_the_spelling() {
        // Given
        let line = ":skipif: True";

        // When
        let matched = match_option(line);

        // Then
        assert_eq!(matched, Some((DocTestOptionName::SkipIf, "True")));
    }

    #[test]
    fn test_match_option_returns_none_for_an_unrelated_field() {
        // Given
        let line = ":bogus: 1";

        // When
        let matched = match_option(line);

        // Then
        assert_eq!(matched, None);
    }
}

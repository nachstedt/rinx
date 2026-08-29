//! Parsing for the `sphinx.ext.doctest` directive family.
//!
//! All five directives share one option sub-syntax, so they share one scanner
//! ([`options::scan_options`]); what differs is which options
//! each accepts, which is stated once in [`DocTestDirectiveKind::accepts`] and
//! enforced there.
//!
//! # Unrecognized options do not degrade the directive
//!
//! An unknown or misplaced option produces a diagnostic and the block is still
//! built. Falling back to [`Directive::Unknown`] would make "not implemented"
//! indistinguishable from "implemented, with a gap" in the benchmark's
//! unsupported-directive tally, and would drop code the author wrote. Only
//! genuinely unusable input (an empty body) degrades.

use rusty_sphinx_ast::{
    Directive, DocTestBlock, DocTestGroupSelector, HashedContent, NonEmptyVector,
};

use super::blocks::strip_common_indent;

mod options;
use options::{DocTestOptionName, DocTestOptions, scan_options};

/// Which of the five directives is being parsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DocTestDirectiveKind {
    Doctest,
    TestCode,
    TestOutput,
    TestSetup,
    TestCleanup,
}

impl DocTestDirectiveKind {
    /// Resolves a directive name, or `None` if it is not one of the five.
    pub(super) fn from_name(name: &str) -> Option<Self> {
        match name {
            "doctest" => Some(Self::Doctest),
            "testcode" => Some(Self::TestCode),
            "testoutput" => Some(Self::TestOutput),
            "testsetup" => Some(Self::TestSetup),
            "testcleanup" => Some(Self::TestCleanup),
            _ => None,
        }
    }

    /// The directive's name, for diagnostics.
    pub(super) const fn as_name(self) -> &'static str {
        match self {
            Self::Doctest => "doctest",
            Self::TestCode => "testcode",
            Self::TestOutput => "testoutput",
            Self::TestSetup => "testsetup",
            Self::TestCleanup => "testcleanup",
        }
    }

    /// Whether this directive accepts `option`, mirroring Sphinx's per-class
    /// `option_spec`.
    ///
    /// `testsetup`/`testcleanup` take only `:skipif:` — they never render, so
    /// `:hide:` would say nothing, and they are never compared against expected
    /// output, so `:options:` would too. `testcode` has no `:options:` because
    /// it states no output of its own; its companion `testoutput` carries them.
    pub(super) const fn accepts(self, option: DocTestOptionName) -> bool {
        match self {
            Self::Doctest | Self::TestOutput => true,
            Self::TestCode => !matches!(option, DocTestOptionName::Options),
            Self::TestSetup | Self::TestCleanup => matches!(option, DocTestOptionName::SkipIf),
        }
    }
}

/// Parses one directive of the family into a [`Directive`].
///
/// Returns [`Directive::Unknown`] only when the body is empty — a doctest block
/// with no code is unusable, and Sphinx warns about it too.
pub(super) fn parse_doctest_directive(
    kind: DocTestDirectiveKind,
    argument: &str,
    body_lines: &[&str],
    diagnostics: &mut Vec<String>,
) -> Directive {
    let (options, content_start) = scan_options(kind, body_lines, diagnostics);
    let content_lines = skip_leading_blank_lines(&body_lines[content_start..]);
    let content_text = strip_common_indent(content_lines);

    if content_text.trim().is_empty() {
        diagnostics.push(format!("{}: no code in block", kind.as_name()));
        return Directive::Unknown {
            name: kind.as_name().to_string(),
            argument: argument.to_string(),
            body: content_text,
        };
    }

    let groups = parse_group_argument(kind, argument, diagnostics);
    let content = HashedContent::new(content_text);

    Directive::DocTest(build_block(kind, groups, content, options))
}

/// Assembles the variant for `kind`, handing each one only the options it
/// accepts.
fn build_block(
    kind: DocTestDirectiveKind,
    groups: NonEmptyVector<DocTestGroupSelector>,
    content: HashedContent,
    options: DocTestOptions,
) -> DocTestBlock {
    let DocTestOptions {
        hide,
        flags,
        pyversion,
        skipif,
        trim,
    } = options;

    match kind {
        DocTestDirectiveKind::Doctest => DocTestBlock::Interactive {
            groups,
            content,
            hide,
            flags,
            pyversion,
            skipif,
            trim,
        },
        DocTestDirectiveKind::TestCode => DocTestBlock::Code {
            groups,
            content,
            hide,
            pyversion,
            skipif,
            trim,
        },
        DocTestDirectiveKind::TestOutput => DocTestBlock::Output {
            groups,
            content,
            hide,
            flags,
            pyversion,
            skipif,
            trim,
        },
        DocTestDirectiveKind::TestSetup => DocTestBlock::Setup {
            groups,
            content,
            skipif,
        },
        DocTestDirectiveKind::TestCleanup => DocTestBlock::Cleanup {
            groups,
            content,
            skipif,
        },
    }
}

/// Splits the directive argument into group selectors.
///
/// The argument is a *comma-separated list*, so `.. testcode:: g1, g2` joins
/// both groups. An absent argument means the `default` group.
fn parse_group_argument(
    kind: DocTestDirectiveKind,
    argument: &str,
    diagnostics: &mut Vec<String>,
) -> NonEmptyVector<DocTestGroupSelector> {
    if argument.trim().is_empty() {
        return NonEmptyVector::single(DocTestGroupSelector::new(""));
    }

    let selectors: Vec<DocTestGroupSelector> = argument
        .split(',')
        .map(|token| {
            if token.trim().is_empty() {
                diagnostics.push(format!(
                    "{}: empty group name in argument '{argument}', using the default group",
                    kind.as_name()
                ));
            }
            DocTestGroupSelector::new(token)
        })
        .collect();

    NonEmptyVector::try_from(selectors)
        .unwrap_or_else(|_| NonEmptyVector::single(DocTestGroupSelector::new("")))
}

/// Drops blank lines separating the option block from the content.
pub(super) fn skip_leading_blank_lines<'a, 'b>(lines: &'a [&'b str]) -> &'a [&'b str] {
    let start = lines
        .iter()
        .position(|line| !line.trim().is_empty())
        .unwrap_or(lines.len());
    &lines[start..]
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Parses `body` under `kind`, returning the directive and any diagnostics.
    ///
    /// `pub(crate)` because [`options`]'s own test module
    /// also drives directive parsing through this same helper.
    pub(crate) fn parse(
        kind: DocTestDirectiveKind,
        argument: &str,
        body: &[&str],
    ) -> (Directive, Vec<String>) {
        let mut diagnostics = Vec::new();
        let directive = parse_doctest_directive(kind, argument, body, &mut diagnostics);
        (directive, diagnostics)
    }

    /// Unwraps the [`DocTestBlock`] a successful parse produced.
    pub(crate) fn block_of(directive: Directive) -> DocTestBlock {
        match directive {
            Directive::DocTest(block) => block,
            other => panic!("expected a doctest block, got {other:?}"),
        }
    }

    #[test]
    fn test_from_name_resolves_every_directive_in_the_family() {
        // Given
        let cases = [
            ("doctest", DocTestDirectiveKind::Doctest),
            ("testcode", DocTestDirectiveKind::TestCode),
            ("testoutput", DocTestDirectiveKind::TestOutput),
            ("testsetup", DocTestDirectiveKind::TestSetup),
            ("testcleanup", DocTestDirectiveKind::TestCleanup),
        ];

        for (name, expected) in cases {
            // When / Then
            assert_eq!(DocTestDirectiveKind::from_name(name), Some(expected));
        }
    }

    #[test]
    fn test_from_name_rejects_an_unrelated_directive() {
        // Given / When / Then
        assert_eq!(DocTestDirectiveKind::from_name("note"), None);
    }

    #[test]
    fn test_from_name_and_as_name_roundtrip_for_every_kind() {
        // Given — two hand-maintained mappings that must stay in step.
        let kinds = [
            DocTestDirectiveKind::Doctest,
            DocTestDirectiveKind::TestCode,
            DocTestDirectiveKind::TestOutput,
            DocTestDirectiveKind::TestSetup,
            DocTestDirectiveKind::TestCleanup,
        ];

        for kind in kinds {
            // When / Then
            assert_eq!(DocTestDirectiveKind::from_name(kind.as_name()), Some(kind));
        }
    }

    #[test]
    fn test_setup_and_cleanup_accept_only_skipif() {
        // Given
        for kind in [
            DocTestDirectiveKind::TestSetup,
            DocTestDirectiveKind::TestCleanup,
        ] {
            for (_, option) in DocTestOptionName::ALL {
                // When
                let accepted = kind.accepts(option);

                // Then
                assert_eq!(accepted, option == DocTestOptionName::SkipIf, "{kind:?}");
            }
        }
    }

    #[test]
    fn test_testcode_accepts_everything_except_options() {
        // Given
        for (_, option) in DocTestOptionName::ALL {
            // When
            let accepted = DocTestDirectiveKind::TestCode.accepts(option);

            // Then
            assert_eq!(accepted, option != DocTestOptionName::Options);
        }
    }

    #[test]
    fn test_doctest_and_testoutput_accept_every_option() {
        // Given
        for kind in [
            DocTestDirectiveKind::Doctest,
            DocTestDirectiveKind::TestOutput,
        ] {
            for (_, option) in DocTestOptionName::ALL {
                // When / Then
                assert!(kind.accepts(option), "{kind:?} should accept {option:?}");
            }
        }
    }

    #[test]
    fn test_parses_a_minimal_doctest_block() {
        // Given
        let body = ["   >>> 1 + 1", "   2"];

        // When
        let (directive, diagnostics) = parse(DocTestDirectiveKind::Doctest, "", &body);

        // Then
        let block = block_of(directive);
        assert_eq!(block.content().body(), ">>> 1 + 1\n2");
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_preserves_relative_indentation_in_the_body() {
        // Given — Python's meaning depends on it.
        let body = ["   def f():", "       return 1"];

        // When
        let (directive, _) = parse(DocTestDirectiveKind::TestCode, "", &body);

        // Then
        assert_eq!(
            block_of(directive).content().body(),
            "def f():\n    return 1"
        );
    }

    #[test]
    fn test_defaults_to_the_default_group_without_an_argument() {
        // Given
        let body = ["   >>> 1"];

        // When
        let (directive, _) = parse(DocTestDirectiveKind::Doctest, "", &body);

        // Then
        assert_eq!(
            block_of(directive).groups().as_slice(),
            &[DocTestGroupSelector::new("")]
        );
    }

    #[test]
    fn test_splits_a_comma_separated_group_argument() {
        // Given — the argument is a list, not a single name.
        let body = ["   print(1)"];

        // When
        let (directive, diagnostics) = parse(DocTestDirectiveKind::TestCode, "g1, g2", &body);

        // Then
        assert_eq!(
            block_of(directive).groups().as_slice(),
            &[
                DocTestGroupSelector::new("g1"),
                DocTestGroupSelector::new("g2")
            ]
        );
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_resolves_a_star_argument_to_all_groups() {
        // Given
        let body = ["   import os"];

        // When
        let (directive, _) = parse(DocTestDirectiveKind::TestSetup, "*", &body);

        // Then
        assert_eq!(
            block_of(directive).groups().as_slice(),
            &[DocTestGroupSelector::AllGroups]
        );
    }

    #[test]
    fn test_reports_an_empty_group_token() {
        // Given
        let body = ["   print(1)"];

        // When
        let (_, diagnostics) = parse(DocTestDirectiveKind::TestCode, "g1,,g2", &body);

        // Then
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].contains("empty group name"));
    }

    #[test]
    fn test_degrades_to_unknown_when_the_body_is_empty() {
        // Given — a block with options but no code is unusable.
        let body = ["   :hide:"];

        // When
        let (directive, diagnostics) = parse(DocTestDirectiveKind::TestCode, "", &body);

        // Then
        assert!(matches!(directive, Directive::Unknown { .. }));
        assert!(diagnostics.iter().any(|d| d.contains("no code in block")));
    }

    #[test]
    fn test_degrades_to_unknown_for_a_body_with_no_lines_at_all() {
        // Given
        let body: [&str; 0] = [];

        // When
        let (directive, _) = parse(DocTestDirectiveKind::Doctest, "", &body);

        // Then
        assert!(matches!(directive, Directive::Unknown { .. }));
    }

    #[test]
    fn test_diagnostics_are_prefixed_with_the_directive_name() {
        // Given — the benchmark buckets diagnostics by the text before the
        // first colon, so this is what makes them attributable per directive.
        let body = ["   :bogus:", "", "   import os"];

        // When
        let (_, diagnostics) = parse(DocTestDirectiveKind::TestCleanup, "", &body);

        // Then
        assert!(diagnostics[0].starts_with("testcleanup:"));
    }

    #[test]
    fn test_skip_leading_blank_lines_drops_only_the_leading_run() {
        // Given
        let lines = ["", "  ", "code", "", "more"];

        // When
        let remaining = skip_leading_blank_lines(&lines);

        // Then
        assert_eq!(remaining, &["code", "", "more"]);
    }

    #[test]
    fn test_skip_leading_blank_lines_handles_an_all_blank_slice() {
        // Given
        let lines = ["", "   "];

        // When
        let remaining = skip_leading_blank_lines(&lines);

        // Then
        assert!(remaining.is_empty());
    }
}

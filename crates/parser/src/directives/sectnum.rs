use crate::directives::invocation::{DirectiveInvocation, ParseState, single};
use crate::directives::spec::{Arguments, DirectiveShape, DirectiveSpec};
use std::num::NonZeroU32;

use rinx_ast::{Directive, SectnumOptions};

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;

use super::option_value::{DepthLimit, NonEmptyText};
use super::options::{DirectiveOptions, OptionLine, classify_options, directive_options};

const DIRECTIVE: &str = "sectnum";

directive_options! {
    /// The options `.. sectnum::` takes: docutils' four.
    enum SectnumOption {
        /// `:depth:`.
        Depth(DepthLimit) => (["depth"], SectnumDepthInvalid, "How many section levels are numbered."),
        /// `:start:`.
        Start(NonZeroU32) => (["start"], SectnumStartInvalid, "The number the first section gets."),
        /// `:prefix:`.
        Prefix(NonEmptyText) => (["prefix"], "Text written before each number."),
        /// `:suffix:`.
        Suffix(NonEmptyText) => (["suffix"], "Text written after each number."),
    }
}

/// Reads the four options `.. sectnum::` accepts, returning them with the
/// lines it did not recognize, in source order.
fn parse_sectnum_options(
    option_lines: &[OptionLine],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> SectnumOptions {
    let mut options = SectnumOptions::default();

    for (option, _) in classify_options::<SectnumOption>(option_lines, DIRECTIVE, diagnostics, ctx)
    {
        match option {
            SectnumOption::Depth(depth) => options.depth = depth.0,
            SectnumOption::Start(start) => options.start = Some(start),
            SectnumOption::Prefix(prefix) => options.prefix = prefix.0,
            SectnumOption::Suffix(suffix) => options.suffix = suffix.0,
        }
    }

    options
}

/// This directive's entry in the table of directives this build implements.
///
/// Docutils' own alias, `.. section-numbering::`, shares this directive's
/// implementation completely — unlike `.. code-block::`/`.. code::`, the two
/// spellings agree on every option, so there is no need to record which one an
/// author wrote.
pub(in crate::directives) const SPEC: DirectiveSpec = DirectiveSpec {
    names: &["sectnum", "section-numbering"],
    doc: "Numbers this document's sections.",
    shape: DirectiveShape {
        arguments: Arguments::NONE,
        takes_options: true,
        takes_content: false,
        content_hint: None,
    },
    options: <SectnumOption as DirectiveOptions>::GROUPS,
    accepts_further: false,
};

/// Interprets `.. sectnum::` and `.. section-numbering::`.
pub(in crate::directives) fn interpret_sectnum(
    input: &DirectiveInvocation<'_, '_>,
    state: &mut ParseState<'_>,
) -> Vec<rinx_ast::Node> {
    single(parse_sectnum(input, state.diagnostics))
}

/// Parses a `.. sectnum::`/`.. section-numbering::` into a
/// [`Directive::Sectnum`]. docutils declares no argument and no content for
/// it, so text after the `::` is reported by the reader, as content it takes
/// none of.
pub(super) fn parse_sectnum(
    invocation: &DirectiveInvocation<'_, '_>,
    diagnostics: &mut Diagnostics,
) -> Directive {
    let ctx = &invocation.ctx;
    let options = parse_sectnum_options(&invocation.options, diagnostics, ctx);

    Directive::Sectnum(options)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use rinx_ast::{Diagnostic, DiagnosticCode};
    use rinx_ast::{Domain, Node};
    use std::num::NonZeroUsize;

    /// Parses a directive body directly, bypassing the block dispatcher, for
    /// tests about `parse_sectnum` itself rather than about recognition.
    /// Takes no argument — see [`parse_body_with_argument`] for that.
    fn parse_body(body_lines: &[&str]) -> (SectnumOptions, Diagnostics) {
        parse_body_with_argument("", body_lines)
    }

    fn parse_body_with_argument(
        argument: &str,
        body_lines: &[&str],
    ) -> (SectnumOptions, Diagnostics) {
        let mut diagnostics = Diagnostics::default();
        let ctx = ParseCtx::with_domain(Domain::Py);
        let invocation = crate::directives::reader::test_invocation(
            "sectnum",
            SPEC.shape,
            argument,
            body_lines,
            &mut diagnostics,
            &ctx,
        );
        let directive = parse_sectnum(&invocation, &mut diagnostics);
        match directive {
            Directive::Sectnum(options) => (options, diagnostics),
            other => panic!("Expected Sectnum, got {other:?}"),
        }
    }

    /// Parses a whole document and returns its single `.. sectnum::`, for
    /// tests that need the real span origin the dispatcher sets up.
    fn parse_document(input: &str) -> (SectnumOptions, Vec<Diagnostic>) {
        let doc = parse("test.rst", input);
        let options = doc
            .nodes
            .iter()
            .find_map(|node| match node {
                Node::Directive(Directive::Sectnum(options)) => Some(options.clone()),
                _ => None,
            })
            .expect("document should contain a sectnum directive");
        (options, doc.diagnostics)
    }

    #[test]
    fn test_both_spellings_parse_as_sectnum() {
        // Given / When / Then
        for name in ["sectnum", "section-numbering"] {
            let doc = crate::parse("index.rst", &format!(".. {name}::\n"));
            assert!(
                matches!(doc.nodes[..], [Node::Directive(Directive::Sectnum(_))]),
                "`{name}` parsed as {:?}",
                doc.nodes
            );
        }
    }

    #[test]
    fn test_parse_sectnum_with_no_options_is_all_defaults() {
        // Given / When
        let (options, diagnostics) = parse_body(&[]);

        // Then
        assert_eq!(options, SectnumOptions::default());
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_parse_sectnum_diagnoses_a_stray_argument() {
        // Given — docutils declares zero argument slots for this directive
        // and really does reject one, as a directive error, rather than
        // silently dropping it.
        let (options, diagnostics) = parse_body_with_argument("oops", &[]);

        // Then — the argument is diagnosed, not applied to anything.
        assert_eq!(options, SectnumOptions::default());
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics.entries()[0].code,
            DiagnosticCode::DirectiveContentNotPermitted
        );
        assert!(diagnostics.entries()[0].message.contains("oops"));
    }

    #[test]
    fn test_parse_sectnum_reads_depth() {
        // Given
        let body = [":depth: 2"];

        // When
        let (options, diagnostics) = parse_body(&body);

        // Then
        assert!(diagnostics.is_empty());
        assert_eq!(options.depth, NonZeroUsize::new(2));
    }

    #[test]
    fn test_parse_sectnum_treats_a_zero_depth_as_unlimited() {
        // Given
        let body = [":depth: 0"];

        // When
        let (options, diagnostics) = parse_body(&body);

        // Then
        assert_eq!(options.depth, None);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_parse_sectnum_diagnoses_a_non_numeric_depth() {
        // Given
        let body = [":depth: deep"];

        // When
        let (options, diagnostics) = parse_body(&body);

        // Then
        assert_eq!(options.depth, None);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics.entries()[0].code,
            DiagnosticCode::SectnumDepthInvalid
        );
    }

    #[test]
    fn test_parse_sectnum_reads_start() {
        // Given
        let body = [":start: 5"];

        // When
        let (options, diagnostics) = parse_body(&body);

        // Then
        assert!(diagnostics.is_empty());
        assert_eq!(options.start, NonZeroU32::new(5));
    }

    #[test]
    fn test_parse_sectnum_diagnoses_a_zero_start() {
        // Given — unlike `:depth:`, `0` has no sensible number to display.
        let body = [":start: 0"];

        // When
        let (options, diagnostics) = parse_body(&body);

        // Then
        assert_eq!(options.start, None);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics.entries()[0].code,
            DiagnosticCode::SectnumStartInvalid
        );
    }

    #[test]
    fn test_parse_sectnum_diagnoses_a_non_numeric_start() {
        // Given
        let body = [":start: five"];

        // When
        let (options, diagnostics) = parse_body(&body);

        // Then
        assert_eq!(options.start, None);
        assert_eq!(
            diagnostics.entries()[0].code,
            DiagnosticCode::SectnumStartInvalid
        );
    }

    #[test]
    fn test_parse_sectnum_reads_prefix_and_suffix() {
        // Given
        let body = [":prefix: Appendix ", ":suffix: ."];

        // When
        let (options, diagnostics) = parse_body(&body);

        // Then
        assert!(diagnostics.is_empty());
        assert_eq!(options.prefix, "Appendix");
        assert_eq!(options.suffix, ".");
    }

    #[test]
    fn test_parse_sectnum_diagnoses_an_empty_prefix() {
        // Given
        let body = [":prefix:"];

        // When
        let (options, diagnostics) = parse_body(&body);

        // Then
        assert_eq!(options.prefix, "");
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics.entries()[0].code,
            DiagnosticCode::DirectiveEmptyOptionValue
        );
    }

    #[test]
    fn test_parse_sectnum_diagnoses_an_empty_suffix() {
        // Given
        let body = [":suffix:"];

        // When
        let (options, diagnostics) = parse_body(&body);

        // Then
        assert_eq!(options.suffix, "");
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics.entries()[0].code,
            DiagnosticCode::DirectiveEmptyOptionValue
        );
    }

    #[test]
    fn test_parse_sectnum_diagnoses_a_stray_argument_through_the_full_parser() {
        // Given
        let input = ".. sectnum:: oops\n";

        // When
        let (_, diagnostics) = parse_document(input);

        // Then
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].code,
            DiagnosticCode::DirectiveContentNotPermitted
        );
        assert!(diagnostics[0].message.contains("oops"));
    }

    #[test]
    fn test_parse_sectnum_diagnoses_an_unknown_option_under_its_own_code() {
        // Given
        let input = ".. sectnum::\n   :invalid_opt:\n";

        // When
        let (_, diagnostics) = parse_document(input);

        // Then
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, DiagnosticCode::DirectiveUnknownOption);
        assert!(diagnostics[0].message.contains(":invalid_opt:"));
    }

    #[test]
    fn test_parse_sectnum_reads_every_option_together() {
        // Given
        let body = [":depth: 2", ":start: 3", ":prefix: Sec ", ":suffix: ."];

        // When
        let (options, diagnostics) = parse_body(&body);

        // Then
        assert!(diagnostics.is_empty());
        assert_eq!(options.depth, NonZeroUsize::new(2));
        assert_eq!(options.start, NonZeroU32::new(3));
        assert_eq!(options.prefix, "Sec");
        assert_eq!(options.suffix, ".");
    }

    #[test]
    fn test_both_spellings_parse_to_the_same_directive() {
        // Given
        let sphinx_spelling = parse("test.rst", ".. sectnum::\n   :depth: 1\n");
        let docutils_spelling = parse("test.rst", ".. section-numbering::\n   :depth: 1\n");

        // When
        let find = |doc: &rinx_ast::Document| {
            doc.nodes
                .iter()
                .find_map(|node| match node {
                    Node::Directive(directive @ Directive::Sectnum(_)) => Some(directive.clone()),
                    _ => None,
                })
                .expect("should contain a sectnum directive")
        };

        // Then
        assert_eq!(find(&sphinx_spelling), find(&docutils_spelling));
    }
}

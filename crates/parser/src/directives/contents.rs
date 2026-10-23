use crate::directives::invocation::{DirectiveInvocation, ParseState, single};
use crate::directives::spec::{Arguments, DirectiveShape, DirectiveSpec};
use rinx_ast::{Contents, ContentsBacklinks, ContentsOptions, Directive, TargetName};

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;

use super::option_value::{ChoiceValue, Classes, DepthLimit};
use super::options::{DirectiveOptions, OptionLine, classify_options, directive_options};

const DIRECTIVE: &str = "contents";

directive_options! {
    /// The options `.. contents::` takes: docutils' four, and a `:name:`.
    enum ContentsOption {
        /// `:depth:`.
        Depth(DepthLimit) => (["depth"], ContentsDepthInvalid, "How many section levels the table lists."),
        /// `:local:`.
        Local => (["local"], Flag, "Lists only the sections below the directive's own."),
        /// `:backlinks:`.
        Backlinks(ContentsBacklinks) => (["backlinks"], ContentsBacklinksInvalid, "Where each section title links back to."),
        /// `:class:`.
        Class(Classes) => (["class"], "Classes for the table."),
        /// `:name:`.
        Name(TargetName) => (["name"], "A label to reference the table by."),
    }
}

/// Reads the five options `.. contents::` accepts, reporting every other one.
/// A `:depth:` of zero or less means unlimited, as [`DepthLimit`] says.
fn parse_contents_options(
    option_lines: &[OptionLine],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> ContentsOptions {
    let mut options = ContentsOptions::default();

    for (option, _) in classify_options::<ContentsOption>(option_lines, DIRECTIVE, diagnostics, ctx)
    {
        match option {
            ContentsOption::Depth(depth) => options.depth = depth.0,
            ContentsOption::Local => options.local = true,
            ContentsOption::Backlinks(backlinks) => options.backlinks = backlinks,
            ContentsOption::Class(classes) => options.classes = classes.0,
            ContentsOption::Name(name) => options.name = Some(name),
        }
    }

    options
}

/// A `:backlinks:`: `entry`, `top` or `none`. Any other value is reported and
/// the default kept (`entry`, docutils'), rather than silently disabling
/// backlinks the author never asked to turn off.
impl ChoiceValue for ContentsBacklinks {
    fn choices() -> Vec<&'static str> {
        [Self::Entry, Self::Top, Self::Off]
            .into_iter()
            .map(Self::option_value)
            .collect()
    }

    fn from_choice(written: &str) -> Option<Self> {
        Self::from_option_value(written)
    }
}

/// This directive's entry in the table of directives this build implements.
pub(in crate::directives) const SPEC: DirectiveSpec = DirectiveSpec {
    names: &["contents"],
    doc: "A table of contents of this document's sections.",
    shape: DirectiveShape {
        arguments: Arguments::OPTIONAL_TEXT,
        takes_options: true,
        takes_content: false,
        content_hint: None,
    },
    options: <ContentsOption as DirectiveOptions>::GROUPS,
    accepts_further: false,
};

/// Interprets `.. contents::`.
pub(in crate::directives) fn interpret_contents(
    input: &DirectiveInvocation<'_, '_>,
    state: &mut ParseState<'_>,
) -> Vec<rinx_ast::Node> {
    single(parse_contents(input, state.diagnostics))
}

/// Parses a `.. contents::` body into a [`Directive::Contents`].
///
/// Its argument is the directive's title, stored only when non-empty: an
/// absent title means "use the default", applied at render time rather than
/// baked in here.
pub(super) fn parse_contents(
    invocation: &DirectiveInvocation<'_, '_>,
    diagnostics: &mut Diagnostics,
) -> Directive {
    let ctx = &invocation.ctx;
    let options = parse_contents_options(&invocation.options, diagnostics, ctx);

    let title = invocation.argument(0).trim();
    Directive::Contents(Contents {
        title: (!title.is_empty()).then(|| title.to_string()),
        options,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use rinx_ast::{Diagnostic, DiagnosticCode};
    use rinx_ast::{Domain, Node};

    /// Parses a directive body directly, bypassing the block dispatcher, for
    /// tests about `parse_contents` itself rather than about recognition.
    fn parse_body(argument: &str, body_lines: &[&str]) -> (Contents, Diagnostics) {
        let mut diagnostics = Diagnostics::default();
        let ctx = ParseCtx::with_domain(Domain::Py);
        let invocation = crate::directives::reader::test_invocation(
            "contents",
            SPEC.shape,
            argument,
            body_lines,
            &mut diagnostics,
            &ctx,
        );
        let directive = parse_contents(&invocation, &mut diagnostics);
        match directive {
            Directive::Contents(contents) => (contents, diagnostics),
            other => panic!("Expected Contents, got {other:?}"),
        }
    }

    /// Parses a whole document and returns its single `.. contents::`, for
    /// tests that need the real span origin the dispatcher sets up.
    fn parse_document(input: &str) -> (Contents, Vec<Diagnostic>) {
        let doc = parse("test.rst", input);
        let contents = doc
            .nodes
            .iter()
            .find_map(|node| match node {
                Node::Directive(Directive::Contents(contents)) => Some(contents.clone()),
                _ => None,
            })
            .expect("document should contain a contents directive");
        (contents, doc.diagnostics)
    }

    #[test]
    fn test_parse_contents_with_no_argument_has_no_title() {
        // Given / When
        let (contents, diagnostics) = parse_body("", &[]);

        // Then
        assert_eq!(contents.title, None);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_parse_contents_reads_an_explicit_title() {
        // Given / When
        let (contents, _) = parse_body("Overview", &[]);

        // Then
        assert_eq!(contents.title.as_deref(), Some("Overview"));
    }

    #[test]
    fn test_parse_contents_trims_the_title() {
        // Given / When
        let (contents, _) = parse_body("  Overview  ", &[]);

        // Then
        assert_eq!(contents.title.as_deref(), Some("Overview"));
    }

    #[test]
    fn test_parse_contents_reads_depth() {
        // Given
        let body = [":depth: 2"];

        // When
        let (contents, diagnostics) = parse_body("", &body);

        // Then
        assert!(diagnostics.is_empty());
        assert_eq!(contents.options.depth, std::num::NonZeroUsize::new(2));
    }

    #[test]
    fn test_parse_contents_treats_a_zero_depth_as_unlimited() {
        // Given
        let body = [":depth: 0"];

        // When
        let (contents, diagnostics) = parse_body("", &body);

        // Then
        assert_eq!(contents.options.depth, None);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_parse_contents_diagnoses_a_non_numeric_depth() {
        // Given
        let body = [":depth: deep"];

        // When
        let (contents, diagnostics) = parse_body("", &body);

        // Then
        assert_eq!(contents.options.depth, None);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics.entries()[0].code,
            DiagnosticCode::ContentsDepthInvalid
        );
    }

    #[test]
    fn test_parse_contents_reads_the_local_flag() {
        // Given
        let body = [":local:"];

        // When
        let (contents, diagnostics) = parse_body("", &body);

        // Then
        assert!(contents.options.local);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_parse_contents_defaults_local_to_false() {
        // Given / When
        let (contents, _) = parse_body("", &[]);

        // Then
        assert!(!contents.options.local);
    }

    #[test]
    fn test_parse_contents_reads_each_backlinks_value() {
        for (value, expected) in [
            ("entry", ContentsBacklinks::Entry),
            ("top", ContentsBacklinks::Top),
            ("none", ContentsBacklinks::Off),
        ] {
            // Given
            let body = [format!(":backlinks: {value}")];
            let body_refs: Vec<&str> = body.iter().map(String::as_str).collect();

            // When
            let (contents, diagnostics) = parse_body("", &body_refs);

            // Then
            assert!(diagnostics.is_empty(), "{value}");
            assert_eq!(contents.options.backlinks, expected, "{value}");
        }
    }

    #[test]
    fn test_parse_contents_defaults_backlinks_to_entry() {
        // Given / When
        let (contents, _) = parse_body("", &[]);

        // Then
        assert_eq!(contents.options.backlinks, ContentsBacklinks::Entry);
    }

    #[test]
    fn test_parse_contents_diagnoses_an_invalid_backlinks_value() {
        // Given
        let body = [":backlinks: sideways"];

        // When
        let (contents, diagnostics) = parse_body("", &body);

        // Then — the default is kept rather than silently disabling backlinks.
        assert_eq!(contents.options.backlinks, ContentsBacklinks::Entry);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics.entries()[0].code,
            DiagnosticCode::ContentsBacklinksInvalid
        );
    }

    #[test]
    fn test_parse_contents_reads_class() {
        // Given
        let body = [":class: wide narrow"];

        // When
        let (contents, _) = parse_body("", &body);

        // Then
        assert_eq!(contents.options.classes, vec!["wide", "narrow"]);
    }

    #[test]
    fn test_parse_contents_reads_the_name_as_a_target() {
        // Given
        let body = [":name: main-contents"];

        // When
        let (contents, diagnostics) = parse_body("", &body);

        // Then
        assert_eq!(
            contents.options.name,
            Some(TargetName::new("main-contents"))
        );
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_parse_contents_diagnoses_an_empty_name() {
        // Given
        let body = [":name:"];

        // When
        let (contents, diagnostics) = parse_body("", &body);

        // Then
        assert_eq!(contents.options.name, None);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics.entries()[0].code,
            DiagnosticCode::DirectiveEmptyOptionValue
        );
    }

    #[test]
    fn test_parse_contents_diagnoses_an_unknown_option_under_its_own_code() {
        // Given
        let input = ".. contents::\n   :invalid_opt:\n";

        // When
        let (_, diagnostics) = parse_document(input);

        // Then
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, DiagnosticCode::DirectiveUnknownOption);
        assert!(diagnostics[0].message.contains(":invalid_opt:"));
    }

    #[test]
    fn test_parse_contents_reads_every_option_together() {
        // Given
        let body = [
            ":depth: 2",
            ":local:",
            ":backlinks: top",
            ":class: wide",
            ":name: toc",
        ];

        // When
        let (contents, diagnostics) = parse_body("My Contents", &body);

        // Then
        assert!(diagnostics.is_empty());
        assert_eq!(contents.title.as_deref(), Some("My Contents"));
        assert_eq!(contents.options.depth, std::num::NonZeroUsize::new(2));
        assert!(contents.options.local);
        assert_eq!(contents.options.backlinks, ContentsBacklinks::Top);
        assert_eq!(contents.options.classes, vec!["wide"]);
        assert_eq!(contents.options.name, Some(TargetName::new("toc")));
    }
}

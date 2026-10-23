use crate::directives::invocation::{DirectiveInvocation, ParseState, single};
use crate::directives::spec::{Arguments, DirectiveShape, DirectiveSpec};
use std::num::NonZeroUsize;

use rinx_ast::{
    Directive, NumberedDepth, TargetName, TocEntry, Toctree, ToctreeFlag, ToctreeOptions,
};

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::explicit_title::split_explicit_title;

use super::option_value::{DepthLimit, OptionValueType};
use super::options::{DirectiveOptions, OptionLine, classify_options, directive_options};
use super::spec::OptionValue;

const DIRECTIVE: &str = "toctree";

/// The URL schemes an entry may start with to be an external link rather than
/// a document path.
///
/// Sphinx decides this with `docutils.parsers.rst.directives.uri`, which
/// accepts any scheme; this list is deliberately narrower, because a document
/// path is by far the commoner entry and a bare `foo:bar` is much more likely
/// to be a mistyped path than a link. A scheme this misses becomes a
/// `Document` entry and is then reported as a missing document, which names
/// the real problem either way.
const URL_SCHEMES: [&str; 5] = ["http://", "https://", "ftp://", "ftps://", "mailto:"];

directive_options! {
    /// The options `.. toctree::` takes: Sphinx's ten.
    enum ToctreeOption {
        /// `:maxdepth:`.
        Maxdepth(DepthLimit) => (["maxdepth"], ToctreeMaxdepthInvalid, "How many heading levels the tree shows; zero or less shows all."),
        /// `:numbered:`.
        Numbered(Numbering) => (["numbered"], ToctreeNumberedInvalid, "Numbers the sections, to the depth given if any."),
        /// `:caption:`.
        Caption(String) => (["caption"], "A caption shown above the tree."),
        /// `:name:`.
        Name(TargetName) => (["name"], "A label to reference the tree by."),
        /// `:titlesonly:`.
        TitlesOnly => (["titlesonly"], Flag, "Lists each document's title only, not its sections."),
        /// `:reversed:`.
        Reversed => (["reversed"], Flag, "Reverses the order of the entries."),
        /// `:hidden:`.
        Hidden => (["hidden"], Flag, "Builds the navigation from the tree without showing it."),
        /// `:includehidden:`.
        IncludeHidden => (["includehidden"], Flag, "Also expands the hidden trees of the documents listed."),
        /// `:glob:`.
        Glob => (["glob"], Flag, "Reads the entries as glob patterns."),
        _ => (["class"], Classes, "Classes for the tree's container."),
    }
}

/// Reads the options `.. toctree::` accepts, reporting every other one.
fn parse_toctree_options(
    option_lines: &[OptionLine],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> ToctreeOptions {
    let mut options = ToctreeOptions::default();
    for (option, _) in classify_options::<ToctreeOption>(option_lines, DIRECTIVE, diagnostics, ctx)
    {
        match option {
            ToctreeOption::Maxdepth(depth) => options.maxdepth = depth.0,
            ToctreeOption::Numbered(numbering) => options.numbered = numbering.0,
            ToctreeOption::Caption(caption) => options.caption = Some(caption),
            ToctreeOption::Name(name) => options.name = Some(name),
            ToctreeOption::TitlesOnly => options.set(ToctreeFlag::TitlesOnly),
            ToctreeOption::Reversed => options.set(ToctreeFlag::Reversed),
            ToctreeOption::Hidden => options.set(ToctreeFlag::Hidden),
            ToctreeOption::IncludeHidden => options.set(ToctreeFlag::IncludeHidden),
            ToctreeOption::Glob => options.set(ToctreeFlag::Glob),
        }
    }
    options
}

/// A `:numbered:`: no value numbers every level, a positive number limits the
/// depth, and zero or less numbers nothing — Sphinx's reading, with any other
/// value reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Numbering(Option<NumberedDepth>);

impl OptionValueType for Numbering {
    const VALUE: OptionValue = OptionValue::Integer;

    fn read(line: &OptionLine, _ctx: &ParseCtx<'_>) -> Result<Self, String> {
        let value = line.value.trim();
        if value.is_empty() {
            return Ok(Self(Some(NumberedDepth::Unlimited)));
        }
        value
            .parse::<isize>()
            .map(|value| {
                Self(
                    usize::try_from(value)
                        .ok()
                        .and_then(NonZeroUsize::new)
                        .map(NumberedDepth::Levels),
                )
            })
            .map_err(|_| format!("needs a whole number or no value, found '{value}'"))
    }
}

/// Classifies one entry line into the kind of thing it names.
///
/// `glob` comes from the directive's own `:glob:` option, which is why this
/// runs only after the options are parsed: without it, `api/*` is an ordinary
/// (if oddly named) document path, exactly as Sphinx treats it.
fn parse_entry(line: &str, glob: bool, span: Option<rinx_ast::Span>) -> TocEntry {
    let (title, target) = match split_explicit_title(line) {
        Some((display, target)) if !display.is_empty() => (Some(display), target),
        Some((_, target)) => (None, target),
        None => (None, line.to_string()),
    };

    if target == "self" {
        return TocEntry::SelfRef { title, span };
    }
    if URL_SCHEMES.iter().any(|scheme| target.starts_with(scheme)) {
        return TocEntry::External {
            title,
            url: target,
            span,
        };
    }
    if glob && target.contains(['*', '?', '[']) {
        // A glob has no title: Sphinx has no syntax for titling a pattern that
        // expands to an arbitrary number of documents, so an explicit title
        // written here is discarded rather than silently applied to whichever
        // document happens to match first.
        return TocEntry::Glob {
            pattern: target,
            span,
        };
    }
    TocEntry::Document {
        title,
        docname: target,
        span,
    }
}

/// This directive's entry in the table of directives this build implements.
pub(in crate::directives) const SPEC: DirectiveSpec = DirectiveSpec {
    names: &["toctree"],
    doc: "A table of contents linking other documents into the navigation.",
    shape: DirectiveShape {
        arguments: Arguments::NONE,
        takes_options: true,
        takes_content: true,
        content_hint: None,
    },
    options: <ToctreeOption as DirectiveOptions>::GROUPS,
    accepts_further: false,
};

/// Interprets `.. toctree::`.
pub(in crate::directives) fn interpret_toctree(
    input: &DirectiveInvocation<'_, '_>,
    state: &mut ParseState<'_>,
) -> Vec<rinx_ast::Node> {
    single(parse_toctree(input, state.diagnostics))
}

/// Parses a `.. toctree::` body into a [`Directive::Toctree`].
///
/// Entries are classified but never *resolved*: a relative path stays
/// relative and a glob stays a glob. Both need a project-wide document list
/// the parser does not have, and the phases that do have one each need a
/// different list — see [`Toctree`]'s doc comment.
pub(super) fn parse_toctree(
    invocation: &DirectiveInvocation<'_, '_>,
    diagnostics: &mut Diagnostics,
) -> Directive {
    let ctx = &invocation.ctx;
    let options = parse_toctree_options(&invocation.options, diagnostics, ctx);

    let content = invocation.content();
    let entries = content
        .lines
        .iter()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(index, line)| {
            let trimmed = line.trim();
            parse_entry(
                trimmed,
                options.has(ToctreeFlag::Glob),
                content.ctx.line_span(index, line),
            )
        })
        .collect();

    Directive::Toctree(Toctree { entries, options })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use rinx_ast::{Diagnostic, DiagnosticCode};
    use rinx_ast::{Domain, Node};

    /// Parses a directive body directly, bypassing the block dispatcher, for
    /// tests about `parse_toctree` itself rather than about recognition.
    fn parse_body(body_lines: &[&str]) -> (Toctree, Diagnostics) {
        let mut diagnostics = Diagnostics::default();
        let ctx = ParseCtx::with_domain(Domain::Py);
        let invocation = crate::directives::reader::test_invocation(
            "toctree",
            SPEC.shape,
            "",
            body_lines,
            &mut diagnostics,
            &ctx,
        );
        let directive = parse_toctree(&invocation, &mut diagnostics);
        match directive {
            Directive::Toctree(toctree) => (toctree, diagnostics),
            other => panic!("Expected Toctree, got {other:?}"),
        }
    }

    /// Parses a whole document and returns its single toctree, for tests that
    /// need the real span origin the dispatcher sets up.
    fn parse_document(input: &str) -> (Toctree, Vec<Diagnostic>) {
        let doc = parse("test.rst", input);
        let toctree = doc
            .nodes
            .iter()
            .find_map(|node| match node {
                Node::Directive(Directive::Toctree(toctree)) => Some(toctree.clone()),
                _ => None,
            })
            .expect("document should contain a toctree");
        (toctree, doc.diagnostics)
    }

    #[test]
    fn test_parse_toctree_collects_plain_document_entries() {
        // Given
        let body = ["path1", "path2/index"];

        // When
        let (toctree, diagnostics) = parse_body(&body);

        // Then
        assert!(diagnostics.is_empty());
        assert_eq!(toctree.entries.len(), 2);
        assert!(matches!(
            &toctree.entries[0],
            TocEntry::Document { docname, title: None, .. } if docname == "path1"
        ));
        assert!(matches!(
            &toctree.entries[1],
            TocEntry::Document { docname, title: None, .. } if docname == "path2/index"
        ));
    }

    #[test]
    fn test_parse_toctree_skips_blank_lines_between_entries() {
        // Given
        let body = ["path1", "", "path2"];

        // When
        let (toctree, _) = parse_body(&body);

        // Then
        assert_eq!(toctree.entries.len(), 2);
    }

    #[test]
    fn test_parse_toctree_reads_an_explicit_title_entry() {
        // Given
        let body = ["Getting started <intro>"];

        // When
        let (toctree, _) = parse_body(&body);

        // Then — the shared `Display text <target>` helper split it.
        assert!(matches!(
            &toctree.entries[0],
            TocEntry::Document { title: Some(title), docname, .. }
                if title == "Getting started" && docname == "intro"
        ));
    }

    #[test]
    fn test_parse_toctree_reads_a_self_entry() {
        // Given
        let body = ["self"];

        // When
        let (toctree, _) = parse_body(&body);

        // Then
        assert!(matches!(
            &toctree.entries[0],
            TocEntry::SelfRef { title: None, .. }
        ));
    }

    #[test]
    fn test_parse_toctree_reads_a_titled_self_entry() {
        // Given — Sphinx allows a title on `self`.
        let body = ["Overview <self>"];

        // When
        let (toctree, _) = parse_body(&body);

        // Then
        assert!(matches!(
            &toctree.entries[0],
            TocEntry::SelfRef { title: Some(title), .. } if title == "Overview"
        ));
    }

    #[test]
    fn test_parse_toctree_reads_an_external_url_entry() {
        // Given
        let body = ["Upstream <https://example.org/docs>"];

        // When
        let (toctree, _) = parse_body(&body);

        // Then
        assert!(matches!(
            &toctree.entries[0],
            TocEntry::External { title: Some(title), url, .. }
                if title == "Upstream" && url == "https://example.org/docs"
        ));
    }

    #[test]
    fn test_parse_toctree_reads_a_bare_url_entry() {
        // Given
        let body = ["https://example.org/docs"];

        // When
        let (toctree, _) = parse_body(&body);

        // Then
        assert!(matches!(
            &toctree.entries[0],
            TocEntry::External { title: None, url, .. } if url == "https://example.org/docs"
        ));
    }

    #[test]
    fn test_parse_toctree_reads_a_glob_entry_only_when_glob_is_set() {
        // Given
        let body = [":glob:", "api/*"];

        // When
        let (toctree, _) = parse_body(&body);

        // Then
        assert!(toctree.options.has(ToctreeFlag::Glob));
        assert!(matches!(
            &toctree.entries[0],
            TocEntry::Glob { pattern, .. } if pattern == "api/*"
        ));
    }

    #[test]
    fn test_parse_toctree_treats_a_wildcard_as_a_path_without_the_glob_option() {
        // Given — without `:glob:`, Sphinx treats `api/*` as a literal path.
        let body = ["api/*"];

        // When
        let (toctree, _) = parse_body(&body);

        // Then
        assert!(matches!(
            &toctree.entries[0],
            TocEntry::Document { docname, .. } if docname == "api/*"
        ));
    }

    #[test]
    fn test_parse_toctree_reads_maxdepth() {
        // Given
        let body = [":maxdepth: 2", "path1"];

        // When
        let (toctree, diagnostics) = parse_body(&body);

        // Then
        assert!(diagnostics.is_empty());
        assert_eq!(toctree.options.maxdepth, NonZeroUsize::new(2));
        assert_eq!(toctree.entries.len(), 1);
    }

    #[test]
    fn test_parse_toctree_treats_a_negative_maxdepth_as_unlimited() {
        // Given — Sphinx's documented spelling of "no limit".
        let body = [":maxdepth: -1"];

        // When
        let (toctree, diagnostics) = parse_body(&body);

        // Then
        assert_eq!(toctree.options.maxdepth, None);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_parse_toctree_treats_a_zero_maxdepth_as_unlimited() {
        // Given
        let body = [":maxdepth: 0"];

        // When
        let (toctree, diagnostics) = parse_body(&body);

        // Then
        assert_eq!(toctree.options.maxdepth, None);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_parse_toctree_diagnoses_a_non_numeric_maxdepth() {
        // Given — previously dropped in silence, which quietly rendered an
        // unlimited tree.
        let body = [":maxdepth: deep"];

        // When
        let (toctree, diagnostics) = parse_body(&body);

        // Then
        assert_eq!(toctree.options.maxdepth, None);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics.entries()[0].code,
            DiagnosticCode::ToctreeMaxdepthInvalid
        );
    }

    #[test]
    fn test_parse_toctree_reads_a_bare_numbered_as_unlimited() {
        // Given
        let body = [":numbered:"];

        // When
        let (toctree, diagnostics) = parse_body(&body);

        // Then
        assert_eq!(toctree.options.numbered, Some(NumberedDepth::Unlimited));
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_parse_toctree_reads_a_numbered_depth() {
        // Given
        let body = [":numbered: 2"];

        // When
        let (toctree, _) = parse_body(&body);

        // Then
        assert_eq!(
            toctree.options.numbered,
            Some(NumberedDepth::Levels(NonZeroUsize::new(2).unwrap()))
        );
    }

    #[test]
    fn test_parse_toctree_treats_a_zero_numbered_as_unnumbered() {
        // Given
        let body = [":numbered: 0"];

        // When
        let (toctree, diagnostics) = parse_body(&body);

        // Then
        assert_eq!(toctree.options.numbered, None);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_parse_toctree_diagnoses_a_non_numeric_numbered() {
        // Given
        let body = [":numbered: lots"];

        // When
        let (toctree, diagnostics) = parse_body(&body);

        // Then
        assert_eq!(toctree.options.numbered, None);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics.entries()[0].code,
            DiagnosticCode::ToctreeNumberedInvalid
        );
    }

    #[test]
    fn test_parse_toctree_reads_the_caption() {
        // Given
        let body = [":caption: Getting Started"];

        // When
        let (toctree, _) = parse_body(&body);

        // Then
        assert_eq!(toctree.options.caption.as_deref(), Some("Getting Started"));
    }

    #[test]
    fn test_parse_toctree_reads_the_name_as_a_target() {
        // Given
        let body = [":name: main-toc"];

        // When
        let (toctree, diagnostics) = parse_body(&body);

        // Then
        assert_eq!(toctree.options.name, Some(TargetName::new("main-toc")));
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_parse_toctree_diagnoses_an_empty_name() {
        // Given — nothing could ever reference this toctree.
        let body = [":name:"];

        // When
        let (toctree, diagnostics) = parse_body(&body);

        // Then
        assert_eq!(toctree.options.name, None);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics.entries()[0].code,
            DiagnosticCode::DirectiveEmptyOptionValue
        );
    }

    #[test]
    fn test_parse_toctree_reads_every_flag_option() {
        // Given
        let body = [":titlesonly:", ":reversed:", ":hidden:", ":includehidden:"];

        // When
        let (toctree, diagnostics) = parse_body(&body);

        // Then
        assert!(diagnostics.is_empty());
        assert!(toctree.options.has(ToctreeFlag::TitlesOnly));
        assert!(toctree.options.has(ToctreeFlag::Reversed));
        assert!(toctree.options.has(ToctreeFlag::Hidden));
        assert!(toctree.options.has(ToctreeFlag::IncludeHidden));
        assert!(!toctree.options.has(ToctreeFlag::Glob));
    }

    #[test]
    fn test_parse_toctree_diagnoses_an_unknown_option_under_its_own_code() {
        // Given — the code is author-facing and already suppressible, so it
        // must survive the move onto the shared option scanner.
        let input = ".. toctree::\n   :invalid_opt:\n\n   foo";

        // When
        let (toctree, diagnostics) = parse_document(input);

        // Then
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, DiagnosticCode::DirectiveUnknownOption);
        assert!(diagnostics[0].message.contains(":invalid_opt:"));
        assert_eq!(toctree.entries.len(), 1);
    }

    #[test]
    fn test_parse_toctree_records_a_span_for_each_entry() {
        // Given — the span is what lets the index phase, much later, point a
        // missing-document warning at the line the author wrote.
        let input = ".. toctree::\n\n   first\n   second\n";

        // When
        let (toctree, _) = parse_document(input);

        // Then
        let lines: Vec<u32> = toctree
            .entries
            .iter()
            .map(|entry| entry.span().expect("entry has a span").start.line)
            .collect();
        assert_eq!(lines, vec![3, 4]);
    }
}

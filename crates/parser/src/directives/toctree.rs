use std::num::NonZeroUsize;

use rusty_sphinx_ast::{
    Diagnostic, DiagnosticCode, Directive, NumberedDepth, TargetName, TocEntry, Toctree,
    ToctreeFlag, ToctreeOptions,
};

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::explicit_title::split_explicit_title;
use crate::indent::unindent_body_lines;

use super::options::{OptionLine, report_unknown_options, scan_option_lines};

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

/// Reads the nine options `.. toctree::` accepts, returning them with the
/// lines it did not recognize, in source order.
///
/// Both depth options treat a zero as *unset* rather than as a real limit,
/// which is what Sphinx does: `:maxdepth: 0` renders an unlimited tree and
/// `:numbered: 0` numbers nothing. That ambiguity is resolved here, in the one
/// place that sees the source text, so [`ToctreeOptions`] can use
/// `NonZeroUsize` and make the meaningless state unrepresentable downstream.
///
/// A negative `:maxdepth:` is Sphinx's documented spelling of "unlimited" and
/// is accepted silently; any other non-integer value is a diagnostic, because
/// silently switching to unlimited would render a far larger tree than the
/// author asked for.
fn parse_toctree_options<'a>(
    option_lines: &'a [OptionLine],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> (ToctreeOptions, Vec<&'a OptionLine>) {
    let mut options = ToctreeOptions::default();
    let mut unrecognized = Vec::new();

    for line in option_lines {
        match line.name.as_str() {
            "maxdepth" => options.maxdepth = parse_depth(line, diagnostics, ctx),
            "numbered" => options.numbered = parse_numbered(line, diagnostics, ctx),
            "caption" => options.caption = Some(line.value.clone()),
            "name" => {
                if line.value.is_empty() {
                    diagnostics.push(Diagnostic::at(
                        DiagnosticCode::ToctreeEmptyName,
                        format!("A {DIRECTIVE} :name: option needs a value: {}", line.raw),
                        ctx.line_span(line.line_index, &line.raw),
                    ));
                } else {
                    options.name = Some(TargetName::new(&line.value));
                }
            }
            name => match ToctreeFlag::from_option_name(name) {
                Some(flag) => options.set(flag),
                None => unrecognized.push(line),
            },
        }
    }

    (options, unrecognized)
}

/// Reads a `:maxdepth:` value. A negative number means unlimited, as does a
/// zero; anything else non-numeric is diagnosed.
fn parse_depth(
    line: &OptionLine,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<NonZeroUsize> {
    match line.value.trim().parse::<isize>() {
        Ok(value) if value <= 0 => None,
        Ok(value) => NonZeroUsize::new(usize::try_from(value).unwrap_or(0)),
        Err(_) => {
            diagnostics.push(Diagnostic::at(
                DiagnosticCode::ToctreeMaxdepthInvalid,
                format!(
                    "A {DIRECTIVE} :maxdepth: option needs a whole number: {}",
                    line.raw
                ),
                ctx.line_span(line.line_index, &line.raw),
            ));
            None
        }
    }
}

/// Reads a `:numbered:` value: absent means unlimited, a positive number
/// limits the depth, zero disables numbering, and anything else is diagnosed.
fn parse_numbered(
    line: &OptionLine,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<NumberedDepth> {
    let value = line.value.trim();
    if value.is_empty() {
        return Some(NumberedDepth::Unlimited);
    }
    match value.parse::<isize>() {
        Ok(value) if value <= 0 => None,
        Ok(value) => {
            NonZeroUsize::new(usize::try_from(value).unwrap_or(0)).map(NumberedDepth::Levels)
        }
        Err(_) => {
            diagnostics.push(Diagnostic::at(
                DiagnosticCode::ToctreeNumberedInvalid,
                format!(
                    "A {DIRECTIVE} :numbered: option needs a whole number or no value: {}",
                    line.raw
                ),
                ctx.line_span(line.line_index, &line.raw),
            ));
            None
        }
    }
}

/// Classifies one entry line into the kind of thing it names.
///
/// `glob` comes from the directive's own `:glob:` option, which is why this
/// runs only after the options are parsed: without it, `api/*` is an ordinary
/// (if oddly named) document path, exactly as Sphinx treats it.
fn parse_entry(line: &str, glob: bool, span: Option<rusty_sphinx_ast::Span>) -> TocEntry {
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

/// Parses a `.. toctree::` body into a [`Directive::Toctree`].
///
/// Entries are classified but never *resolved*: a relative path stays
/// relative and a glob stays a glob. Both need a project-wide document list
/// the parser does not have, and the phases that do have one each need a
/// different list — see [`Toctree`]'s doc comment.
pub(super) fn parse_toctree(
    body_lines: &[&str],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Directive {
    let unindented_lines = unindent_body_lines(body_lines);
    let (option_lines, body_start) = scan_option_lines(&unindented_lines);
    let (options, unrecognized) = parse_toctree_options(&option_lines, diagnostics, ctx);
    report_unknown_options(
        &unrecognized,
        DIRECTIVE,
        DiagnosticCode::DirectiveToctreeUnknownOption,
        diagnostics,
        ctx,
    );

    let entries = unindented_lines
        .iter()
        .enumerate()
        .skip(body_start)
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(index, line)| {
            let trimmed = line.trim();
            parse_entry(
                trimmed,
                options.has(ToctreeFlag::Glob),
                ctx.line_span(index, line),
            )
        })
        .collect();

    Directive::Toctree(Toctree { entries, options })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use rusty_sphinx_ast::{Domain, Node};

    /// Parses a directive body directly, bypassing the block dispatcher, for
    /// tests about `parse_toctree` itself rather than about recognition.
    fn parse_body(body_lines: &[&str]) -> (Toctree, Diagnostics) {
        let mut diagnostics = Diagnostics::default();
        let directive = parse_toctree(
            body_lines,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );
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
            DiagnosticCode::ToctreeEmptyName
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
        assert_eq!(
            diagnostics[0].code,
            DiagnosticCode::DirectiveToctreeUnknownOption
        );
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

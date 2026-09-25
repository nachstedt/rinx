//! `.. if-builder::` — sphinx-simplepdf's conditional-on-the-builder content.
//!
//! The body is parsed and contributed only when the argument names the builder
//! that is running. This build has exactly one, [`BUILDER_NAME`], so in
//! practice `.. if-builder:: html` includes and every other name excludes.
//!
//! Like [`super::include`], and unlike every other body-bearing directive
//! here, this one contributes *several* nodes rather than wrapping them: a
//! section heading, a hyperlink target or a `.. toctree::` written inside it
//! belongs to the enclosing document exactly as if it had been typed there.
//! That is not an optimization but the directive's whole purpose — upstream
//! parses the matching body with docutils' `nested_parse_with_titles`
//! specifically so that titles and toctrees inside it are real structure, and
//! sphinx-simplepdf's own documentation recommends it over `.. only::` on
//! exactly that ground.
//!
//! Which is also why this build does **not** reproduce the
//! `<div class="docutils container">` that upstream's `nodes.container()`
//! emits around the included body. A container node here would make
//! [`rinx_analyzer`]'s outline builder stop at the block — it does not
//! descend into directive bodies, since reStructuredText has no section inside
//! one — so the headings the directive exists to admit would silently stop
//! being sections. The wrapper carries nothing the source expressed; the
//! structure does. Do not "fix" this back.
//!
//! A non-matching body is never parsed, which is upstream's behaviour too
//! (its `nested_parse_with_titles` call sits inside the `if`). That is what
//! makes a `.. if-builder:: simplepdf` block full of PDF-only directives free:
//! no unknown-directive reports, no targets, no toctree entries, and so no
//! Bazel `deps` entry for documents only the PDF build would reach.

use rinx_ast::{Diagnostic, DiagnosticCode, Node, Span};

use crate::blocks::parse_blocks;
use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::directives::error_node::malformed_directive;
use crate::headings::Adornment;
use crate::indent::unindent_body_lines;

/// The directive name, used throughout this module's diagnostics.
const DIRECTIVE: &str = "if-builder";

/// The builder this program is.
///
/// The one place rinx names its own builder: it produces HTML and
/// nothing else, so a second builder would start here rather than by
/// scattering the string.
const BUILDER_NAME: &str = "html";

/// Builder names that are *recognised* but are not this one, so naming them
/// excludes the block silently.
///
/// sphinx-simplepdf's own builder plus Sphinx's built-ins. Meaning one of
/// these is the normal use of the directive — hiding content from the PDF or
/// the man pages — and reporting it would fire on every correct document.
/// Anything outside this list and [`BUILDER_NAME`] is far likelier to be a
/// typo, and a typo here deletes content silently, so it is reported.
///
/// Extending the list is a one-line change; it is not a promise that this
/// build can *produce* any of them.
const KNOWN_BUILDERS: &[&str] = &[
    "simplepdf",
    "applehelp",
    "changes",
    "devhelp",
    "dirhtml",
    "dummy",
    "epub",
    "gettext",
    "htmlhelp",
    "json",
    "latex",
    "linkcheck",
    "man",
    "pickle",
    "pseudoxml",
    "qthelp",
    "singlehtml",
    "text",
    "texinfo",
    "xml",
];

/// Parses a `.. if-builder::`, returning the nodes its body contributed.
///
/// Empty when the named builder is not this one — the ordinary case, and not a
/// diagnostic. A single degraded [`Node::Directive`] when the argument names
/// no builder at all, so the page shows what could not be decided instead of
/// losing the block without trace.
pub(in crate::directives) fn parse_if_builder(
    argument: &str,
    directive_span: Option<Span>,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Vec<Node> {
    let builder = argument.trim();
    if builder.is_empty() {
        return vec![Node::Directive(malformed_directive(
            DIRECTIVE,
            argument,
            body_lines,
            DiagnosticCode::IfBuilderMissingBuilder,
            format!("{DIRECTIVE}: needs the name of a builder, such as '{BUILDER_NAME}'"),
            directive_span,
            diagnostics,
        ))];
    }

    if !selects_this_build(builder) {
        if !is_known_builder(builder) {
            diagnostics.push(Diagnostic::at(
                DiagnosticCode::IfBuilderUnknownBuilder,
                format!(
                    "{DIRECTIVE}: unknown builder '{builder}'; this build is '{BUILDER_NAME}', \
                     so the block was left out"
                ),
                directive_span,
            ));
        }
        return Vec::new();
    }

    let content = unindent_body_lines(body_lines);
    if content.is_empty() {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::IfBuilderEmptyBody,
            format!("{DIRECTIVE}: '{builder}' is this build, but the block has no content"),
            directive_span,
        ));
        return Vec::new();
    }

    // `adornment_order` is threaded rather than reset, exactly as an
    // `.. include::` threads it: a heading in here takes its level from the
    // document's own adornment sequence, which is what makes the splice
    // transparent. This is the local spelling of upstream's
    // `nested_parse_with_titles`.
    let lines: Vec<&str> = content.iter().map(String::as_str).collect();
    parse_blocks(&lines, adornment_order, diagnostics, ctx)
}

/// Whether `builder` names the builder this program is.
///
/// Case-insensitively, porting upstream's `.upper() == .upper()`. ASCII
/// folding rather than full Unicode: every builder name is an ASCII
/// identifier, so the two rules can only differ on a name no builder has.
fn selects_this_build(builder: &str) -> bool {
    builder.eq_ignore_ascii_case(BUILDER_NAME)
}

/// Whether `builder` is a builder name this build recognises without being it.
fn is_known_builder(builder: &str) -> bool {
    KNOWN_BUILDERS
        .iter()
        .any(|known| builder.eq_ignore_ascii_case(known))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::{Directive, Document, Domain, InlineNode};
    use std::collections::HashSet;

    /// Parses a whole document through the public entry point, so the
    /// multi-node splice is exercised the way the block loop drives it.
    fn parse_document(rst: &str) -> Document {
        let ctx = ParseCtx::with_domain(Domain::Py);
        crate::parse_with_ctx("guide.rst", rst, &ctx)
    }

    fn codes(document: &Document) -> Vec<DiagnosticCode> {
        document.diagnostics.iter().map(|d| d.code).collect()
    }

    /// The text of every paragraph in `nodes`, for asserting on spliced prose.
    fn paragraphs(nodes: &[Node]) -> Vec<String> {
        nodes
            .iter()
            .filter_map(|node| match node {
                Node::Paragraph(inline) => Some(
                    inline
                        .iter()
                        .map(|part| match part {
                            InlineNode::Text(text) => text.clone(),
                            other => format!("{other:?}"),
                        })
                        .collect::<String>(),
                ),
                _ => None,
            })
            .collect()
    }

    /// Every heading in `nodes`, as `(level, text)`.
    fn headings(nodes: &[Node]) -> Vec<(u8, String)> {
        nodes
            .iter()
            .filter_map(|node| match node {
                Node::Heading { level, text } => Some((
                    *level,
                    text.iter()
                        .map(|part| match part {
                            InlineNode::Text(text) => text.clone(),
                            other => format!("{other:?}"),
                        })
                        .collect::<String>(),
                )),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn test_a_matching_builder_splices_its_body_into_the_document() {
        // Given a block naming the builder this build is
        let document = parse_document("Before.\n\n.. if-builder:: html\n\n   Inside.\n\nAfter.\n");

        // Then — spliced in place, not wrapped in a container
        assert_eq!(
            paragraphs(&document.nodes),
            vec![
                "Before.".to_string(),
                "Inside.".to_string(),
                "After.".to_string()
            ]
        );
        assert!(
            document.diagnostics.is_empty(),
            "{:?}",
            document.diagnostics
        );
    }

    #[test]
    fn test_a_matching_builder_contributes_several_nodes() {
        // Given a body of more than one block
        let document =
            parse_document(".. if-builder:: html\n\n   First.\n\n   Second.\n\nAfter.\n");

        // Then — every one of them belongs to the document
        assert_eq!(
            paragraphs(&document.nodes),
            vec![
                "First.".to_string(),
                "Second.".to_string(),
                "After.".to_string()
            ]
        );
    }

    #[test]
    fn test_another_builder_contributes_nothing_and_is_silent() {
        // Given a block for the PDF builder
        let document =
            parse_document("Before.\n\n.. if-builder:: simplepdf\n\n   Inside.\n\nAfter.\n");

        // Then — the body is gone, and naming another builder is not a mistake
        assert_eq!(
            paragraphs(&document.nodes),
            vec!["Before.".to_string(), "After.".to_string()]
        );
        assert!(
            document.diagnostics.is_empty(),
            "{:?}",
            document.diagnostics
        );
    }

    #[test]
    fn test_the_builder_name_is_matched_case_insensitively() {
        // Given the name in capitals, which upstream's `.upper()` accepts
        let document = parse_document(".. if-builder:: HTML\n\n   Inside.\n");

        // Then
        assert_eq!(paragraphs(&document.nodes), vec!["Inside.".to_string()]);
        assert!(
            document.diagnostics.is_empty(),
            "{:?}",
            document.diagnostics
        );
    }

    #[test]
    fn test_an_excluded_body_is_never_parsed() {
        // Given a non-matching block whose body would not parse on its own
        let document = parse_document(".. if-builder:: latex\n\n   .. nonesuch-directive::\n");

        // Then — no unknown-directive report, because nothing in there was
        // read. This is what makes a PDF-only block free.
        assert!(
            document.diagnostics.is_empty(),
            "{:?}",
            document.diagnostics
        );
        assert!(document.nodes.is_empty(), "{:?}", document.nodes);
    }

    #[test]
    fn test_a_heading_inside_a_matching_block_becomes_a_real_heading() {
        // Given a heading under an already-established adornment
        let document = parse_document(
            "Guide\n=====\n\n.. if-builder:: html\n\n   Install\n   -------\n\n   Steps.\n",
        );

        // Then — it takes its level from the document's own adornment
        // sequence, exactly as an included heading does.
        assert_eq!(
            headings(&document.nodes),
            vec![(1, "Guide".to_string()), (2, "Install".to_string())]
        );
    }

    #[test]
    fn test_a_nested_if_builder_is_evaluated_too() {
        // Given a matching block inside a matching block
        let document = parse_document(
            ".. if-builder:: html\n\n   Outer.\n\n   .. if-builder:: html\n\n      Inner.\n",
        );

        // Then
        assert_eq!(
            paragraphs(&document.nodes),
            vec!["Outer.".to_string(), "Inner.".to_string()]
        );
    }

    #[test]
    fn test_a_non_matching_block_nested_in_a_matching_one_is_left_out() {
        // Given
        let document = parse_document(
            ".. if-builder:: html\n\n   Outer.\n\n   .. if-builder:: latex\n\n      Inner.\n",
        );

        // Then
        assert_eq!(paragraphs(&document.nodes), vec!["Outer.".to_string()]);
    }

    #[test]
    fn test_a_missing_builder_name_is_reported_and_the_block_kept() {
        // Given no argument at all
        let document = parse_document(".. if-builder::\n\n   Inside.\n");

        // Then — reported, and drawn as a visible error block quoting the
        // source, so the content is not lost without trace.
        assert_eq!(
            codes(&document),
            vec![DiagnosticCode::IfBuilderMissingBuilder]
        );
        assert!(
            matches!(
                document.nodes.as_slice(),
                [Node::Directive(Directive::Malformed { name, .. })] if name == DIRECTIVE
            ),
            "{:?}",
            document.nodes
        );
    }

    #[test]
    fn test_an_unknown_builder_name_is_reported_and_the_block_left_out() {
        // Given a plausible typo of a real builder
        let document = parse_document(".. if-builder:: htlm\n\n   Inside.\n");

        // Then — upstream would exclude this silently, which makes the typo
        // indistinguishable from a deliberate exclusion.
        assert_eq!(
            codes(&document),
            vec![DiagnosticCode::IfBuilderUnknownBuilder]
        );
        assert!(document.nodes.is_empty(), "{:?}", document.nodes);
    }

    #[test]
    fn test_the_unknown_builder_message_quotes_what_was_written() {
        // Given
        let document = parse_document(".. if-builder:: htlm\n\n   Inside.\n");

        // Then
        assert!(
            document.diagnostics[0].message.contains("'htlm'"),
            "{}",
            document.diagnostics[0].message
        );
    }

    #[test]
    fn test_a_multi_word_argument_is_an_unknown_builder() {
        // Given — the directive takes the rest of its line, so this really is
        // one argument, and no builder is named like it.
        let document = parse_document(".. if-builder:: html and latex\n\n   Inside.\n");

        // Then
        assert_eq!(
            codes(&document),
            vec![DiagnosticCode::IfBuilderUnknownBuilder]
        );
        assert!(document.nodes.is_empty(), "{:?}", document.nodes);
    }

    #[test]
    fn test_an_empty_body_is_reported_when_the_builder_matches() {
        // Given a selected block with nothing in it
        let document = parse_document("Before.\n\n.. if-builder:: html\n\nAfter.\n");

        // Then
        assert_eq!(codes(&document), vec![DiagnosticCode::IfBuilderEmptyBody]);
    }

    #[test]
    fn test_an_empty_body_is_silent_when_the_builder_does_not_match() {
        // Given the same emptiness on a branch nobody selected
        let document = parse_document("Before.\n\n.. if-builder:: latex\n\nAfter.\n");

        // Then — emptiness only means a mistake on the branch that was taken.
        assert!(
            document.diagnostics.is_empty(),
            "{:?}",
            document.diagnostics
        );
    }

    #[test]
    fn test_a_target_inside_a_matching_block_belongs_to_the_document() {
        // Given a hyperlink target written inside the block
        let document = parse_document(".. if-builder:: html\n\n   .. _inside-label:\n\n   Text.\n");

        // Then — it is a document-level target, not something nested inside a
        // container, which is what lets a `:ref:` elsewhere resolve to it.
        assert!(
            document.nodes.iter().any(
                |node| matches!(node, Node::Target { name, .. } if name.as_str() == "inside-label")
            ),
            "{:?}",
            document.nodes
        );
    }

    #[test]
    fn test_selects_this_build_accepts_only_this_builders_name() {
        // Given / When / Then
        assert!(selects_this_build("html"));
        assert!(selects_this_build("HtMl"));
        assert!(!selects_this_build("simplepdf"));
        assert!(!selects_this_build("htmlhelp"));
        assert!(!selects_this_build(""));
    }

    #[test]
    fn test_is_known_builder_recognizes_the_table_case_insensitively() {
        // Given / When / Then
        assert!(is_known_builder("simplepdf"));
        assert!(is_known_builder("LaTeX"));
        assert!(!is_known_builder("htlm"));
        // This build's own name is not in the table; `selects_this_build`
        // answers for it, and both callers run before this one is consulted.
        assert!(!is_known_builder(BUILDER_NAME));
    }

    #[test]
    fn test_the_known_builder_table_is_well_formed() {
        // Given the hand-maintained table
        let names: HashSet<&str> = KNOWN_BUILDERS.iter().copied().collect();

        // When / Then — a duplicate is harmless but a sign of a half-finished
        // edit, and a capital would never match the lowercase names Sphinx
        // uses even though the comparison folds case.
        assert_eq!(names.len(), KNOWN_BUILDERS.len(), "a name is listed twice");
        for name in KNOWN_BUILDERS {
            assert_eq!(*name, name.to_ascii_lowercase(), "{name} is not lowercase");
            assert!(!name.is_empty());
            assert_ne!(
                *name, BUILDER_NAME,
                "this build's own name must not be in the excluded-builders table",
            );
        }
    }
}

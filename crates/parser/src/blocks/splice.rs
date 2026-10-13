//! Sphinx's `rst_prolog` and `rst_epilog`: text a project adds to the start
//! and the end of every document.
//!
//! Sphinx pastes the text into the document's lines, after its leading field
//! list and before its end, with a blank line at each seam. A blank line ends
//! every block that could otherwise run across it, so parsing the text as a
//! segment of its own — with the document's adornment order, diagnostics and
//! role table — gives the same nodes without moving any of the document's
//! lines. What a segment adds is a [`FileId`](rinx_ast::FileId) of its own,
//! so a diagnostic about it names the prolog at the prolog's line, as Sphinx
//! reports `<rst_prolog>:2`. See `docs/decisions/040-rst-prolog.md`.

use rinx_ast::Node;

use super::dispatch::parse_blocks;
use crate::context::{ParseCtx, SplicedText};
use crate::diagnostics::Diagnostics;
use crate::headings::Adornment;

/// Parses `splice` as a segment of the document `ctx` parses, or yields
/// nothing for an empty text — which then names no file either.
pub(super) fn parse_spliced(
    splice: Option<SplicedText<'_>>,
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Vec<Node> {
    let Some(splice) = splice.filter(|splice| !splice.text.trim().is_empty()) else {
        return Vec::new();
    };
    let file = diagnostics.intern_source_file(splice.name);
    let lines: Vec<&str> = splice.text.lines().collect();
    parse_blocks(&lines, adornment_order, diagnostics, &ctx.spliced(file))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use rinx_ast::{DiagnosticCode, Document, Domain, InlineNode, Node, Position};

    use crate::context::{ParseCtx, SplicedText};
    use crate::{LoadedFile, ParseFileLoader, parse_with_ctx};

    /// A loader resolving a path to itself.
    struct FakeFiles(HashMap<String, String>);

    impl ParseFileLoader for FakeFiles {
        fn load(&self, path: &str, _relative_to: Option<&str>) -> Result<LoadedFile, String> {
            self.0
                .get(path)
                .map(|text| LoadedFile {
                    id: path.to_string(),
                    text: text.clone(),
                })
                .ok_or_else(|| format!("cannot read '{path}': no such file"))
        }
    }

    const NO_FILES: &[(&str, &str)] = &[];

    /// Parses `rst` as `guide.rst` with `prolog` and `epilog`, reading
    /// `files`.
    fn parse_decorated(prolog: &str, rst: &str, epilog: &str, files: &[(&str, &str)]) -> Document {
        let loader = FakeFiles(
            files
                .iter()
                .map(|(path, text)| ((*path).to_string(), (*text).to_string()))
                .collect(),
        );
        let ctx = ParseCtx::new(Domain::Py, &loader)
            .with_prolog(SplicedText {
                name: "prolog.rst",
                text: prolog,
            })
            .with_epilog(SplicedText {
                name: "epilog.rst",
                text: epilog,
            });
        parse_with_ctx("guide.rst", rst, &ctx)
    }

    fn codes(document: &Document) -> Vec<DiagnosticCode> {
        document.diagnostics.iter().map(|d| d.code).collect()
    }

    /// The name of the file `code`'s first diagnostic is in, and its start.
    fn located(document: &Document, code: DiagnosticCode) -> (Option<&str>, Position) {
        let diagnostic = document
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == code)
            .unwrap_or_else(|| panic!("no {code:?} in {:?}", document.diagnostics));
        let span = diagnostic.span.expect("positioned");
        let file = span
            .file
            .map(|file| document.source_files[file.index()].as_str());
        (file, span.start)
    }

    #[test]
    fn test_a_substitution_defined_in_the_prolog_resolves_in_the_document() {
        // When
        let document = parse_decorated(
            ".. |project| replace:: rinx\n",
            "About |project|.\n",
            "",
            NO_FILES,
        );

        // Then
        assert_eq!(codes(&document), Vec::new());
        let Some(Node::Paragraph(inline)) = document.nodes.last() else {
            panic!("expected a paragraph, got {:?}", document.nodes);
        };
        assert!(
            inline
                .iter()
                .any(|part| matches!(part, InlineNode::Text(text) if text.contains("rinx"))),
            "{inline:?}"
        );
    }

    #[test]
    fn test_a_substitution_defined_in_the_epilog_resolves_in_the_document() {
        // When
        let document = parse_decorated(
            "",
            "About |project|.\n",
            ".. |project| replace:: rinx\n",
            NO_FILES,
        );

        // Then
        assert_eq!(codes(&document), Vec::new());
    }

    #[test]
    fn test_a_role_defined_in_the_prolog_applies_to_the_document() {
        // When
        let document = parse_decorated(
            ".. role:: python(code)\n   :language: python\n",
            "Call :python:`f()`.\n",
            "",
            NO_FILES,
        );

        // Then
        assert_eq!(codes(&document), Vec::new());
        let Some(Node::Paragraph(inline)) = document.nodes.last() else {
            panic!("expected a paragraph, got {:?}", document.nodes);
        };
        assert!(
            inline
                .iter()
                .any(|part| matches!(part, InlineNode::Code { text, .. } if text == "f()")),
            "{inline:?}"
        );
    }

    #[test]
    fn test_the_epilog_comes_after_the_document() {
        // When
        let document = parse_decorated("Prolog.\n", "Document.\n", "Epilog.\n", NO_FILES);

        // Then
        let paragraphs: Vec<&Node> = document
            .nodes
            .iter()
            .filter(|node| matches!(node, Node::Paragraph(_)))
            .collect();
        assert_eq!(
            paragraphs,
            ["Prolog.", "Document.", "Epilog."]
                .map(|text| Node::Paragraph(vec![InlineNode::Text(text.to_string())]))
                .iter()
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_the_documents_own_positions_are_unchanged_by_a_prolog() {
        // When — the undefined substitution is on the document's line 3
        let document = parse_decorated(
            ".. |a| replace:: A\n.. |b| replace:: B\n",
            "Title\n=====\n\nAbout |missing|.\n",
            "",
            NO_FILES,
        );

        // Then
        assert_eq!(
            located(&document, DiagnosticCode::SubstitutionUndefined),
            (None, Position::new(4, 7))
        );
    }

    #[test]
    fn test_a_diagnostic_in_the_prolog_names_the_prolog_at_its_own_line() {
        // When
        let document = parse_decorated("\n.. nonsense::\n", "Title\n=====\n", "", NO_FILES);

        // Then
        assert_eq!(
            located(&document, DiagnosticCode::DirectiveUnknown),
            (Some("prolog.rst"), Position::new(2, 1))
        );
    }

    #[test]
    fn test_a_diagnostic_in_the_epilog_names_the_epilog_at_its_own_line() {
        // When
        let document = parse_decorated("", "Title\n=====\n", ".. nonsense::\n", NO_FILES);

        // Then
        assert_eq!(
            located(&document, DiagnosticCode::DirectiveUnknown),
            (Some("epilog.rst"), Position::new(1, 1))
        );
    }

    #[test]
    fn test_a_noqa_in_the_prolog_silences_only_the_prolog() {
        // When — the same unknown directive in the prolog, under a `.. noqa:`,
        // and in the document
        let document = parse_decorated(
            ".. noqa: directive.unknown\n.. nonsense::\n",
            ".. nonsense::\n",
            "",
            NO_FILES,
        );

        // Then — the prolog's is silenced, the document's is not
        let reported: Vec<_> =
            rinx_ast::retain_reportable(&document.diagnostics, &document.suppressions)
                .map(|diagnostic| diagnostic.span.and_then(|span| span.file))
                .collect();
        assert_eq!(document.diagnostics.len(), 2);
        assert_eq!(reported, [None]);
    }

    #[test]
    fn test_the_prolog_comes_after_the_documents_metadata() {
        // When
        let document = parse_decorated(
            ".. |a| replace:: A\n",
            ":orphan:\n\nAbout |a|.\n",
            "",
            NO_FILES,
        );

        // Then
        assert!(document.metadata.contains_key("orphan"));
        assert_eq!(codes(&document), Vec::new());
    }

    #[test]
    fn test_an_included_fragment_is_not_decorated_again() {
        // When — the prolog would define `|a|` twice if the fragment had it
        let document = parse_decorated(
            ".. |a| replace:: A\n",
            ".. include:: part.rst\n\nAbout |a|.\n",
            "",
            &[("part.rst", "Fragment.\n")],
        );

        // Then
        assert_eq!(codes(&document), Vec::new());
        assert_eq!(document.source_files, ["prolog.rst", "part.rst"]);
    }

    #[test]
    fn test_an_include_in_the_prolog_resolves_against_the_document() {
        // When
        let document = parse_decorated(
            ".. include:: part.rst\n",
            "Document.\n",
            "",
            &[("part.rst", "Fragment.\n")],
        );

        // Then
        assert_eq!(codes(&document), Vec::new());
        assert_eq!(document.source_files, ["prolog.rst", "part.rst"]);
    }

    #[test]
    fn test_an_empty_prolog_names_no_file() {
        // When
        let document = parse_decorated("\n", "Document.\n", "", NO_FILES);

        // Then
        assert!(document.source_files.is_empty());
        assert_eq!(document.nodes.len(), 1);
    }

    #[test]
    fn test_a_templated_document_keeps_its_positions_and_its_prolog_is_not_rendered() {
        // Given
        let loader = FakeFiles(HashMap::new());
        let context = [("release".to_string(), "3.14".to_string())];
        let ctx = ParseCtx::new(Domain::Py, &loader)
            .with_jinja(&context)
            .with_prolog(SplicedText {
                name: "prolog.rst",
                text: "Literally {{ release }}.\n",
            });

        // When
        let document = parse_with_ctx(
            "guide.rst",
            "Release {{ release }}.\n\n.. nonsense::\n",
            &ctx,
        );

        // Then
        assert_eq!(
            document.nodes[0],
            Node::Paragraph(vec![InlineNode::Text(
                "Literally {{ release }}.".to_string()
            )])
        );
        assert_eq!(
            located(&document, DiagnosticCode::DirectiveUnknown),
            (None, Position::new(3, 1))
        );
    }
}

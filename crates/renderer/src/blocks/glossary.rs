//! `glossary` directive rendering and the bare `.. index::` anchor.

use std::fmt::Write as _;

use crate::RenderCtx;

/// Renders a `glossary` directive as a definition list (`<dl>`).
pub(super) fn render_glossary(
    html: &mut String,
    entries: &[rusty_sphinx_ast::GlossaryEntry],
    ctx: &mut RenderCtx<'_>,
) {
    let _ = writeln!(html, "<dl class=\"glossary\">");
    for entry in entries {
        for term in &entry.terms {
            let term_escaped = html_escape::encode_text(term);
            let id = rusty_sphinx_ast::term_id(term);
            let id_attr = html_escape::encode_double_quoted_attribute(&id);
            let _ = writeln!(html, "  <dt id=\"{id_attr}\">{term_escaped}</dt>");
        }
        let _ = write!(html, "  <dd>");
        super::render_nodes(html, &entry.definition, ctx);
        let _ = writeln!(html, "</dd>");
    }
    let _ = writeln!(html, "</dl>");
}

/// Renders a `.. index::` directive as a bare, invisible anchor at its
/// document position — like `Node::Comment`, it produces no visible content;
/// the genindex page links here via `id`.
pub(super) fn render_index_anchor(html: &mut String, id: &str) {
    let id_attr = html_escape::encode_double_quoted_attribute(id);
    let _ = writeln!(html, "<span id=\"{id_attr}\"></span>");
}

#[cfg(test)]
mod tests {
    use rusty_sphinx_ast::{Directive, Document, InlineNode, Node, TargetName};
    use rusty_sphinx_index::ProjectIndex;

    fn render_doc(doc: &Document) -> String {
        let index = ProjectIndex::default();
        crate::render(doc, &index, &doc.path).html
    }

    #[test]
    fn test_render_literal_block_without_language() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::LiteralBlock {
                language: None,
                content: "def hello():\n    pass".to_string(),
            }],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert_eq!(result, "<pre><code>def hello():\n    pass</code></pre>\n");
    }
    #[test]
    fn test_render_literal_block_with_language() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::LiteralBlock {
                language: Some("python".to_string()),
                content: "x = 1".to_string(),
            }],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert_eq!(
            result,
            "<pre><code class=\"language-python\">x = 1</code></pre>\n"
        );
    }
    #[test]
    fn test_render_literal_block_escapes_html() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::LiteralBlock {
                language: None,
                content: "a < b && b > c".to_string(),
            }],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("a &lt; b &amp;&amp; b &gt; c"));
    }
    #[test]
    fn test_render_index_directive_produces_invisible_anchor() {
        // Given
        let doc = Document::new(
            "guide.rst".to_string(),
            vec![Node::Directive(Directive::Index {
                entries: vec![rusty_sphinx_ast::IndexEntry::Term {
                    primary: "execution".to_string(),
                    subentry: None,
                    main: false,
                }],
                id: "index-0".to_string(),
            })],
        );

        // When
        let result = render_doc(&doc);

        // Then — a bare anchor, no other visible content
        assert_eq!(result, "<span id=\"index-0\"></span>\n");
    }
    #[test]
    fn test_render_glossary_single_entry() {
        // Given
        let doc = Document::new(
            "glossary.rst".to_string(),
            vec![Node::Directive(Directive::Glossary {
                entries: vec![rusty_sphinx_ast::GlossaryEntry {
                    terms: vec!["environment".to_string()],
                    definition: vec![Node::Paragraph(vec![InlineNode::Text(
                        "A structure.".to_string(),
                    )])],
                }],
                sorted: false,
            })],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<dl class=\"glossary\">"));
        assert!(result.contains("<dt id=\"term-environment\">environment</dt>"));
        assert!(result.contains("<dd>"));
        assert!(result.contains("A structure."));
        assert!(result.contains("</dl>"));
    }
    #[test]
    fn test_render_glossary_multi_term_entry_produces_multiple_dt() {
        // Given
        let doc = Document::new(
            "glossary.rst".to_string(),
            vec![Node::Directive(Directive::Glossary {
                entries: vec![rusty_sphinx_ast::GlossaryEntry {
                    terms: vec!["term 1".to_string(), "term 2".to_string()],
                    definition: vec![Node::Paragraph(vec![InlineNode::Text(
                        "Shared.".to_string(),
                    )])],
                }],
                sorted: false,
            })],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<dt id=\"term-term-1\">term 1</dt>"));
        assert!(result.contains("<dt id=\"term-term-2\">term 2</dt>"));
        assert_eq!(result.matches("<dd>").count(), 1);
    }
    #[test]
    fn test_render_glossary_escapes_html_in_terms() {
        // Given
        let doc = Document::new(
            "glossary.rst".to_string(),
            vec![Node::Directive(Directive::Glossary {
                entries: vec![rusty_sphinx_ast::GlossaryEntry {
                    terms: vec!["a < b".to_string()],
                    definition: vec![],
                }],
                sorted: false,
            })],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("a &lt; b"));
        assert!(!result.contains("a < b"));
    }
    #[test]
    fn test_render_term_reference_resolved_links_to_glossary_doc() {
        // Given
        let doc = Document::new(
            "guide.rst".to_string(),
            vec![Node::Paragraph(vec![InlineNode::TermReference {
                display: "environment".to_string(),
                term: "environment".to_string(),
                span: None,
            }])],
        );
        let mut index = ProjectIndex::default();
        index
            .glossary_terms
            .insert(TargetName::new("environment"), "glossary.rst".to_string());

        // When
        let result = crate::render(&doc, &index, &doc.path).html;

        // Then
        assert!(result.contains("href=\"glossary.html#term-environment\""));
        assert!(result.contains("class=\"xref std std-term\""));
        assert!(result.contains(">environment<"));
    }
    #[test]
    fn test_render_term_reference_with_custom_display_text() {
        // Given
        let doc = Document::new(
            "guide.rst".to_string(),
            vec![Node::Paragraph(vec![InlineNode::TermReference {
                display: "the env".to_string(),
                term: "environment".to_string(),
                span: None,
            }])],
        );
        let mut index = ProjectIndex::default();
        index
            .glossary_terms
            .insert(TargetName::new("environment"), "glossary.rst".to_string());

        // When
        let result = crate::render(&doc, &index, &doc.path).html;

        // Then
        assert!(result.contains("href=\"glossary.html#term-environment\""));
        assert!(result.contains(">the env<"));
    }
    #[test]
    fn test_render_term_reference_broken_link_when_term_not_found() {
        // Given
        let doc = Document::new(
            "guide.rst".to_string(),
            vec![Node::Paragraph(vec![InlineNode::TermReference {
                display: "unknown".to_string(),
                term: "unknown".to_string(),
                span: None,
            }])],
        );
        let index = ProjectIndex::default(); // empty — no glossary terms

        // When
        let result = crate::render(&doc, &index, &doc.path).html;

        // Then
        assert!(result.contains("class=\"broken-link\""));
        assert!(result.contains(">unknown<"));
    }
    #[test]
    fn test_render_term_reference_computes_relative_path_across_directories() {
        // Given — document is in a subdirectory, glossary is at root
        let doc = Document::new(
            "guide/page.rst".to_string(),
            vec![Node::Paragraph(vec![InlineNode::TermReference {
                display: "foo".to_string(),
                term: "foo".to_string(),
                span: None,
            }])],
        );
        let mut index = ProjectIndex::default();
        index
            .glossary_terms
            .insert(TargetName::new("foo"), "glossary.rst".to_string());

        // When
        let result = crate::render(&doc, &index, &doc.path).html;

        // Then — href should traverse up one directory
        assert!(result.contains("href=\"../glossary.html#term-foo\""));
    }
}

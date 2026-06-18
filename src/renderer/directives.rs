//! Directive-specific rendering helpers (admonitions, version changes, see-also, glossary).

use crate::ast::Node;
use std::fmt::Write as _;

use super::RenderCtx;

/// Renders an admonition directive (note, warning, hint, etc.) as HTML.
pub(super) fn render_admonition(
    html: &mut String,
    kind: crate::ast::AdmonitionKind,
    title: Option<&str>,
    collapsible: Option<bool>,
    body: &[Node],
    ctx: &mut RenderCtx<'_>,
) {
    let kind_str = kind.as_str();
    let title_text = title.map_or_else(
        || {
            let mut chars = kind_str.chars();
            chars.next().map_or_else(String::new, |c| {
                c.to_uppercase().collect::<String>() + chars.as_str()
            })
        },
        String::from,
    );

    let kind_escaped = html_escape::encode_text(kind_str);
    let title_escaped = html_escape::encode_text(&title_text);

    if let Some(open) = collapsible {
        let open_attr = if open { " open" } else { "" };
        let _ = writeln!(
            html,
            "<details class=\"admonition {kind_escaped}\"{open_attr}>"
        );
        let _ = writeln!(
            html,
            "  <summary class=\"admonition-title\">{title_escaped}</summary>"
        );
        super::render_nodes(html, body, ctx);
        let _ = writeln!(html, "</details>");
    } else {
        let _ = writeln!(html, "<div class=\"admonition {kind_escaped}\">");
        let _ = writeln!(html, "  <p class=\"admonition-title\">{title_escaped}</p>");
        super::render_nodes(html, body, ctx);
        let _ = writeln!(html, "</div>");
    }
}

/// Renders a versionadded / versionchanged / deprecated directive as HTML.
pub(super) fn render_version_change(
    html: &mut String,
    kind: crate::ast::VersionChangeKind,
    version: &str,
    body: &[Node],
    ctx: &mut RenderCtx<'_>,
) {
    let kind_str = kind.as_str();
    let version_escaped = html_escape::encode_text(version);

    let label = match kind {
        crate::ast::VersionChangeKind::Added => format!("New in version {version_escaped}:"),
        crate::ast::VersionChangeKind::Changed => {
            format!("Changed in version {version_escaped}:")
        }
        crate::ast::VersionChangeKind::Deprecated => {
            format!("Deprecated since version {version_escaped}:")
        }
    };

    let inner_class = match kind {
        crate::ast::VersionChangeKind::Added => "added",
        crate::ast::VersionChangeKind::Changed => "changed",
        crate::ast::VersionChangeKind::Deprecated => "deprecated",
    };

    let _ = writeln!(html, "<div class=\"{kind_str}\">");
    let _ = write!(html, "  <p class=\"versionmodified {inner_class}\">");
    let _ = write!(html, "<span class=\"versionmodified-label\">{label}</span>");

    if body.is_empty() {
        let _ = writeln!(html, "</p>");
        let _ = writeln!(html, "</div>");
        return;
    }

    let _ = writeln!(html, "</p>");
    super::render_nodes(html, body, ctx);
    let _ = writeln!(html, "</div>");
}

/// Renders a `seealso` directive as an admonition-style HTML block.
pub(super) fn render_seealso(html: &mut String, body: &[Node], ctx: &mut RenderCtx<'_>) {
    let _ = writeln!(html, "<div class=\"admonition seealso\">");
    let _ = writeln!(html, "  <p class=\"admonition-title\">See also</p>");
    super::render_nodes(html, body, ctx);
    let _ = writeln!(html, "</div>");
}

/// Renders a `glossary` directive as a definition list (`<dl>`).
pub(super) fn render_glossary(
    html: &mut String,
    entries: &[crate::ast::GlossaryEntry],
    ctx: &mut RenderCtx<'_>,
) {
    let _ = writeln!(html, "<dl class=\"glossary\">");
    for entry in entries {
        for term in &entry.terms {
            let term_escaped = html_escape::encode_text(term);
            let id = crate::ast::term_id(term);
            let id_attr = html_escape::encode_double_quoted_attribute(&id);
            let _ = writeln!(html, "  <dt id=\"{id_attr}\">{term_escaped}</dt>");
        }
        let _ = write!(html, "  <dd>");
        super::render_nodes(html, &entry.definition, ctx);
        let _ = writeln!(html, "</dd>");
    }
    let _ = writeln!(html, "</dl>");
}

#[cfg(test)]
mod tests {
    use super::super::RenderCtx;
    use super::*;
    use crate::analyzer::ProjectIndex;
    use crate::ast::{Directive, Document, InlineNode, Node, TargetName};

    fn render_doc(doc: &Document) -> String {
        let index = ProjectIndex::default();
        crate::renderer::render(doc, &index, &doc.path)
    }

    #[test]
    fn test_render_formats_admonition() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::Admonition {
                kind: crate::ast::AdmonitionKind::Note,
                title: None,
                collapsible: None,
                body: vec![Node::Paragraph(vec![InlineNode::Text(
                    "Note body".to_string(),
                )])],
            })],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<div class=\"admonition note\">"));
        assert!(result.contains("<p class=\"admonition-title\">Note</p>"));
        assert!(result.contains("<p>Note body</p>"));
    }

    #[test]
    fn test_render_formats_collapsible_admonition() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::Admonition {
                kind: crate::ast::AdmonitionKind::Warning,
                title: Some("Custom Warning".to_string()),
                collapsible: Some(false),
                body: vec![Node::Paragraph(vec![InlineNode::Text(
                    "Warning body".to_string(),
                )])],
            })],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<details class=\"admonition warning\">"));
        assert!(result.contains("<summary class=\"admonition-title\">Custom Warning</summary>"));
        assert!(result.contains("<p>Warning body</p>"));
    }

    #[test]
    fn test_render_admonition_static() {
        // Given
        let mut html = String::new();
        let kind = crate::ast::AdmonitionKind::Note;
        let title: Option<String> = None;
        let collapsible: Option<bool> = None;
        let body = vec![Node::Paragraph(vec![InlineNode::Text("Body".to_string())])];
        let index = ProjectIndex::default();
        let anon_targets = vec![];
        let mut anon_index = 0;
        let mut ctx = RenderCtx {
            index: &index,
            doc_path: "test.rst",
            anon_targets: &anon_targets,
            anon_index: &mut anon_index,
            original_doc_path: "test.rst",
        };

        // When
        render_admonition(
            &mut html,
            kind,
            title.as_deref(),
            collapsible,
            &body,
            &mut ctx,
        );

        // Then
        assert!(html.contains("<div class=\"admonition note\""));
        assert!(html.contains("<p class=\"admonition-title\">Note</p>"));
        assert!(html.contains("<p>Body</p>"));
    }

    #[test]
    fn test_render_admonition_collapsible_open() {
        // Given
        let mut html = String::new();
        let kind = crate::ast::AdmonitionKind::Warning;
        let title = Some("Custom Title".to_string());
        let collapsible = Some(true);
        let body = vec![];
        let index = ProjectIndex::default();
        let anon_targets = vec![];
        let mut anon_index = 0;
        let mut ctx = RenderCtx {
            index: &index,
            doc_path: "test.rst",
            anon_targets: &anon_targets,
            anon_index: &mut anon_index,
            original_doc_path: "test.rst",
        };

        // When
        render_admonition(
            &mut html,
            kind,
            title.as_deref(),
            collapsible,
            &body,
            &mut ctx,
        );

        // Then
        assert!(html.contains("<details class=\"admonition warning\" open>"));
        assert!(html.contains("<summary class=\"admonition-title\">Custom Title</summary>"));
    }

    #[test]
    fn test_render_versionadded_produces_correct_html() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::VersionChange {
                kind: crate::ast::VersionChangeKind::Added,
                version: "1.0".to_string(),
                body: vec![Node::Paragraph(vec![InlineNode::Text(
                    "Initial release.".to_string(),
                )])],
            })],
        );

        // When
        let result = render_doc(&doc);

        // Then
        let expected = "<div class=\"versionadded\">\n  <p class=\"versionmodified added\"><span class=\"versionmodified-label\">New in version 1.0:</span></p>\n<p>Initial release.</p>\n</div>\n";
        assert_eq!(result, expected);
    }

    #[test]
    fn test_render_deprecated_produces_correct_html() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::VersionChange {
                kind: crate::ast::VersionChangeKind::Deprecated,
                version: "3.0".to_string(),
                body: vec![Node::Paragraph(vec![InlineNode::Text(
                    "Use new API.".to_string(),
                )])],
            })],
        );

        // When
        let result = render_doc(&doc);

        // Then
        let expected = "<div class=\"deprecated\">\n  <p class=\"versionmodified deprecated\"><span class=\"versionmodified-label\">Deprecated since version 3.0:</span></p>\n<p>Use new API.</p>\n</div>\n";
        assert_eq!(result, expected);
    }

    #[test]
    fn test_render_versionchanged_with_empty_body() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::VersionChange {
                kind: crate::ast::VersionChangeKind::Changed,
                version: "2.0".to_string(),
                body: vec![],
            })],
        );

        // When
        let result = render_doc(&doc);

        // Then
        let expected = "<div class=\"versionchanged\">\n  <p class=\"versionmodified changed\"><span class=\"versionmodified-label\">Changed in version 2.0:</span></p>\n</div>\n";
        assert_eq!(result, expected);
    }

    #[test]
    fn test_render_formats_seealso_with_title_and_body() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::SeeAlso {
                body: vec![Node::Paragraph(vec![InlineNode::Text(
                    "The other page.".to_string(),
                )])],
            })],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<div class=\"admonition seealso\">"));
        assert!(result.contains("<p class=\"admonition-title\">See also</p>"));
        assert!(result.contains("<p>The other page.</p>"));
        assert!(result.contains("</div>"));
    }

    #[test]
    fn test_render_seealso_static() {
        // Given
        let mut html = String::new();
        let body = vec![Node::Paragraph(vec![InlineNode::Text(
            "See related.".to_string(),
        )])];
        let index = ProjectIndex::default();
        let anon_targets = vec![];
        let mut anon_index = 0;
        let mut ctx = RenderCtx {
            index: &index,
            doc_path: "test.rst",
            anon_targets: &anon_targets,
            anon_index: &mut anon_index,
            original_doc_path: "test.rst",
        };

        // When
        render_seealso(&mut html, &body, &mut ctx);

        // Then
        assert!(html.contains("<div class=\"admonition seealso\""));
        assert!(html.contains("<p class=\"admonition-title\">See also</p>"));
        assert!(html.contains("<p>See related.</p>"));
        assert!(html.contains("</div>"));
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
    fn test_render_glossary_single_entry() {
        // Given
        let doc = Document::new(
            "glossary.rst".to_string(),
            vec![Node::Directive(Directive::Glossary {
                entries: vec![crate::ast::GlossaryEntry {
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
                entries: vec![crate::ast::GlossaryEntry {
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
                entries: vec![crate::ast::GlossaryEntry {
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
            }])],
        );
        let mut index = ProjectIndex::default();
        index
            .glossary_terms
            .insert(TargetName::new("environment"), "glossary.rst".to_string());

        // When
        let result = crate::renderer::render(&doc, &index, &doc.path);

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
            }])],
        );
        let mut index = ProjectIndex::default();
        index
            .glossary_terms
            .insert(TargetName::new("environment"), "glossary.rst".to_string());

        // When
        let result = crate::renderer::render(&doc, &index, &doc.path);

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
            }])],
        );
        let index = ProjectIndex::default(); // empty — no glossary terms

        // When
        let result = crate::renderer::render(&doc, &index, &doc.path);

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
            }])],
        );
        let mut index = ProjectIndex::default();
        index
            .glossary_terms
            .insert(TargetName::new("foo"), "glossary.rst".to_string());

        // When
        let result = crate::renderer::render(&doc, &index, &doc.path);

        // Then — href should traverse up one directory
        assert!(result.contains("href=\"../glossary.html#term-foo\""));
    }
}

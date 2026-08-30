//! Admonition-family directive rendering: `note`/`warning`/etc.,
//! `versionadded`/`versionchanged`/`deprecated`, and `seealso`.

use rusty_sphinx_ast::Node;
use std::fmt::Write as _;

use crate::RenderCtx;

/// Renders an admonition directive (note, warning, hint, etc.) as HTML.
pub(super) fn render_admonition(
    html: &mut String,
    kind: rusty_sphinx_ast::AdmonitionKind,
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
    kind: rusty_sphinx_ast::VersionChangeKind,
    version: &str,
    body: &[Node],
    ctx: &mut RenderCtx<'_>,
) {
    let kind_str = kind.as_str();
    let version_escaped = html_escape::encode_text(version);

    let label = match kind {
        rusty_sphinx_ast::VersionChangeKind::Added => format!("New in version {version_escaped}:"),
        rusty_sphinx_ast::VersionChangeKind::Changed => {
            format!("Changed in version {version_escaped}:")
        }
        rusty_sphinx_ast::VersionChangeKind::Deprecated => {
            format!("Deprecated since version {version_escaped}:")
        }
    };

    let inner_class = match kind {
        rusty_sphinx_ast::VersionChangeKind::Added => "added",
        rusty_sphinx_ast::VersionChangeKind::Changed => "changed",
        rusty_sphinx_ast::VersionChangeKind::Deprecated => "deprecated",
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

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_sphinx_ast::{Directive, Document, InlineNode, TargetSearchOrder};
    use rusty_sphinx_index::ProjectIndex;

    fn render_doc(doc: &Document) -> String {
        let index = ProjectIndex::default();
        crate::render(doc, &index, &doc.path).html
    }

    #[test]
    fn test_render_formats_admonition() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::Admonition {
                kind: rusty_sphinx_ast::AdmonitionKind::Note,
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
                kind: rusty_sphinx_ast::AdmonitionKind::Warning,
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
        let kind = rusty_sphinx_ast::AdmonitionKind::Note;
        let title: Option<String> = None;
        let collapsible: Option<bool> = None;
        let body = vec![Node::Paragraph(vec![InlineNode::Text("Body".to_string())])];
        let index = ProjectIndex::default();
        let anon_targets = vec![];
        let mut anon_index = 0;
        let resolver = crate::resolution::DomainObjectResolver::new(&index);
        let option_resolver = crate::resolution::OptionResolver::new(&index);
        let mut ctx = RenderCtx {
            index: &index,
            domain_resolver: &resolver,
            option_resolver: &option_resolver,
            doc_path: "test.rst",
            anon_targets: &anon_targets,
            anon_index: &mut anon_index,
            original_doc_path: "test.rst",
            broken_links: &mut Vec::new(),
            object_type_mismatches: &mut Vec::new(),
            scope: rusty_sphinx_scope::Scope::default(),
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
        let kind = rusty_sphinx_ast::AdmonitionKind::Warning;
        let title = Some("Custom Title".to_string());
        let collapsible = Some(true);
        let body = vec![];
        let index = ProjectIndex::default();
        let anon_targets = vec![];
        let mut anon_index = 0;
        let resolver = crate::resolution::DomainObjectResolver::new(&index);
        let option_resolver = crate::resolution::OptionResolver::new(&index);
        let mut ctx = RenderCtx {
            index: &index,
            domain_resolver: &resolver,
            option_resolver: &option_resolver,
            doc_path: "test.rst",
            anon_targets: &anon_targets,
            anon_index: &mut anon_index,
            original_doc_path: "test.rst",
            broken_links: &mut Vec::new(),
            object_type_mismatches: &mut Vec::new(),
            scope: rusty_sphinx_scope::Scope::default(),
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
                kind: rusty_sphinx_ast::VersionChangeKind::Added,
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
                kind: rusty_sphinx_ast::VersionChangeKind::Deprecated,
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
                kind: rusty_sphinx_ast::VersionChangeKind::Changed,
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
    fn test_render_formats_seealso_with_definition_list_body() {
        // Given a seealso body containing a definition list, matching the
        // CPython benchmark's `curses` seealso block
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::SeeAlso {
                body: vec![Node::DefinitionList {
                    items: vec![
                        rusty_sphinx_ast::DefinitionListItem {
                            term: vec![
                                InlineNode::Text("Module ".to_string()),
                                InlineNode::DomainObjectReference {
                                    object_type: rusty_sphinx_ast::ObjectType::Py(
                                        rusty_sphinx_ast::PyObjectType::Module,
                                    ),
                                    name: "curses.ascii".to_string(),
                                    display: "curses.ascii".to_string(),
                                    link: true,
                                    search_order: TargetSearchOrder::LeastQualifiedFirst,
                                    span: None,
                                },
                            ],
                            definition: vec![Node::Paragraph(vec![InlineNode::Text(
                                "Utilities for working with ASCII characters.".to_string(),
                            )])],
                        },
                        rusty_sphinx_ast::DefinitionListItem {
                            term: vec![InlineNode::Reference {
                                display: "curses-howto".to_string(),
                                target: "curses-howto".to_string(),
                                span: None,
                            }],
                            definition: vec![Node::Paragraph(vec![InlineNode::Text(
                                "Tutorial material.".to_string(),
                            )])],
                        },
                    ],
                }],
            })],
        );

        // When
        let result = render_doc(&doc);

        // Then the seealso admonition wraps a proper <dl>/<dt>/<dd> structure
        assert!(result.contains("<div class=\"admonition seealso\">"));
        assert!(result.contains("<dl>"));
        assert!(result.contains("<dt>Module "));
        assert!(result.contains("curses.ascii"));
        assert!(result.contains("<dd><p>Utilities for working with ASCII characters.</p>\n</dd>"));
        assert!(result.contains("</dl>"));
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
        let resolver = crate::resolution::DomainObjectResolver::new(&index);
        let option_resolver = crate::resolution::OptionResolver::new(&index);
        let mut ctx = RenderCtx {
            index: &index,
            domain_resolver: &resolver,
            option_resolver: &option_resolver,
            doc_path: "test.rst",
            anon_targets: &anon_targets,
            anon_index: &mut anon_index,
            original_doc_path: "test.rst",
            broken_links: &mut Vec::new(),
            object_type_mismatches: &mut Vec::new(),
            scope: rusty_sphinx_scope::Scope::default(),
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
    fn test_render_formats_py_module_platform_synopsis_and_deprecated() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyModule {
                    name: "greetings".to_string(),
                    platform: Some("Unix, Windows".to_string()),
                    synopsis: Some("Greeting utilities.".to_string()),
                    deprecated: true,
                    body: vec![],
                },
            ))],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<p class=\"platform\">Platform: Unix, Windows</p>"));
        assert!(result.contains("<p class=\"synopsis\">Greeting utilities.</p>"));
        assert!(result.contains("<p class=\"deprecated\">Deprecated.</p>"));
    }
}

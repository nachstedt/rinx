//! Directive-specific rendering helpers (admonitions, version changes, see-also, glossary).

use rusty_sphinx_ast::Node;
use std::fmt::Write as _;

use super::RenderCtx;

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

/// Renders a domain object directive (e.g. `.. py:function::`, `.. c:function::`,
/// `.. py:module::`, `.. py:data::`) as a Sphinx-style object description
/// (`<dl class="{domain} {objtype}">`), using the same qualified key as the
/// analyzer for the anchor `id`.
///
/// The shared `<dl>`/`<dt>` wrapper and cross-reference key are built
/// generically via `obj`'s accessors; any option specific to one object type
/// (`py:module`'s `platform`/`synopsis`/`deprecated` and `py:data`'s
/// `type`/`value` — real Sphinx has no equivalent module/data index page
/// here, so they're rendered inline as leading `<dd>` paragraphs rather than
/// dropped) is matched explicitly, so adding a new object type with its own
/// options can't be forgotten here.
pub(super) fn render_domain_object(
    html: &mut String,
    obj: &rusty_sphinx_ast::DomainObjectBody,
    ctx: &mut RenderCtx<'_>,
) {
    let object_type = obj.object_type();
    let own_name = obj.name();
    let qualifier = rusty_sphinx_ast::effective_qualifier(
        ctx.class_stack.last().map(String::as_str),
        ctx.current_module.as_deref(),
        object_type.domain(),
    );
    let qualified_name = rusty_sphinx_ast::qualify_name(qualifier, &own_name);
    let key = rusty_sphinx_ast::build_domain_object_key(object_type, &qualified_name);
    if matches!(obj, rusty_sphinx_ast::DomainObjectBody::PyModule { .. }) {
        ctx.current_module = Some(qualified_name.clone());
    }
    let domain_str = object_type.domain().as_str();
    let objtype_str = object_type.as_str();
    let id_attr = html_escape::encode_double_quoted_attribute(key.as_str());
    let sig_escaped = html_escape::encode_text(obj.signature_text());

    let _ = writeln!(html, "<dl class=\"{domain_str} {objtype_str}\">");
    let _ = write!(html, "  <dt id=\"{id_attr}\">");
    for label in domain_object_prefix_labels(obj) {
        let _ = write!(html, "<em class=\"property\">{label}</em> ");
    }
    let _ = writeln!(html, "<code class=\"sig-name\">{sig_escaped}</code></dt>");
    let _ = write!(html, "  <dd>");
    render_domain_object_options(html, obj);
    if matches!(
        obj,
        rusty_sphinx_ast::DomainObjectBody::PyClass { .. }
            | rusty_sphinx_ast::DomainObjectBody::PyException { .. }
    ) {
        ctx.class_stack.push(qualified_name);
        super::render_nodes(html, obj.body(), ctx);
        ctx.class_stack.pop();
    } else {
        super::render_nodes(html, obj.body(), ctx);
    }
    let _ = writeln!(html, "</dd>");
    let _ = writeln!(html, "</dl>");
}

/// Canonical, deterministic prefix-label order for a domain object's `<dt>`
/// (e.g. `abstractmethod`/`async`/`classmethod`/`staticmethod` for
/// `py:method`, `final`/`class` for `py:class`) — independent of how the
/// author wrote the option flags.
fn domain_object_prefix_labels(obj: &rusty_sphinx_ast::DomainObjectBody) -> Vec<&'static str> {
    match obj {
        rusty_sphinx_ast::DomainObjectBody::PyMethod {
            is_classmethod,
            is_staticmethod,
            is_abstractmethod,
            is_async,
            ..
        } => [
            (*is_abstractmethod, "abstractmethod"),
            (*is_async, "async"),
            (*is_classmethod, "classmethod"),
            (*is_staticmethod, "staticmethod"),
        ]
        .into_iter()
        .filter_map(|(active, label)| active.then_some(label))
        .collect(),
        rusty_sphinx_ast::DomainObjectBody::PyClass { is_final, .. } => {
            class_like_prefix_labels(*is_final, "class")
        }
        rusty_sphinx_ast::DomainObjectBody::PyException { is_final, .. } => {
            class_like_prefix_labels(*is_final, "exception")
        }
        _ => Vec::new(),
    }
}

/// Shared prefix-label construction for `py:class`/`py:exception` — both
/// objects have the same `is_final` option and only differ in the trailing
/// label naming the object type (`"class"` vs `"exception"`).
fn class_like_prefix_labels(is_final: bool, kind_label: &'static str) -> Vec<&'static str> {
    let mut labels = Vec::new();
    if is_final {
        labels.push("final");
    }
    labels.push(kind_label);
    labels
}

/// Renders a domain object's type-specific options (`py:module`'s
/// `platform`/`synopsis`/`deprecated`, `py:data`'s `type`/`value`,
/// `py:attribute`'s `type`/`value`/`canonical`) as leading `<dd>` paragraphs.
/// Object types with no such options (`py:function`, `c:function`,
/// `c:macro`, `py:method`, `py:class`, `py:exception`) render nothing here.
fn render_domain_object_options(html: &mut String, obj: &rusty_sphinx_ast::DomainObjectBody) {
    match obj {
        rusty_sphinx_ast::DomainObjectBody::PyModule {
            platform,
            synopsis,
            deprecated,
            ..
        } => {
            if let Some(platform) = platform {
                let _ = write!(
                    html,
                    "<p class=\"platform\">Platform: {}</p>",
                    html_escape::encode_text(platform)
                );
            }
            if let Some(synopsis) = synopsis {
                let _ = write!(
                    html,
                    "<p class=\"synopsis\">{}</p>",
                    html_escape::encode_text(synopsis)
                );
            }
            if *deprecated {
                let _ = write!(html, "<p class=\"deprecated\">Deprecated.</p>");
            }
        }
        rusty_sphinx_ast::DomainObjectBody::PyData { type_, value, .. } => {
            if let Some(type_) = type_ {
                let _ = write!(
                    html,
                    "<p class=\"type\">Type: {}</p>",
                    html_escape::encode_text(type_)
                );
            }
            if let Some(value) = value {
                let _ = write!(
                    html,
                    "<p class=\"value\">Value: {}</p>",
                    html_escape::encode_text(value)
                );
            }
        }
        rusty_sphinx_ast::DomainObjectBody::PyAttribute {
            type_,
            value,
            canonical,
            ..
        } => {
            if let Some(type_) = type_ {
                let _ = write!(
                    html,
                    "<p class=\"type\">Type: {}</p>",
                    html_escape::encode_text(type_)
                );
            }
            if let Some(value) = value {
                let _ = write!(
                    html,
                    "<p class=\"value\">Value: {}</p>",
                    html_escape::encode_text(value)
                );
            }
            if let Some(canonical) = canonical {
                let _ = write!(
                    html,
                    "<p class=\"canonical\">Canonical: {}</p>",
                    html_escape::encode_text(canonical)
                );
            }
        }
        rusty_sphinx_ast::DomainObjectBody::PyFunction { .. }
        | rusty_sphinx_ast::DomainObjectBody::CFunction { .. }
        | rusty_sphinx_ast::DomainObjectBody::CMacro { .. }
        | rusty_sphinx_ast::DomainObjectBody::PyMethod { .. }
        | rusty_sphinx_ast::DomainObjectBody::PyClass { .. }
        | rusty_sphinx_ast::DomainObjectBody::PyException { .. } => {}
    }
}

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
    use super::super::RenderCtx;
    use super::*;
    use rusty_sphinx_analyzer::ProjectIndex;
    use rusty_sphinx_ast::{Directive, Document, InlineNode, Node, TargetName};

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
        let mut ctx = RenderCtx {
            index: &index,
            doc_path: "test.rst",
            anon_targets: &anon_targets,
            anon_index: &mut anon_index,
            original_doc_path: "test.rst",
            broken_links: &mut Vec::new(),
            class_stack: Vec::new(),
            current_module: None,
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
        let mut ctx = RenderCtx {
            index: &index,
            doc_path: "test.rst",
            anon_targets: &anon_targets,
            anon_index: &mut anon_index,
            original_doc_path: "test.rst",
            broken_links: &mut Vec::new(),
            class_stack: Vec::new(),
            current_module: None,
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
                                },
                            ],
                            definition: vec![Node::Paragraph(vec![InlineNode::Text(
                                "Utilities for working with ASCII characters.".to_string(),
                            )])],
                        },
                        rusty_sphinx_ast::DefinitionListItem {
                            term: vec![InlineNode::Reference("curses-howto".to_string())],
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
        let mut ctx = RenderCtx {
            index: &index,
            doc_path: "test.rst",
            anon_targets: &anon_targets,
            anon_index: &mut anon_index,
            original_doc_path: "test.rst",
            broken_links: &mut Vec::new(),
            class_stack: Vec::new(),
            current_module: None,
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
    fn test_render_formats_py_function_domain_object() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyFunction {
                    signature: "greet(name)".to_string(),
                    body: vec![Node::Paragraph(vec![InlineNode::Text(
                        "Greets the given name.".to_string(),
                    )])],
                },
            ))],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<dl class=\"py function\">"));
        assert!(result.contains("<dt id=\"py:function:greet\">"));
        assert!(result.contains("<code class=\"sig-name\">greet(name)</code>"));
        assert!(result.contains("<p>Greets the given name.</p>"));
    }

    #[test]
    fn test_render_formats_c_function_domain_object() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::CFunction {
                    signature: "int add(int a, int b)".to_string(),
                    body: vec![Node::Paragraph(vec![InlineNode::Text(
                        "Adds two numbers.".to_string(),
                    )])],
                },
            ))],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<dl class=\"c function\">"));
        assert!(result.contains("<dt id=\"c:function:add\">"));
        assert!(result.contains("<code class=\"sig-name\">int add(int a, int b)</code>"));
    }

    #[test]
    fn test_render_formats_c_macro_domain_object() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::CMacro {
                    signature: "MAX(a, b)".to_string(),
                    body: vec![Node::Paragraph(vec![InlineNode::Text(
                        "Expands to whichever of a or b is greater.".to_string(),
                    )])],
                },
            ))],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<dl class=\"c macro\">"));
        assert!(result.contains("<dt id=\"c:macro:max\">"));
        assert!(result.contains("<code class=\"sig-name\">MAX(a, b)</code>"));
    }

    #[test]
    fn test_render_formats_py_module_domain_object() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyModule {
                    name: "greetings".to_string(),
                    platform: None,
                    synopsis: None,
                    deprecated: false,
                    body: vec![Node::Paragraph(vec![InlineNode::Text(
                        "A module of greetings.".to_string(),
                    )])],
                },
            ))],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<dl class=\"py module\">"));
        assert!(result.contains("<dt id=\"py:module:greetings\">"));
        assert!(result.contains("<code class=\"sig-name\">greetings</code>"));
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

    #[test]
    fn test_render_formats_py_method_domain_object() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyMethod {
                    signature: "greet(self, name)".to_string(),
                    is_classmethod: false,
                    is_staticmethod: false,
                    is_abstractmethod: false,
                    is_async: false,
                    body: vec![Node::Paragraph(vec![InlineNode::Text(
                        "Greets the given name.".to_string(),
                    )])],
                },
            ))],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<dl class=\"py method\">"));
        assert!(result.contains("<dt id=\"py:method:greet\">"));
        assert!(result.contains("<code class=\"sig-name\">greet(self, name)</code>"));
        assert!(!result.contains("class=\"property\""));
    }

    #[test]
    fn test_render_py_method_modifier_prefixes_in_canonical_order() {
        // Given — options written out of canonical order
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyMethod {
                    signature: "create(cls)".to_string(),
                    is_classmethod: true,
                    is_staticmethod: false,
                    is_abstractmethod: true,
                    is_async: false,
                    body: vec![],
                },
            ))],
        );

        // When
        let result = render_doc(&doc);

        // Then — abstractmethod is rendered before classmethod regardless of
        // struct-field/author order
        let abstractmethod_pos = result.find("abstractmethod").unwrap();
        let classmethod_pos = result.find("classmethod").unwrap();
        assert!(abstractmethod_pos < classmethod_pos);
        assert!(result.contains("<em class=\"property\">abstractmethod</em>"));
        assert!(result.contains("<em class=\"property\">classmethod</em>"));
        assert!(!result.contains("staticmethod"));
        assert!(!result.contains(">async<"));
    }

    #[test]
    fn test_render_formats_py_class_domain_object() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyClass {
                    signature: "greeter".to_string(),
                    is_final: false,
                    body: vec![Node::Paragraph(vec![InlineNode::Text(
                        "A greeter.".to_string(),
                    )])],
                },
            ))],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<dl class=\"py class\">"));
        assert!(result.contains("<dt id=\"py:class:greeter\">"));
        assert!(result.contains("<em class=\"property\">class</em>"));
        assert!(result.contains("<code class=\"sig-name\">greeter</code>"));
        assert!(!result.contains("final"));
    }

    #[test]
    fn test_render_py_class_final_prefix_precedes_class_prefix() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyClass {
                    signature: "greeter".to_string(),
                    is_final: true,
                    body: vec![],
                },
            ))],
        );

        // When
        let result = render_doc(&doc);

        // Then
        let final_pos = result.find("final").unwrap();
        let class_pos = result.find("class</em>").unwrap();
        assert!(final_pos < class_pos);
        assert!(result.contains("<em class=\"property\">final</em>"));
    }

    #[test]
    fn test_render_qualifies_method_nested_in_class_id() {
        // Given — a `py:method` nested inside a `py:class` body
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyClass {
                    signature: "greeter".to_string(),
                    is_final: false,
                    body: vec![Node::Directive(Directive::DomainObject(
                        rusty_sphinx_ast::DomainObjectBody::PyMethod {
                            signature: "greet(self, name)".to_string(),
                            is_classmethod: false,
                            is_staticmethod: false,
                            is_abstractmethod: false,
                            is_async: false,
                            body: vec![],
                        },
                    ))],
                },
            ))],
        );

        // When
        let result = render_doc(&doc);

        // Then — the nested method's id and signature are qualified/plain
        // respectively, matching the analyzer's index key exactly.
        assert!(result.contains("<dt id=\"py:method:greeter.greet\">"));
        assert!(result.contains("<code class=\"sig-name\">greet(self, name)</code>"));
    }

    #[test]
    fn test_render_qualifies_nested_classes_two_levels_deep() {
        // Given — a class nested inside another class, each containing a method
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyClass {
                    signature: "outer".to_string(),
                    is_final: false,
                    body: vec![Node::Directive(Directive::DomainObject(
                        rusty_sphinx_ast::DomainObjectBody::PyClass {
                            signature: "inner".to_string(),
                            is_final: false,
                            body: vec![Node::Directive(Directive::DomainObject(
                                rusty_sphinx_ast::DomainObjectBody::PyMethod {
                                    signature: "method(self)".to_string(),
                                    is_classmethod: false,
                                    is_staticmethod: false,
                                    is_abstractmethod: false,
                                    is_async: false,
                                    body: vec![],
                                },
                            ))],
                        },
                    ))],
                },
            ))],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<dt id=\"py:class:outer.inner\">"));
        assert!(result.contains("<dt id=\"py:method:outer.inner.method\">"));
    }

    #[test]
    fn test_render_qualifies_sibling_function_after_module_id() {
        // Given — the real-world CPython shape: `py:module` and the
        // `py:function` it documents are siblings, not nested.
        let doc = Document::new(
            "test.rst".to_string(),
            vec![
                Node::Directive(Directive::DomainObject(
                    rusty_sphinx_ast::DomainObjectBody::PyModule {
                        name: "types".to_string(),
                        platform: None,
                        synopsis: None,
                        deprecated: false,
                        body: vec![],
                    },
                )),
                Node::Directive(Directive::DomainObject(
                    rusty_sphinx_ast::DomainObjectBody::PyFunction {
                        signature: "coroutine(gen_func)".to_string(),
                        body: vec![],
                    },
                )),
            ],
        );

        // When
        let result = render_doc(&doc);

        // Then — the function's anchor id is module-qualified, matching the
        // analyzer's index key for `:func:`types.coroutine``.
        assert!(result.contains("<dt id=\"py:function:types.coroutine\">"));
        assert!(result.contains("<code class=\"sig-name\">coroutine(gen_func)</code>"));
    }

    #[test]
    fn test_render_composes_module_and_class_qualifiers_in_id() {
        // Given — a class documented as a sibling after `py:module`, with a
        // method nested inside the class — both qualifiers must compose.
        let doc = Document::new(
            "test.rst".to_string(),
            vec![
                Node::Directive(Directive::DomainObject(
                    rusty_sphinx_ast::DomainObjectBody::PyModule {
                        name: "types".to_string(),
                        platform: None,
                        synopsis: None,
                        deprecated: false,
                        body: vec![],
                    },
                )),
                Node::Directive(Directive::DomainObject(
                    rusty_sphinx_ast::DomainObjectBody::PyClass {
                        signature: "DynamicClassAttribute".to_string(),
                        is_final: false,
                        body: vec![Node::Directive(Directive::DomainObject(
                            rusty_sphinx_ast::DomainObjectBody::PyMethod {
                                signature: "__get__(self, instance, owner)".to_string(),
                                is_classmethod: false,
                                is_staticmethod: false,
                                is_abstractmethod: false,
                                is_async: false,
                                body: vec![],
                            },
                        ))],
                    },
                )),
            ],
        );

        // When
        let result = render_doc(&doc);

        // Then — id is lowercased, matching `TargetName`'s normalization
        assert!(result.contains("<dt id=\"py:class:types.dynamicclassattribute\">"));
        assert!(result.contains("<dt id=\"py:method:types.dynamicclassattribute.__get__\">"));
    }

    #[test]
    fn test_render_formats_py_exception_domain_object() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyException {
                    signature: "greetererror".to_string(),
                    is_final: false,
                    body: vec![Node::Paragraph(vec![InlineNode::Text(
                        "Raised when greeting fails.".to_string(),
                    )])],
                },
            ))],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<dl class=\"py exception\">"));
        assert!(result.contains("<dt id=\"py:exception:greetererror\">"));
        assert!(result.contains("<em class=\"property\">exception</em>"));
        assert!(result.contains("<code class=\"sig-name\">greetererror</code>"));
        assert!(!result.contains("final"));
    }

    #[test]
    fn test_render_py_exception_final_prefix_precedes_exception_prefix() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyException {
                    signature: "greetererror".to_string(),
                    is_final: true,
                    body: vec![],
                },
            ))],
        );

        // When
        let result = render_doc(&doc);

        // Then
        let final_pos = result.find("final").unwrap();
        let exception_pos = result.find("exception</em>").unwrap();
        assert!(final_pos < exception_pos);
        assert!(result.contains("<em class=\"property\">final</em>"));
    }

    #[test]
    fn test_render_qualifies_method_nested_in_exception_id() {
        // Given — a `py:method` nested inside a `py:exception` body
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyException {
                    signature: "greetererror".to_string(),
                    is_final: false,
                    body: vec![Node::Directive(Directive::DomainObject(
                        rusty_sphinx_ast::DomainObjectBody::PyMethod {
                            signature: "reason(self)".to_string(),
                            is_classmethod: false,
                            is_staticmethod: false,
                            is_abstractmethod: false,
                            is_async: false,
                            body: vec![],
                        },
                    ))],
                },
            ))],
        );

        // When
        let result = render_doc(&doc);

        // Then — the nested method's id is qualified, matching the
        // analyzer's index key exactly.
        assert!(result.contains("<dt id=\"py:method:greetererror.reason\">"));
        assert!(result.contains("<code class=\"sig-name\">reason(self)</code>"));
    }

    #[test]
    fn test_render_does_not_double_qualify_already_qualified_nested_attribute() {
        // Given — mirrors CPython's `Doc/library/exceptions.rst`, which
        // nests `.. attribute:: StopIteration.value` (already fully
        // qualified) inside `.. exception:: StopIteration`.
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyException {
                    signature: "StopIteration".to_string(),
                    is_final: false,
                    body: vec![Node::Directive(Directive::DomainObject(
                        rusty_sphinx_ast::DomainObjectBody::PyAttribute {
                            name: "StopIteration.value".to_string(),
                            type_: None,
                            value: None,
                            canonical: None,
                            body: vec![],
                        },
                    ))],
                },
            ))],
        );

        // When
        let result = render_doc(&doc);

        // Then — not doubled to "py:attribute:stopiteration.stopiteration.value"
        // (`TargetName` lowercases keys, same as every other domain object test).
        assert!(result.contains("<dt id=\"py:attribute:stopiteration.value\">"));
    }

    #[test]
    fn test_render_class_stack_does_not_leak_across_sibling_classes() {
        // Given — two sibling classes, each with a method of the same name;
        // the second class's method must not inherit the first class's
        // qualifier from a stale, un-popped stack entry.
        let doc = Document::new(
            "test.rst".to_string(),
            vec![
                Node::Directive(Directive::DomainObject(
                    rusty_sphinx_ast::DomainObjectBody::PyClass {
                        signature: "first".to_string(),
                        is_final: false,
                        body: vec![Node::Directive(Directive::DomainObject(
                            rusty_sphinx_ast::DomainObjectBody::PyMethod {
                                signature: "run(self)".to_string(),
                                is_classmethod: false,
                                is_staticmethod: false,
                                is_abstractmethod: false,
                                is_async: false,
                                body: vec![],
                            },
                        ))],
                    },
                )),
                Node::Directive(Directive::DomainObject(
                    rusty_sphinx_ast::DomainObjectBody::PyClass {
                        signature: "second".to_string(),
                        is_final: false,
                        body: vec![Node::Directive(Directive::DomainObject(
                            rusty_sphinx_ast::DomainObjectBody::PyMethod {
                                signature: "run(self)".to_string(),
                                is_classmethod: false,
                                is_staticmethod: false,
                                is_abstractmethod: false,
                                is_async: false,
                                body: vec![],
                            },
                        ))],
                    },
                )),
            ],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<dt id=\"py:method:first.run\">"));
        assert!(result.contains("<dt id=\"py:method:second.run\">"));
        assert!(!result.contains("py:method:first.second.run"));
    }

    #[test]
    fn test_render_formats_py_data_domain_object() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyData {
                    name: "DEFAULT_TIMEOUT".to_string(),
                    type_: None,
                    value: None,
                    body: vec![Node::Paragraph(vec![InlineNode::Text(
                        "The default timeout in seconds.".to_string(),
                    )])],
                },
            ))],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<dl class=\"py data\">"));
        assert!(result.contains("<dt id=\"py:data:default_timeout\">"));
        assert!(result.contains("<code class=\"sig-name\">DEFAULT_TIMEOUT</code>"));
    }

    #[test]
    fn test_render_formats_py_data_type_and_value() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyData {
                    name: "DEFAULT_TIMEOUT".to_string(),
                    type_: Some("int".to_string()),
                    value: Some("30".to_string()),
                    body: vec![],
                },
            ))],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<p class=\"type\">Type: int</p>"));
        assert!(result.contains("<p class=\"value\">Value: 30</p>"));
    }

    #[test]
    fn test_render_formats_py_attribute_domain_object() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyAttribute {
                    name: "Greeter.name".to_string(),
                    type_: None,
                    value: None,
                    canonical: None,
                    body: vec![Node::Paragraph(vec![InlineNode::Text(
                        "The greeter's name.".to_string(),
                    )])],
                },
            ))],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<dl class=\"py attribute\">"));
        assert!(result.contains("<dt id=\"py:attribute:greeter.name\">"));
        assert!(result.contains("<code class=\"sig-name\">Greeter.name</code>"));
    }

    #[test]
    fn test_render_formats_py_attribute_type_value_and_canonical() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyAttribute {
                    name: "Greeter.name".to_string(),
                    type_: Some("str".to_string()),
                    value: Some("\"anonymous\"".to_string()),
                    canonical: Some("mymodule.MyClass.name".to_string()),
                    body: vec![],
                },
            ))],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<p class=\"type\">Type: str</p>"));
        assert!(result.contains("<p class=\"value\">Value: \"anonymous\"</p>"));
        assert!(result.contains("<p class=\"canonical\">Canonical: mymodule.MyClass.name</p>"));
    }

    #[test]
    fn test_render_domain_object_resolves_nested_anonymous_hyperlink_in_body() {
        // Given — regression test for the collect_anonymous_targets catch-all:
        // an anonymous reference nested inside a DomainObject body must still resolve.
        let doc = Document::new(
            "test.rst".to_string(),
            vec![
                Node::Directive(Directive::DomainObject(
                    rusty_sphinx_ast::DomainObjectBody::PyFunction {
                        signature: "greet(name)".to_string(),
                        body: vec![Node::Paragraph(vec![InlineNode::AnonymousReference(
                            "See more".to_string(),
                        )])],
                    },
                )),
                Node::AnonymousTarget {
                    uri: "https://example.com".to_string(),
                },
            ],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<a href=\"https://example.com\">See more</a>"));
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
        let result = crate::render(&doc, &index, &doc.path).html;

        // Then — href should traverse up one directory
        assert!(result.contains("href=\"../glossary.html#term-foo\""));
    }

    #[test]
    fn test_domain_object_prefix_labels_orders_method_flags_independent_of_input_order() {
        // Given — flags set in a different order than the canonical output order
        let obj = rusty_sphinx_ast::DomainObjectBody::PyMethod {
            signature: "run()".to_string(),
            is_classmethod: true,
            is_staticmethod: true,
            is_abstractmethod: true,
            is_async: true,
            body: vec![],
        };

        // When
        let labels = domain_object_prefix_labels(&obj);

        // Then
        assert_eq!(
            labels,
            vec!["abstractmethod", "async", "classmethod", "staticmethod"]
        );
    }

    #[test]
    fn test_domain_object_prefix_labels_omits_inactive_method_flags() {
        // Given
        let obj = rusty_sphinx_ast::DomainObjectBody::PyMethod {
            signature: "run()".to_string(),
            is_classmethod: false,
            is_staticmethod: true,
            is_abstractmethod: false,
            is_async: false,
            body: vec![],
        };

        // When
        let labels = domain_object_prefix_labels(&obj);

        // Then
        assert_eq!(labels, vec!["staticmethod"]);
    }

    #[test]
    fn test_domain_object_prefix_labels_includes_final_before_class_label() {
        // Given
        let obj = rusty_sphinx_ast::DomainObjectBody::PyClass {
            signature: "Greeter".to_string(),
            is_final: true,
            body: vec![],
        };

        // When
        let labels = domain_object_prefix_labels(&obj);

        // Then
        assert_eq!(labels, vec!["final", "class"]);
    }

    #[test]
    fn test_domain_object_prefix_labels_includes_final_before_exception_label() {
        // Given
        let obj = rusty_sphinx_ast::DomainObjectBody::PyException {
            signature: "GreeterError".to_string(),
            is_final: true,
            body: vec![],
        };

        // When
        let labels = domain_object_prefix_labels(&obj);

        // Then
        assert_eq!(labels, vec!["final", "exception"]);
    }

    #[test]
    fn test_class_like_prefix_labels_omits_final_when_not_set() {
        // Given / When
        let labels = class_like_prefix_labels(false, "exception");

        // Then
        assert_eq!(labels, vec!["exception"]);
    }

    #[test]
    fn test_domain_object_prefix_labels_is_empty_for_object_types_without_flags() {
        // Given
        let obj = rusty_sphinx_ast::DomainObjectBody::PyFunction {
            signature: "greet(name)".to_string(),
            body: vec![],
        };

        // When
        let labels = domain_object_prefix_labels(&obj);

        // Then
        assert!(labels.is_empty());
    }

    #[test]
    fn test_render_domain_object_options_renders_nothing_for_py_function() {
        // Given
        let obj = rusty_sphinx_ast::DomainObjectBody::PyFunction {
            signature: "greet(name)".to_string(),
            body: vec![],
        };
        let mut html = String::new();

        // When
        render_domain_object_options(&mut html, &obj);

        // Then
        assert!(html.is_empty());
    }

    #[test]
    fn test_render_domain_object_options_renders_nothing_for_py_exception() {
        // Given
        let obj = rusty_sphinx_ast::DomainObjectBody::PyException {
            signature: "GreeterError".to_string(),
            is_final: false,
            body: vec![],
        };
        let mut html = String::new();

        // When
        render_domain_object_options(&mut html, &obj);

        // Then
        assert!(html.is_empty());
    }
}

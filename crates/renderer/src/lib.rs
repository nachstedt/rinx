//! The renderer module converts the AST and `ProjectIndex` into HTML.

pub mod config;
mod directives;
mod inline;
mod nav;
mod page;

pub use page::{css_relative_path, render_page};

use directives::{
    render_admonition, render_domain_object, render_glossary, render_seealso, render_version_change,
};
use inline::render_inline;
use nav::{find_nav_entry, render_nav_entry};
use rusty_sphinx_analyzer::ProjectIndex;
use rusty_sphinx_ast::{Directive, Document, InlineNode, Node, TableRow};
use std::fmt::Write as _;

/// The kind of cross-reference role that produced a [`BrokenLink`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrokenLinkKind {
    /// A `:ref:` role (`InlineNode::Reference`).
    Reference,
    /// A named hyperlink (`InlineNode::Hyperlink`).
    Hyperlink,
    /// An anonymous `__` reference with no matching anonymous target left.
    AnonymousReference,
    /// A `:term:` role (`InlineNode::TermReference`).
    TermReference,
    /// A domain-object role (`:func:`, `:py:func:`, etc.).
    DomainObjectReference,
}

impl BrokenLinkKind {
    /// Returns a short, human-readable label for this kind, used in CLI diagnostics.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Reference => "ref",
            Self::Hyperlink => "hyperlink",
            Self::AnonymousReference => "anonymous reference",
            Self::TermReference => "term",
            Self::DomainObjectReference => "domain object",
        }
    }
}

/// A cross-reference that failed to resolve while rendering a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrokenLink {
    pub kind: BrokenLinkKind,
    pub target: String,
}

/// The result of rendering a document: the body HTML plus any cross-references
/// that failed to resolve against the [`ProjectIndex`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderOutput {
    pub html: String,
    pub broken_links: Vec<BrokenLink>,
}

/// Shared rendering state threaded through the node traversal.
pub(crate) struct RenderCtx<'a> {
    pub index: &'a ProjectIndex,
    pub doc_path: &'a str,
    pub anon_targets: &'a [String],
    pub anon_index: &'a mut usize,
    pub original_doc_path: &'a str,
    pub broken_links: &'a mut Vec<BrokenLink>,
}

/// Renders a Document into HTML, reporting any cross-references that failed to resolve.
#[must_use]
pub fn render(doc: &Document, index: &ProjectIndex, doc_path: &str) -> RenderOutput {
    let mut html = String::new();

    // Collect anonymous targets for local resolution recursively
    let mut anon_targets = Vec::new();
    collect_anonymous_targets(&doc.nodes, &mut anon_targets);
    let mut anon_index = 0;
    let mut broken_links = Vec::new();

    let mut ctx = RenderCtx {
        index,
        doc_path,
        anon_targets: &anon_targets,
        anon_index: &mut anon_index,
        original_doc_path: &doc.path,
        broken_links: &mut broken_links,
    };

    render_nodes(&mut html, &doc.nodes, &mut ctx);

    RenderOutput { html, broken_links }
}

fn collect_anonymous_targets(nodes: &[Node], targets: &mut Vec<String>) {
    for node in nodes {
        match node {
            Node::AnonymousTarget { uri } => targets.push(uri.clone()),
            Node::Directive(
                Directive::Admonition { body, .. }
                | Directive::VersionChange { body, .. }
                | Directive::SeeAlso { body },
            ) => {
                collect_anonymous_targets(body, targets);
            }
            Node::Directive(Directive::DomainObject(obj)) => {
                collect_anonymous_targets(obj.body(), targets);
            }
            Node::Directive(Directive::Glossary { entries, .. }) => {
                for entry in entries {
                    collect_anonymous_targets(&entry.definition, targets);
                }
            }
            _ => {}
        }
    }
}

/// Renders a sequence of inline nodes in order, sharing the same `ctx`
/// (index/anon-target state) across calls. Used for both heading text and
/// paragraph content, which are both just a `Vec<InlineNode>`.
fn render_inlines(html: &mut String, inlines: &[InlineNode], ctx: &mut RenderCtx<'_>) {
    for inline in inlines {
        render_inline(
            html,
            inline,
            ctx.index,
            ctx.doc_path,
            ctx.anon_targets,
            ctx.anon_index,
            ctx.broken_links,
        );
    }
}

/// Renders a single grid-table row, emitting each cell with the given tag
/// (`th` for header rows, `td` for body rows). `colspan`/`rowspan` attributes
/// are written only when greater than 1, matching how `LiteralBlock` only
/// emits its optional `language` attribute when present.
fn render_table_row(html: &mut String, row: &TableRow, cell_tag: &str, ctx: &mut RenderCtx<'_>) {
    let _ = writeln!(html, "<tr>");
    for cell in &row.cells {
        let _ = write!(html, "<{cell_tag}");
        if cell.colspan > 1 {
            let _ = write!(html, " colspan=\"{}\"", cell.colspan);
        }
        if cell.rowspan > 1 {
            let _ = write!(html, " rowspan=\"{}\"", cell.rowspan);
        }
        let _ = write!(html, ">");
        render_nodes(html, &cell.content, ctx);
        let _ = writeln!(html, "</{cell_tag}>");
    }
    let _ = writeln!(html, "</tr>");
}

pub(crate) fn render_nodes(html: &mut String, nodes: &[Node], ctx: &mut RenderCtx<'_>) {
    for node in nodes {
        match node {
            Node::Heading { level, text } => {
                let tag = format!("h{}", (*level).clamp(1, 6));
                let _ = write!(html, "<{tag}>");
                render_inlines(html, text, ctx);
                let _ = writeln!(html, "</{tag}>");
            }
            Node::Paragraph(inlines) => {
                let _ = write!(html, "<p>");
                render_inlines(html, inlines, ctx);
                let _ = writeln!(html, "</p>");
            }
            Node::Target { name, uri } => {
                if uri.is_none() {
                    let escaped_name = html_escape::encode_text(name.as_str());
                    let _ = writeln!(html, "<a id=\"{escaped_name}\"></a>");
                }
            }
            // Anonymous targets and comments produce no HTML output.
            Node::AnonymousTarget { .. } | Node::Comment => {}
            Node::Transition => {
                let _ = writeln!(html, "<hr />");
            }
            Node::Directive(directive) => render_directive(html, directive, ctx),
            Node::BulletList { items, .. } => {
                let _ = writeln!(html, "<ul>");
                for item in items {
                    let _ = write!(html, "<li>");
                    render_nodes(html, &item.nodes, ctx);
                    let _ = writeln!(html, "</li>");
                }
                let _ = writeln!(html, "</ul>");
            }
            Node::DefinitionList { items } => {
                let _ = writeln!(html, "<dl>");
                for item in items {
                    let _ = write!(html, "<dt>");
                    render_inlines(html, &item.term, ctx);
                    let _ = writeln!(html, "</dt>");
                    let _ = write!(html, "<dd>");
                    render_nodes(html, &item.definition, ctx);
                    let _ = writeln!(html, "</dd>");
                }
                let _ = writeln!(html, "</dl>");
            }
            Node::Table {
                header_rows,
                body_rows,
            } => {
                let _ = writeln!(html, "<table>");
                if !header_rows.is_empty() {
                    let _ = writeln!(html, "<thead>");
                    for row in header_rows {
                        render_table_row(html, row, "th", ctx);
                    }
                    let _ = writeln!(html, "</thead>");
                }
                let _ = writeln!(html, "<tbody>");
                for row in body_rows {
                    render_table_row(html, row, "td", ctx);
                }
                let _ = writeln!(html, "</tbody>");
                let _ = writeln!(html, "</table>");
            }
            Node::LiteralBlock { language, content } => {
                let escaped = html_escape::encode_text(content);
                if let Some(lang) = language {
                    let lang_attr = html_escape::encode_double_quoted_attribute(lang);
                    let _ = writeln!(
                        html,
                        "<pre><code class=\"language-{lang_attr}\">{escaped}</code></pre>"
                    );
                } else {
                    let _ = writeln!(html, "<pre><code>{escaped}</code></pre>");
                }
            }
        }
    }
}

fn render_directive(html: &mut String, directive: &Directive, ctx: &mut RenderCtx<'_>) {
    match directive {
        Directive::Toctree { maxdepth, .. } => {
            let _ = writeln!(html, "<ul>");
            let current_dir = std::path::Path::new(ctx.doc_path)
                .parent()
                .unwrap_or_else(|| std::path::Path::new(""));
            if let Some(current_entry) = find_nav_entry(&ctx.index.nav_tree, ctx.original_doc_path)
            {
                for child in &current_entry.children {
                    render_nav_entry(html, child, ctx.index, current_dir, 1, *maxdepth);
                }
            }
            let _ = writeln!(html, "</ul>");
        }
        Directive::PlantUml(content) => {
            let escaped_hash = html_escape::encode_text(content.hash());

            let current_dir = std::path::Path::new(ctx.doc_path)
                .parent()
                .unwrap_or_else(|| std::path::Path::new(""));
            let image_path = std::path::Path::new("_images").join(format!("{escaped_hash}.svg"));
            let relative_path =
                pathdiff::diff_paths(&image_path, current_dir).unwrap_or(image_path);
            let src = relative_path.display();

            let _ = writeln!(html, "<div class=\"plantuml-diagram\">");
            let _ = writeln!(html, "  <img src=\"{src}\" alt=\"PlantUML Diagram\" />");
            let _ = writeln!(html, "</div>");
        }
        Directive::Admonition {
            kind,
            title,
            collapsible,
            body,
        } => render_admonition(html, *kind, title.as_deref(), *collapsible, body, ctx),
        Directive::VersionChange {
            kind,
            version,
            body,
        } => render_version_change(html, *kind, version, body, ctx),
        Directive::SeeAlso { body } => render_seealso(html, body, ctx),
        Directive::Glossary { entries, .. } => render_glossary(html, entries, ctx),
        Directive::DomainObject(obj) => render_domain_object(html, obj, ctx),
        Directive::Unknown { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_sphinx_ast::{HashedContent, InlineNode, TargetName};

    #[test]
    fn test_render_returns_empty_string_for_empty_document() {
        // Given
        let doc = Document::new("test.rst".to_string(), vec![]);
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then
        assert_eq!(result, "");
    }

    #[test]
    fn test_render_formats_heading_and_paragraph_nodes() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![
                Node::Heading {
                    level: 1,
                    text: vec![InlineNode::Text("Title".to_string())],
                },
                Node::Paragraph(vec![rusty_sphinx_ast::InlineNode::Text(
                    "Paragraph".to_string(),
                )]),
                Node::Heading {
                    level: 1,
                    text: vec![InlineNode::Text("Another Heading".to_string())],
                },
            ],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then
        assert_eq!(
            result,
            "<h1>Title</h1>\n<p>Paragraph</p>\n<h1>Another Heading</h1>\n"
        );
    }

    #[test]
    fn test_render_heading_resolves_domain_object_reference_as_link() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Heading {
                level: 1,
                text: vec![
                    InlineNode::Text("The ".to_string()),
                    InlineNode::DomainObjectReference {
                        object_type: rusty_sphinx_ast::ObjectType::Py(
                            rusty_sphinx_ast::PyObjectType::Module,
                        ),
                        name: "greetings".to_string(),
                        display: "greetings".to_string(),
                        link: true,
                    },
                    InlineNode::Text(" Module".to_string()),
                ],
            }],
        );
        let mut index = ProjectIndex::default();
        index.domain_objects.insert(
            rusty_sphinx_ast::build_domain_object_key(
                rusty_sphinx_ast::ObjectType::Py(rusty_sphinx_ast::PyObjectType::Module),
                "greetings",
            ),
            "api.rst".to_string(),
        );

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then
        assert!(result.starts_with("<h1>The "));
        assert!(
            result
                .contains("<a class=\"reference internal\" href=\"api.html#py:module:greetings\">")
        );
        assert!(result.ends_with(" Module</h1>\n"));
    }

    #[test]
    fn test_render_escapes_html_special_characters() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![
                Node::Heading {
                    level: 1,
                    text: vec![InlineNode::Text("Title <script>".to_string())],
                },
                Node::Paragraph(vec![rusty_sphinx_ast::InlineNode::Text(
                    "A & B > C".to_string(),
                )]),
            ],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then
        assert_eq!(
            result,
            "<h1>Title &lt;script&gt;</h1>\n<p>A &amp; B &gt; C</p>\n"
        );
    }

    #[test]
    fn test_render_toctree_with_target_entries() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::Toctree {
                paths: vec!["team_a/index".to_string()],
                maxdepth: None,
                ignored_options: vec![],
            })],
        );
        let mut index = ProjectIndex::default();
        index
            .document_titles
            .insert("team_a/index.rst".to_string(), "Team A Module".to_string());
        index.nav_tree = vec![rusty_sphinx_analyzer::NavEntry {
            title: "test".to_string(),
            path: "test.rst".to_string(),
            children: vec![rusty_sphinx_analyzer::NavEntry {
                title: "Team A Module".to_string(),
                path: "team_a/index.rst".to_string(),
                children: vec![],
            }],
        }];

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then
        assert_eq!(
            result,
            "<ul>\n  <li><a href=\"team_a/index.html\">Team A Module</a></li>\n</ul>\n"
        );
    }

    #[test]
    fn test_render_formats_toctree_as_html_list() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::Toctree {
                paths: vec!["team_a/index".to_string(), "team_b/index".to_string()],
                maxdepth: None,
                ignored_options: vec![],
            })],
        );
        let mut index = ProjectIndex::default();
        index
            .document_titles
            .insert("team_a/index.rst".to_string(), "Team A Module".to_string());
        // Build a nav_tree so the renderer can look up children by doc path.
        index.nav_tree = vec![rusty_sphinx_analyzer::NavEntry {
            title: "test".to_string(),
            path: "test.rst".to_string(),
            children: vec![
                rusty_sphinx_analyzer::NavEntry {
                    title: "Team A Module".to_string(),
                    path: "team_a/index.rst".to_string(),
                    children: vec![],
                },
                rusty_sphinx_analyzer::NavEntry {
                    title: "team_b/index".to_string(),
                    path: "team_b/index.rst".to_string(),
                    children: vec![],
                },
            ],
        }];

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then
        assert_eq!(
            result,
            "<ul>\n  <li><a href=\"team_a/index.html\">Team A Module</a></li>\n  <li><a href=\"team_b/index.html\">team_b/index.rst</a></li>\n</ul>\n"
        );
    }

    #[test]
    fn test_render_formats_heading_level_1_as_h1() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Heading {
                level: 1,
                text: vec![InlineNode::Text("Top".to_string())],
            }],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then
        assert_eq!(result, "<h1>Top</h1>\n");
    }

    #[test]
    fn test_render_formats_heading_level_2_as_h2() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Heading {
                level: 2,
                text: vec![InlineNode::Text("Sub".to_string())],
            }],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then
        assert_eq!(result, "<h2>Sub</h2>\n");
    }

    #[test]
    fn test_render_formats_heading_level_6_as_h6() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Heading {
                level: 6,
                text: vec![InlineNode::Text("Deep".to_string())],
            }],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then
        assert_eq!(result, "<h6>Deep</h6>\n");
    }

    #[test]
    fn test_render_clamps_heading_level_above_6_to_h6() {
        // Given — level 7 exceeds the HTML maximum of 6
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Heading {
                level: 7,
                text: vec![InlineNode::Text("VeryDeep".to_string())],
            }],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then
        assert_eq!(result, "<h6>VeryDeep</h6>\n");
    }

    #[test]
    fn test_render_ignores_unknown_directive() {
        // Given a document with an unknown directive
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::Unknown {
                name: "some-unknown".to_string(),
                argument: "arg".to_string(),
                body: "body".to_string(),
            })],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then the output should be empty, as unknown directives are ignored
        assert_eq!(result, "");
    }

    #[test]
    fn test_render_formats_target_node_as_html_anchor() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Target {
                name: TargetName::new("section-1"),
                uri: None,
            }],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then
        assert_eq!(result, "<a id=\"section-1\"></a>\n");
    }

    #[test]
    fn test_render_formats_transition_node_as_horizontal_rule() {
        // Given
        let doc = Document::new("test.rst".to_string(), vec![Node::Transition]);
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then
        assert_eq!(result, "<hr />\n");
    }

    #[test]
    fn test_render_suppresses_anchor_for_external_target() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Target {
                name: TargetName::new("google"),
                uri: Some("https://google.com".to_string()),
            }],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then
        assert_eq!(result, "");
    }

    #[test]
    fn test_render_formats_inline_reference_using_project_index() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Paragraph(vec![
                rusty_sphinx_ast::InlineNode::Reference("other-section".to_string()),
            ])],
        );
        let mut index = ProjectIndex::default();
        index.targets.insert(
            TargetName::new("other-section"),
            rusty_sphinx_analyzer::TargetLocation::Internal("other_file.rst".to_string()),
        );

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then
        assert_eq!(
            result,
            "<p><a href=\"other_file.html#other-section\">other-section</a></p>\n"
        );
    }

    #[test]
    fn test_render_reports_no_broken_links_when_all_references_resolve() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Paragraph(vec![
                rusty_sphinx_ast::InlineNode::Reference("other-section".to_string()),
            ])],
        );
        let mut index = ProjectIndex::default();
        index.targets.insert(
            TargetName::new("other-section"),
            rusty_sphinx_analyzer::TargetLocation::Internal("other_file.rst".to_string()),
        );

        // When
        let result = render(&doc, &index, &doc.path);

        // Then
        assert!(result.broken_links.is_empty());
    }

    #[test]
    fn test_render_collects_broken_links_across_multiple_reference_kinds() {
        // Given a document with a broken :ref: and a broken :term:
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Paragraph(vec![
                rusty_sphinx_ast::InlineNode::Reference("missing-ref".to_string()),
                rusty_sphinx_ast::InlineNode::TermReference {
                    display: "missing term".to_string(),
                    term: "missing-term".to_string(),
                },
            ])],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path);

        // Then
        assert_eq!(
            result.broken_links,
            vec![
                crate::BrokenLink {
                    kind: crate::BrokenLinkKind::Reference,
                    target: "missing-ref".to_string(),
                },
                crate::BrokenLink {
                    kind: crate::BrokenLinkKind::TermReference,
                    target: "missing-term".to_string(),
                },
            ]
        );
    }

    #[test]
    fn test_render_resolves_cross_directory_references_as_relative_links() {
        // Given a document in a subdirectory
        let doc = Document::new(
            "examples/team_b/index.rst".to_string(),
            vec![Node::Paragraph(vec![
                rusty_sphinx_ast::InlineNode::Reference("target-in-a".to_string()),
            ])],
        );

        let mut index = ProjectIndex::default();
        index.targets.insert(
            TargetName::new("target-in-a"),
            rusty_sphinx_analyzer::TargetLocation::Internal(
                "examples/team_a/index.rst".to_string(),
            ),
        );

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then the link should point backwards up out of team_b/ and into team_a/
        assert_eq!(
            result,
            "<p><a href=\"../team_a/index.html#target-in-a\">target-in-a</a></p>\n"
        );
    }

    #[test]
    fn test_render_formats_external_hyperlink() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Paragraph(vec![
                rusty_sphinx_ast::InlineNode::Hyperlink {
                    text: "Python".to_string(),
                    target: "Python".to_string(),
                },
            ])],
        );
        let mut index = ProjectIndex::default();
        index.targets.insert(
            TargetName::new("Python"),
            rusty_sphinx_analyzer::TargetLocation::External("https://python.org".to_string()),
        );

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then
        assert_eq!(result, "<p><a href=\"https://python.org\">Python</a></p>\n");
    }

    #[test]
    fn test_render_formats_direct_uri_hyperlink() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Paragraph(vec![
                rusty_sphinx_ast::InlineNode::Hyperlink {
                    text: "Google".to_string(),
                    target: "https://google.com".to_string(),
                },
            ])],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then
        assert_eq!(result, "<p><a href=\"https://google.com\">Google</a></p>\n");
    }

    #[test]
    fn test_render_formats_plantuml_with_relative_path() {
        // Given a document in a subdirectory
        let content = HashedContent::new("A -> B".to_string());
        let expected_hash = content.hash().to_string();
        let doc = Document::new(
            "examples/team_b/index.rst".to_string(),
            vec![Node::Directive(Directive::PlantUml(content))],
        );

        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then the image src should point backwards up out of team_b/ and examples/ and into _images/
        let expected = format!(
            "<div class=\"plantuml-diagram\">\n  <img src=\"../../_images/{expected_hash}.svg\" alt=\"PlantUML Diagram\" />\n</div>\n"
        );
        assert_eq!(result, expected);
    }

    #[test]
    fn test_render_bullet_list() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::BulletList {
                bullet: '*',
                items: vec![
                    rusty_sphinx_ast::BulletListItem {
                        nodes: vec![Node::Paragraph(vec![InlineNode::Text(
                            "Item 1".to_string(),
                        )])],
                    },
                    rusty_sphinx_ast::BulletListItem {
                        nodes: vec![Node::Paragraph(vec![InlineNode::Text(
                            "Item 2".to_string(),
                        )])],
                    },
                ],
            }],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then
        assert_eq!(
            result,
            "<ul>\n<li><p>Item 1</p>\n</li>\n<li><p>Item 2</p>\n</li>\n</ul>\n"
        );
    }

    #[test]
    fn test_render_bullet_list_nested() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::BulletList {
                bullet: '*',
                items: vec![rusty_sphinx_ast::BulletListItem {
                    nodes: vec![
                        Node::Paragraph(vec![InlineNode::Text("Parent".to_string())]),
                        Node::BulletList {
                            bullet: '-',
                            items: vec![rusty_sphinx_ast::BulletListItem {
                                nodes: vec![Node::Paragraph(vec![InlineNode::Text(
                                    "Child".to_string(),
                                )])],
                            }],
                        },
                    ],
                }],
            }],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then
        assert!(result.contains(
            "<ul>\n<li><p>Parent</p>\n<ul>\n<li><p>Child</p>\n</li>\n</ul>\n</li>\n</ul>"
        ));
    }

    #[test]
    fn test_render_bullet_list_multi_paragraph() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::BulletList {
                bullet: '*',
                items: vec![rusty_sphinx_ast::BulletListItem {
                    nodes: vec![
                        Node::Paragraph(vec![InlineNode::Text("Para 1".to_string())]),
                        Node::Paragraph(vec![InlineNode::Text("Para 2".to_string())]),
                    ],
                }],
            }],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then
        assert!(result.contains("<li><p>Para 1</p>\n<p>Para 2</p>\n</li>"));
    }

    #[test]
    fn test_render_definition_list() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::DefinitionList {
                items: vec![
                    rusty_sphinx_ast::DefinitionListItem {
                        term: vec![InlineNode::Text("Term 1".to_string())],
                        definition: vec![Node::Paragraph(vec![InlineNode::Text(
                            "Def 1".to_string(),
                        )])],
                    },
                    rusty_sphinx_ast::DefinitionListItem {
                        term: vec![InlineNode::Text("Term 2".to_string())],
                        definition: vec![Node::Paragraph(vec![InlineNode::Text(
                            "Def 2".to_string(),
                        )])],
                    },
                ],
            }],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then
        assert_eq!(
            result,
            "<dl>\n<dt>Term 1</dt>\n<dd><p>Def 1</p>\n</dd>\n<dt>Term 2</dt>\n<dd><p>Def 2</p>\n</dd>\n</dl>\n"
        );
    }

    #[test]
    fn test_render_definition_list_escapes_and_renders_inline_markup_in_term() {
        // Given a term containing a domain-object reference, mirroring the
        // CPython benchmark's `seealso` definition-list content
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::DefinitionList {
                items: vec![rusty_sphinx_ast::DefinitionListItem {
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
                        "Utilities for ASCII characters.".to_string(),
                    )])],
                }],
            }],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then the <dt> contains the rendered inline markup, not raw text
        assert!(result.starts_with("<dl>\n<dt>Module "));
        assert!(result.contains("curses.ascii"));
        assert!(result.contains("<dd><p>Utilities for ASCII characters.</p>\n</dd>"));
    }

    #[test]
    fn test_render_table_with_header() {
        // Given a table with a header row and a body row, no spans
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Table {
                header_rows: vec![rusty_sphinx_ast::TableRow {
                    cells: vec![
                        rusty_sphinx_ast::TableCell {
                            colspan: 1,
                            rowspan: 1,
                            content: vec![Node::Paragraph(vec![InlineNode::Text("A".to_string())])],
                        },
                        rusty_sphinx_ast::TableCell {
                            colspan: 1,
                            rowspan: 1,
                            content: vec![Node::Paragraph(vec![InlineNode::Text("B".to_string())])],
                        },
                    ],
                }],
                body_rows: vec![rusty_sphinx_ast::TableRow {
                    cells: vec![
                        rusty_sphinx_ast::TableCell {
                            colspan: 1,
                            rowspan: 1,
                            content: vec![Node::Paragraph(vec![InlineNode::Text(
                                "a1".to_string(),
                            )])],
                        },
                        rusty_sphinx_ast::TableCell {
                            colspan: 1,
                            rowspan: 1,
                            content: vec![Node::Paragraph(vec![InlineNode::Text(
                                "b1".to_string(),
                            )])],
                        },
                    ],
                }],
            }],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then
        assert_eq!(
            result,
            "<table>\n\
             <thead>\n<tr>\n<th><p>A</p>\n</th>\n<th><p>B</p>\n</th>\n</tr>\n</thead>\n\
             <tbody>\n<tr>\n<td><p>a1</p>\n</td>\n<td><p>b1</p>\n</td>\n</tr>\n</tbody>\n\
             </table>\n"
        );
    }

    #[test]
    fn test_render_table_without_header_omits_thead() {
        // Given a header-less table
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Table {
                header_rows: vec![],
                body_rows: vec![rusty_sphinx_ast::TableRow {
                    cells: vec![rusty_sphinx_ast::TableCell {
                        colspan: 1,
                        rowspan: 1,
                        content: vec![Node::Paragraph(vec![InlineNode::Text("only".to_string())])],
                    }],
                }],
            }],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then — no <thead> element at all
        assert!(!result.contains("<thead>"));
        assert_eq!(
            result,
            "<table>\n<tbody>\n<tr>\n<td><p>only</p>\n</td>\n</tr>\n</tbody>\n</table>\n"
        );
    }

    #[test]
    fn test_render_table_emits_colspan_and_rowspan_attributes() {
        // Given a body cell spanning 2 columns and 3 rows
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Table {
                header_rows: vec![],
                body_rows: vec![rusty_sphinx_ast::TableRow {
                    cells: vec![rusty_sphinx_ast::TableCell {
                        colspan: 2,
                        rowspan: 3,
                        content: vec![Node::Paragraph(vec![InlineNode::Text(
                            "spanning".to_string(),
                        )])],
                    }],
                }],
            }],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then — attributes present with the correct values
        assert!(result.contains("<td colspan=\"2\" rowspan=\"3\"><p>spanning</p>\n</td>"));
    }

    #[test]
    fn test_render_resolves_anonymous_links_in_order() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![
                Node::Paragraph(vec![
                    InlineNode::AnonymousReference("First".to_string()),
                    InlineNode::Text(" and ".to_string()),
                    InlineNode::AnonymousReference("Second".to_string()),
                ]),
                Node::AnonymousTarget {
                    uri: "https://first.com".to_string(),
                },
                Node::AnonymousTarget {
                    uri: "https://second.com".to_string(),
                },
            ],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then
        assert!(result.contains("<a href=\"https://first.com\">First</a>"));
        assert!(result.contains("<a href=\"https://second.com\">Second</a>"));
    }

    #[test]
    fn test_render_anonymous_hyperlink_with_embedded_uri_does_not_consume_targets() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![
                Node::Paragraph(vec![
                    InlineNode::AnonymousHyperlink {
                        text: "Embedded".to_string(),
                        target: "https://embedded.com".to_string(),
                    },
                    InlineNode::Text(" then ".to_string()),
                    InlineNode::AnonymousReference("Reference".to_string()),
                ]),
                Node::AnonymousTarget {
                    uri: "https://target.com".to_string(),
                },
            ],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then
        assert!(result.contains("<a href=\"https://embedded.com\">Embedded</a>"));
        assert!(result.contains("<a href=\"https://target.com\">Reference</a>"));
    }

    #[test]
    fn test_render_toctree_avoids_infinite_loop_on_cyclic_nav_tree() {
        // Given a document with a toctree that includes itself (cycle)
        let doc = Document::new(
            "cycle.rst".to_string(),
            vec![Node::Directive(Directive::Toctree {
                paths: vec!["cycle".to_string()],
                maxdepth: None,
                ignored_options: vec![],
            })],
        );

        // And a ProjectIndex that represents this cycle but is truncated by the analyzer
        // to a finite depth (e.g. depth 2)
        let index = ProjectIndex {
            nav_tree: vec![rusty_sphinx_analyzer::NavEntry {
                path: "cycle.rst".to_string(),
                title: "Cycle".to_string(),
                children: vec![rusty_sphinx_analyzer::NavEntry {
                    path: "cycle.rst".to_string(), // Cycle back to the same path
                    title: "Cycle".to_string(),
                    children: vec![], // Truncated here
                }],
            }],
            document_titles: std::iter::once(("cycle.rst".to_string(), "Cycle".to_string()))
                .collect(),
            ..ProjectIndex::default()
        };

        // When
        // This would stack overflow if the renderer searched from the root for every child
        let html = render(&doc, &index, &doc.path).html;

        // Then
        // The output should contain nested lists reflecting the finite depth of nav_tree
        assert!(html.contains("<ul>"));
        assert!(html.contains("<li><a href=\"cycle.html\">Cycle</a>"));
    }

    #[test]
    fn test_render_toctree_renders_empty_list_when_doc_not_in_nav_tree() {
        // Given — the document has a toctree but is absent from the nav tree
        let doc = Document::new(
            "index.rst".to_string(),
            vec![Node::Directive(Directive::Toctree {
                paths: vec!["child".to_string()],
                maxdepth: None,
                ignored_options: vec![],
            })],
        );
        let index = ProjectIndex::default(); // empty nav_tree

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then — no crash, just an empty list
        assert_eq!(result, "<ul>\n</ul>\n");
    }

    #[test]
    fn test_render_comment_produces_no_html() {
        // Given
        let doc = Document::new("test.rst".to_string(), vec![Node::Comment]);
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then
        assert_eq!(result, "");
    }

    #[test]
    fn test_render_toctree_respects_maxdepth() {
        // Given — a two-level nav tree, maxdepth: 1 should suppress the grandchild
        let doc = Document::new(
            "index.rst".to_string(),
            vec![Node::Directive(Directive::Toctree {
                paths: vec!["child".to_string()],
                maxdepth: Some(1),
                ignored_options: vec![],
            })],
        );
        let mut index = ProjectIndex::default();
        index
            .document_titles
            .insert("child.rst".to_string(), "Child".to_string());
        index
            .document_titles
            .insert("grandchild.rst".to_string(), "Grandchild".to_string());
        index.nav_tree = vec![rusty_sphinx_analyzer::NavEntry {
            title: "Root".to_string(),
            path: "index.rst".to_string(),
            children: vec![rusty_sphinx_analyzer::NavEntry {
                title: "Child".to_string(),
                path: "child.rst".to_string(),
                children: vec![rusty_sphinx_analyzer::NavEntry {
                    title: "Grandchild".to_string(),
                    path: "grandchild.rst".to_string(),
                    children: vec![],
                }],
            }],
        }];

        // When
        let result = render(&doc, &index, &doc.path).html;

        // Then — child appears but grandchild is suppressed by maxdepth: 1
        assert!(result.contains("Child"), "child should be rendered");
        assert!(
            !result.contains("Grandchild"),
            "grandchild must be suppressed by maxdepth:1"
        );
    }
}

//! The renderer module converts the AST and `ProjectIndex` into HTML.

use crate::analyzer::{ProjectIndex, TargetLocation};
use crate::ast::{Directive, Document, Node, TargetName};
use crate::config::SiteConfig;
use anyhow::{Context, Result};
use std::fmt::Write as _;

/// Computes the relative path from a document at `doc_path` to a file
/// at the site output root (e.g., `default.css`).
///
/// For a document at `team_a/index.rst`, this returns `../default.css`.
/// For a document at the root (`index.rst`), this returns `default.css`.
#[must_use]
pub fn css_relative_path(doc_path: &str, css_filename: &str) -> String {
    let depth = doc_path.matches('/').count();
    if depth == 0 {
        css_filename.to_string()
    } else {
        let prefix = "../".repeat(depth);
        format!("{prefix}{css_filename}")
    }
}

/// Renders a page body into a full HTML document using a `MiniJinja` template.
///
/// The `template_str` is Jinja2-compatible markup loaded from an `.html` file.
/// The `body` is the inner HTML produced by [`render()`]. The `config` provides
/// project metadata, `css_path` is the relative path to the stylesheet, and
/// `nav_tree` provides hierarchical sidebar navigation.
///
/// # Errors
///
/// Returns an error if the template cannot be parsed or rendered.
pub fn render_page(
    body: &str,
    template_str: &str,
    config: &SiteConfig,
    css_path: &str,
    page_title: &str,
    doc_path: &str,
    nav_tree: &[crate::analyzer::NavEntry],
) -> Result<String> {
    let resolved_nav = resolve_nav_hrefs(nav_tree, doc_path);

    let mut env = minijinja::Environment::new();
    env.add_template("page", template_str)
        .context("Failed to parse template")?;
    let tmpl = env.get_template("page").context("Template not found")?;
    let ctx = minijinja::context! {
        body => body,
        project => &config.project,
        version => &config.version,
        css_path => css_path,
        page_title => page_title,
        nav_tree => resolved_nav,
    };
    tmpl.render(ctx).context("Failed to render template")
}

/// A nav entry with its path resolved to a relative HTML href.
#[derive(Debug, serde::Serialize)]
struct ResolvedNavEntry {
    title: String,
    href: String,
    children: Vec<Self>,
}

/// Converts a nav tree's `.rst` paths into relative `.html` hrefs
/// based on the current document's location.
fn resolve_nav_hrefs(
    entries: &[crate::analyzer::NavEntry],
    doc_path: &str,
) -> Vec<ResolvedNavEntry> {
    let doc_dir = std::path::Path::new(doc_path)
        .parent()
        .unwrap_or_else(|| std::path::Path::new(""));

    entries
        .iter()
        .map(|entry| {
            let path_without_ext = entry.path.strip_suffix(".rst").unwrap_or(&entry.path);
            let target_html = format!("{path_without_ext}.html");
            let target = std::path::Path::new(&target_html);
            let href = pathdiff::diff_paths(target, doc_dir)
                .map_or_else(|| target_html, |p| p.to_string_lossy().replace('\\', "/"));

            ResolvedNavEntry {
                title: entry.title.clone(),
                href,
                children: resolve_nav_hrefs(&entry.children, doc_path),
            }
        })
        .collect()
}

/// Renders a Document into an HTML string.
#[must_use]
pub fn render(doc: &Document, index: &ProjectIndex, doc_path: &str) -> String {
    let mut html = String::new();

    // Collect anonymous targets for local resolution recursively
    let mut anon_targets = Vec::new();
    collect_anonymous_targets(&doc.nodes, &mut anon_targets);
    let mut anon_index = 0;

    render_nodes(
        &mut html,
        &doc.nodes,
        index,
        doc_path,
        &anon_targets,
        &mut anon_index,
        &doc.path,
    );

    html
}

fn collect_anonymous_targets(nodes: &[Node], targets: &mut Vec<String>) {
    for node in nodes {
        match node {
            Node::AnonymousTarget { uri } => targets.push(uri.clone()),
            Node::Directive(Directive::Admonition { body, .. }) => {
                collect_anonymous_targets(body, targets);
            }
            _ => {}
        }
    }
}

fn render_nodes(
    html: &mut String,
    nodes: &[Node],
    index: &ProjectIndex,
    doc_path: &str,
    anon_targets: &[String],
    anon_index: &mut usize,
    original_doc_path: &str,
) {
    for node in nodes {
        match node {
            Node::Heading { level, text } => {
                let tag = format!("h{}", (*level).clamp(1, 6));
                let escaped_text = html_escape::encode_text(text);
                let _ = writeln!(html, "<{tag}>{escaped_text}</{tag}>");
            }
            Node::Paragraph(inlines) => {
                let _ = write!(html, "<p>");
                for inline in inlines {
                    render_inline(html, inline, index, doc_path, anon_targets, anon_index);
                }
                let _ = writeln!(html, "</p>");
            }
            Node::Target { name, uri } => {
                if uri.is_none() {
                    let escaped_name = html_escape::encode_text(name.as_str());
                    let _ = writeln!(html, "<a id=\"{escaped_name}\"></a>");
                }
            }
            Node::AnonymousTarget { .. } => {}
            Node::Directive(directive) => render_directive(
                html,
                directive,
                index,
                doc_path,
                anon_targets,
                anon_index,
                original_doc_path,
            ),
            Node::BulletList { items, .. } => {
                let _ = writeln!(html, "<ul>");
                for item in items {
                    let _ = write!(html, "<li>");
                    render_nodes(
                        html,
                        &item.nodes,
                        index,
                        doc_path,
                        anon_targets,
                        anon_index,
                        original_doc_path,
                    );
                    let _ = writeln!(html, "</li>");
                }
                let _ = writeln!(html, "</ul>");
            }
        }
    }
}

fn render_directive(
    html: &mut String,
    directive: &Directive,
    index: &ProjectIndex,
    doc_path: &str,
    anon_targets: &[String],
    anon_index: &mut usize,
    original_doc_path: &str,
) {
    match directive {
        Directive::Toctree { maxdepth, .. } => {
            let _ = writeln!(html, "<ul>");
            let current_dir = std::path::Path::new(doc_path)
                .parent()
                .unwrap_or_else(|| std::path::Path::new(""));
            if let Some(current_entry) = find_nav_entry(&index.nav_tree, original_doc_path) {
                for child in &current_entry.children {
                    render_nav_entry(html, child, index, current_dir, 1, *maxdepth);
                }
            }
            let _ = writeln!(html, "</ul>");
        }
        Directive::PlantUml(content) => {
            let escaped_hash = html_escape::encode_text(content.hash());

            let current_dir = std::path::Path::new(doc_path)
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
        } => render_admonition(
            html,
            *kind,
            title.as_deref(),
            *collapsible,
            body,
            index,
            doc_path,
            anon_targets,
            anon_index,
            original_doc_path,
        ),
        Directive::Unknown { .. } => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn render_admonition(
    html: &mut String,
    kind: crate::ast::AdmonitionKind,
    title: Option<&str>,
    collapsible: Option<bool>,
    body: &[Node],
    index: &ProjectIndex,
    doc_path: &str,
    anon_targets: &[String],
    anon_index: &mut usize,
    original_doc_path: &str,
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
        render_nodes(
            html,
            body,
            index,
            doc_path,
            anon_targets,
            anon_index,
            original_doc_path,
        );
        let _ = writeln!(html, "</details>");
    } else {
        let _ = writeln!(html, "<div class=\"admonition {kind_escaped}\">");
        let _ = writeln!(html, "  <p class=\"admonition-title\">{title_escaped}</p>");
        render_nodes(
            html,
            body,
            index,
            doc_path,
            anon_targets,
            anon_index,
            original_doc_path,
        );
        let _ = writeln!(html, "</div>");
    }
}

fn render_inline(
    html: &mut String,
    inline: &crate::ast::InlineNode,
    index: &ProjectIndex,
    doc_path: &str,
    anon_targets: &[String],
    anon_index: &mut usize,
) {
    match inline {
        crate::ast::InlineNode::Text(text) => {
            let escaped = html_escape::encode_text(text);
            let _ = write!(html, "{escaped}");
        }
        crate::ast::InlineNode::Reference(target) => {
            let target_escaped = html_escape::encode_text(target);
            // Legacy :ref: behavior (still supported)
            let target_name = TargetName::new(target);
            if let Some(TargetLocation::Internal(target_path)) = index.targets.get(&target_name) {
                let current_dir = std::path::Path::new(doc_path)
                    .parent()
                    .unwrap_or_else(|| std::path::Path::new(""));
                let target_html_path = std::path::Path::new(target_path).with_extension("html");

                let relative_path = pathdiff::diff_paths(&target_html_path, current_dir)
                    .unwrap_or(target_html_path);

                // display() on Unix uses `/`, so it maps correctly to URLs
                let href = format!("{}#{}", relative_path.display(), target_name.as_str());
                let href_attr = html_escape::encode_double_quoted_attribute(&href);
                let _ = write!(html, "<a href=\"{href_attr}\">{target_escaped}</a>");
            } else {
                // Fallback, could print warning
                let _ = write!(
                    html,
                    "<a href=\"#{target_escaped}\" class=\"broken-link\">{target_escaped}</a>"
                );
            }
        }
        crate::ast::InlineNode::Hyperlink { text, target } => {
            let text_escaped = html_escape::encode_text(text);
            // 1. Is it a direct URI?
            if target.starts_with("http://")
                || target.starts_with("https://")
                || target.starts_with("mailto:")
            {
                let target_attr = html_escape::encode_double_quoted_attribute(target);
                let _ = write!(html, "<a href=\"{target_attr}\">{text_escaped}</a>");
            } else {
                // 2. Lookup in index
                let target_name = TargetName::new(target);
                if let Some(location) = index.targets.get(&target_name) {
                    match location {
                        TargetLocation::External(url) => {
                            let url_attr = html_escape::encode_double_quoted_attribute(&url);
                            let _ = write!(html, "<a href=\"{url_attr}\">{text_escaped}</a>");
                        }
                        TargetLocation::Internal(target_path) => {
                            let current_dir = std::path::Path::new(doc_path)
                                .parent()
                                .unwrap_or_else(|| std::path::Path::new(""));
                            let target_html_path =
                                std::path::Path::new(target_path).with_extension("html");

                            let relative_path =
                                pathdiff::diff_paths(&target_html_path, current_dir)
                                    .unwrap_or(target_html_path);

                            let href =
                                format!("{}#{}", relative_path.display(), target_name.as_str());
                            let href_attr = html_escape::encode_double_quoted_attribute(&href);
                            let _ = write!(html, "<a href=\"{href_attr}\">{text_escaped}</a>");
                        }
                    }
                } else {
                    // Broken link
                    let _ = write!(
                        html,
                        "<a href=\"#\" class=\"broken-link\">{text_escaped}</a>"
                    );
                }
            }
        }
        crate::ast::InlineNode::AnonymousReference(text) => {
            let text_escaped = html_escape::encode_text(text);
            if let Some(uri) = anon_targets.get(*anon_index) {
                let uri_attr = html_escape::encode_double_quoted_attribute(uri);
                let _ = write!(html, "<a href=\"{uri_attr}\">{text_escaped}</a>");
                *anon_index += 1;
            } else {
                let _ = write!(
                    html,
                    "<a href=\"#\" class=\"broken-link\">{text_escaped}</a>"
                );
            }
        }
        crate::ast::InlineNode::AnonymousHyperlink { text, target } => {
            let text_escaped = html_escape::encode_text(text);
            let target_attr = html_escape::encode_double_quoted_attribute(target);
            let _ = write!(html, "<a href=\"{target_attr}\">{text_escaped}</a>");
        }
        crate::ast::InlineNode::Emphasis(text) => {
            let escaped = html_escape::encode_text(text);
            let _ = write!(html, "<em>{escaped}</em>");
        }
        crate::ast::InlineNode::Strong(text) => {
            let escaped = html_escape::encode_text(text);
            let _ = write!(html, "<strong>{escaped}</strong>");
        }
        crate::ast::InlineNode::Literal(text) => {
            let escaped = html_escape::encode_text(text);
            let _ = write!(html, "<code>{escaped}</code>");
        }
    }
}

fn render_nav_entry(
    html: &mut String,
    entry: &crate::analyzer::NavEntry,
    index: &ProjectIndex,
    current_dir: &std::path::Path,
    current_depth: usize,
    maxdepth: Option<usize>,
) {
    if maxdepth.is_some_and(|m| current_depth > m) {
        return;
    }

    let link_text = index
        .document_titles
        .get(&entry.path)
        .unwrap_or(&entry.path);

    let target_html = std::path::PathBuf::from(&entry.path).with_extension("html");
    let relative_path = pathdiff::diff_paths(&target_html, current_dir).unwrap_or(target_html);

    let href = format!("{}", relative_path.display());
    let escaped_text = html_escape::encode_text(link_text);

    let href_attr = html_escape::encode_double_quoted_attribute(&href);
    let _ = write!(html, "  <li><a href=\"{href_attr}\">{escaped_text}</a>");

    if !entry.children.is_empty() && maxdepth.is_none_or(|m| current_depth < m) {
        let _ = writeln!(html, "\n<ul>");
        for child in &entry.children {
            render_nav_entry(html, child, index, current_dir, current_depth + 1, maxdepth);
        }
        let _ = write!(html, "</ul>\n  ");
    }
    let _ = writeln!(html, "</li>");
}

fn find_nav_entry<'a>(
    entries: &'a [crate::analyzer::NavEntry],
    path: &str,
) -> Option<&'a crate::analyzer::NavEntry> {
    for entry in entries {
        if entry.path == path {
            return Some(entry);
        }
        if let Some(found) = find_nav_entry(&entry.children, path) {
            return Some(found);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{HashedContent, InlineNode};

    #[test]
    fn test_render_returns_empty_string_for_empty_document() {
        // Given
        let doc = Document::new("test.rst".to_string(), vec![]);
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path);

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
                    text: "Title".to_string(),
                },
                Node::Paragraph(vec![crate::ast::InlineNode::Text("Paragraph".to_string())]),
                Node::Heading {
                    level: 1,
                    text: "Another Heading".to_string(),
                },
            ],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path);

        // Then
        assert_eq!(
            result,
            "<h1>Title</h1>\n<p>Paragraph</p>\n<h1>Another Heading</h1>\n"
        );
    }

    #[test]
    fn test_render_escapes_html_special_characters() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![
                Node::Heading {
                    level: 1,
                    text: "Title <script>".to_string(),
                },
                Node::Paragraph(vec![crate::ast::InlineNode::Text("A & B > C".to_string())]),
            ],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path);

        // Then
        assert_eq!(
            result,
            "<h1>Title &lt;script&gt;</h1>\n<p>A &amp; B &gt; C</p>\n"
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
        index.nav_tree = vec![crate::analyzer::NavEntry {
            title: "test".to_string(),
            path: "test.rst".to_string(),
            children: vec![
                crate::analyzer::NavEntry {
                    title: "Team A Module".to_string(),
                    path: "team_a/index.rst".to_string(),
                    children: vec![],
                },
                crate::analyzer::NavEntry {
                    title: "team_b/index".to_string(),
                    path: "team_b/index.rst".to_string(),
                    children: vec![],
                },
            ],
        }];

        // When
        let result = render(&doc, &index, &doc.path);

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
                text: "Top".to_string(),
            }],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path);

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
                text: "Sub".to_string(),
            }],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path);

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
                text: "Deep".to_string(),
            }],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path);

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
                text: "VeryDeep".to_string(),
            }],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path);

        // Then
        assert_eq!(result, "<h6>VeryDeep</h6>\n");
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
        let result = render(&doc, &index, &doc.path);

        // Then
        assert_eq!(result, "<a id=\"section-1\"></a>\n");
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
        let result = render(&doc, &index, &doc.path);

        // Then
        assert_eq!(result, "");
    }

    #[test]
    fn test_render_formats_inline_reference_using_project_index() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Paragraph(vec![crate::ast::InlineNode::Reference(
                "other-section".to_string(),
            )])],
        );
        let mut index = ProjectIndex::default();
        index.targets.insert(
            TargetName::new("other-section"),
            TargetLocation::Internal("other_file.rst".to_string()),
        );

        // When
        let result = render(&doc, &index, &doc.path);

        // Then
        assert_eq!(
            result,
            "<p><a href=\"other_file.html#other-section\">other-section</a></p>\n"
        );
    }

    #[test]
    fn test_render_resolves_cross_directory_references_as_relative_links() {
        // Given a document in a subdirectory
        let doc = Document::new(
            "examples/team_b/index.rst".to_string(),
            vec![Node::Paragraph(vec![crate::ast::InlineNode::Reference(
                "target-in-a".to_string(),
            )])],
        );

        let mut index = ProjectIndex::default();
        index.targets.insert(
            TargetName::new("target-in-a"),
            TargetLocation::Internal("examples/team_a/index.rst".to_string()),
        );

        // When
        let result = render(&doc, &index, &doc.path);

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
            vec![Node::Paragraph(vec![crate::ast::InlineNode::Hyperlink {
                text: "Python".to_string(),
                target: "Python".to_string(),
            }])],
        );
        let mut index = ProjectIndex::default();
        index.targets.insert(
            TargetName::new("Python"),
            TargetLocation::External("https://python.org".to_string()),
        );

        // When
        let result = render(&doc, &index, &doc.path);

        // Then
        assert_eq!(result, "<p><a href=\"https://python.org\">Python</a></p>\n");
    }

    #[test]
    fn test_render_formats_direct_uri_hyperlink() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Paragraph(vec![crate::ast::InlineNode::Hyperlink {
                text: "Google".to_string(),
                target: "https://google.com".to_string(),
            }])],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path);

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
        let result = render(&doc, &index, &doc.path);

        // Then the image src should point backwards up out of team_b/ and examples/ and into _images/
        let expected = format!(
            "<div class=\"plantuml-diagram\">\n  <img src=\"../../_images/{expected_hash}.svg\" alt=\"PlantUML Diagram\" />\n</div>\n"
        );
        assert_eq!(result, expected);
    }

    #[test]
    fn test_render_bullet_list() {
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::BulletList {
                bullet: '*',
                items: vec![
                    crate::ast::BulletListItem {
                        nodes: vec![Node::Paragraph(vec![InlineNode::Text(
                            "Item 1".to_string(),
                        )])],
                    },
                    crate::ast::BulletListItem {
                        nodes: vec![Node::Paragraph(vec![InlineNode::Text(
                            "Item 2".to_string(),
                        )])],
                    },
                ],
            }],
        );
        let index = ProjectIndex::default();
        let result = render(&doc, &index, &doc.path);
        assert_eq!(
            result,
            "<ul>\n<li><p>Item 1</p>\n</li>\n<li><p>Item 2</p>\n</li>\n</ul>\n"
        );
    }

    #[test]
    fn test_render_bullet_list_nested() {
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::BulletList {
                bullet: '*',
                items: vec![crate::ast::BulletListItem {
                    nodes: vec![
                        Node::Paragraph(vec![InlineNode::Text("Parent".to_string())]),
                        Node::BulletList {
                            bullet: '-',
                            items: vec![crate::ast::BulletListItem {
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
        let result = render(&doc, &index, &doc.path);
        assert!(result.contains(
            "<ul>\n<li><p>Parent</p>\n<ul>\n<li><p>Child</p>\n</li>\n</ul>\n</li>\n</ul>"
        ));
    }

    #[test]
    fn test_render_bullet_list_multi_paragraph() {
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::BulletList {
                bullet: '*',
                items: vec![crate::ast::BulletListItem {
                    nodes: vec![
                        Node::Paragraph(vec![InlineNode::Text("Para 1".to_string())]),
                        Node::Paragraph(vec![InlineNode::Text("Para 2".to_string())]),
                    ],
                }],
            }],
        );
        let index = ProjectIndex::default();
        let result = render(&doc, &index, &doc.path);
        assert!(result.contains("<li><p>Para 1</p>\n<p>Para 2</p>\n</li>"));
    }

    #[test]
    fn test_css_relative_path_returns_filename_for_root_document() {
        // Given a document at the root level
        let doc_path = "index.rst";

        // When
        let result = css_relative_path(doc_path, "default.css");

        // Then
        assert_eq!(result, "default.css");
    }

    #[test]
    fn test_css_relative_path_returns_parent_prefix_for_nested_document() {
        // Given a document one directory deep
        let doc_path = "team_a/index.rst";

        // When
        let result = css_relative_path(doc_path, "default.css");

        // Then
        assert_eq!(result, "../default.css");
    }

    #[test]
    fn test_css_relative_path_returns_double_parent_for_deeply_nested_document() {
        // Given a document two directories deep
        let doc_path = "examples/team_b/index.rst";

        // When
        let result = css_relative_path(doc_path, "default.css");

        // Then
        assert_eq!(result, "../../default.css");
    }

    #[test]
    fn test_render_page_renders_body_into_template() {
        // Given
        let template = "<html><body>{{ body }}</body></html>";
        let config = SiteConfig::default();
        let body = "<h1>Title</h1>\n";

        // When
        let result = render_page(body, template, &config, "default.css", "Title", "", &[]).unwrap();

        // Then
        assert!(result.contains("<h1>Title</h1>"));
        assert!(result.starts_with("<html>"));
    }

    #[test]
    fn test_render_page_injects_project_name() {
        // Given
        let template = "<title>{{ project }}</title>{{ body }}";
        let config = SiteConfig {
            project: "MyProject".to_string(),
            ..SiteConfig::default()
        };

        // When
        let result = render_page("body", template, &config, "default.css", "", "", &[]).unwrap();

        // Then
        assert!(result.contains("<title>MyProject</title>"));
    }

    #[test]
    fn test_render_page_injects_css_path() {
        // Given
        let template = r#"<link href="{{ css_path }}">{{ body }}"#;
        let config = SiteConfig::default();

        // When
        let result =
            render_page("body", template, &config, "../../default.css", "", "", &[]).unwrap();

        // Then
        assert!(result.contains(r#"<link href="../../default.css">"#));
    }

    #[test]
    fn test_render_page_returns_error_for_invalid_template() {
        // Given
        let template = "{% if unclosed";
        let config = SiteConfig::default();

        // When
        let result = render_page("body", template, &config, "default.css", "", "", &[]);

        // Then
        assert!(result.is_err());
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
            nav_tree: vec![crate::analyzer::NavEntry {
                path: "cycle.rst".to_string(),
                title: "Cycle".to_string(),
                children: vec![crate::analyzer::NavEntry {
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
        let html = render(&doc, &index, &doc.path);

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
        let result = render(&doc, &index, &doc.path);

        // Then — no crash, just an empty list
        assert_eq!(result, "<ul>\n</ul>\n");
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
        index.nav_tree = vec![crate::analyzer::NavEntry {
            title: "Root".to_string(),
            path: "index.rst".to_string(),
            children: vec![crate::analyzer::NavEntry {
                title: "Child".to_string(),
                path: "child.rst".to_string(),
                children: vec![crate::analyzer::NavEntry {
                    title: "Grandchild".to_string(),
                    path: "grandchild.rst".to_string(),
                    children: vec![],
                }],
            }],
        }];

        // When
        let result = render(&doc, &index, &doc.path);

        // Then — child appears but grandchild is suppressed by maxdepth: 1
        assert!(result.contains("Child"), "child should be rendered");
        assert!(
            !result.contains("Grandchild"),
            "grandchild must be suppressed by maxdepth:1"
        );
    }

    // ── find_nav_entry ────────────────────────────────────────────────────────

    fn make_entry(
        path: &str,
        children: Vec<crate::analyzer::NavEntry>,
    ) -> crate::analyzer::NavEntry {
        crate::analyzer::NavEntry {
            title: path.to_string(),
            path: path.to_string(),
            children,
        }
    }

    #[test]
    fn test_find_nav_entry_returns_none_for_empty_tree() {
        assert!(find_nav_entry(&[], "a.rst").is_none());
    }

    #[test]
    fn test_find_nav_entry_finds_root_level_entry() {
        let tree = vec![make_entry("a.rst", vec![]), make_entry("b.rst", vec![])];
        let result = find_nav_entry(&tree, "b.rst");
        assert!(result.is_some());
        assert_eq!(result.unwrap().path, "b.rst");
    }

    #[test]
    fn test_find_nav_entry_finds_deeply_nested_entry() {
        let tree = vec![make_entry(
            "root.rst",
            vec![make_entry(
                "child.rst",
                vec![make_entry("grandchild.rst", vec![])],
            )],
        )];
        let result = find_nav_entry(&tree, "grandchild.rst");
        assert!(result.is_some());
        assert_eq!(result.unwrap().path, "grandchild.rst");
    }

    #[test]
    fn test_find_nav_entry_returns_none_for_missing_path() {
        let tree = vec![make_entry("a.rst", vec![make_entry("b.rst", vec![])])];
        assert!(find_nav_entry(&tree, "missing.rst").is_none());
    }

    // ── render_nav_entry ──────────────────────────────────────────────────────

    #[test]
    fn test_render_nav_entry_renders_leaf_as_list_item() {
        // Given
        let entry = make_entry("page.rst", vec![]);
        let mut index = ProjectIndex::default();
        index
            .document_titles
            .insert("page.rst".to_string(), "My Page".to_string());
        let current_dir = std::path::Path::new("");
        let mut html = String::new();

        // When
        render_nav_entry(&mut html, &entry, &index, current_dir, 1, None);

        // Then
        assert_eq!(html, "  <li><a href=\"page.html\">My Page</a></li>\n");
    }

    #[test]
    fn test_render_nav_entry_falls_back_to_path_when_no_title() {
        // Given — entry has no title in document_titles
        let entry = make_entry("section/page.rst", vec![]);
        let index = ProjectIndex::default(); // no titles
        let current_dir = std::path::Path::new("");
        let mut html = String::new();

        // When
        render_nav_entry(&mut html, &entry, &index, current_dir, 1, None);

        // Then — path is used as fallback link text
        assert!(
            html.contains("section/page.rst"),
            "path should be used as fallback title"
        );
        assert!(
            html.contains("section/page.html"),
            "href should point to html"
        );
    }

    #[test]
    fn test_render_nav_entry_renders_children_as_nested_list() {
        // Given
        let child = make_entry("child.rst", vec![]);
        let entry = make_entry("parent.rst", vec![child]);
        let mut index = ProjectIndex::default();
        index
            .document_titles
            .insert("parent.rst".to_string(), "Parent".to_string());
        index
            .document_titles
            .insert("child.rst".to_string(), "Child".to_string());
        let current_dir = std::path::Path::new("");
        let mut html = String::new();

        // When
        render_nav_entry(&mut html, &entry, &index, current_dir, 1, None);

        // Then
        assert!(html.contains("<ul>"), "nested list should be opened");
        assert!(html.contains("Child"), "child link text should be present");
        assert!(html.contains("child.html"), "child href should be present");
    }

    #[test]
    fn test_render_nav_entry_suppresses_children_when_maxdepth_reached() {
        // Given — maxdepth: 1, rendering at depth 1 means children must not appear
        let child = make_entry("child.rst", vec![]);
        let entry = make_entry("parent.rst", vec![child]);
        let mut index = ProjectIndex::default();
        index
            .document_titles
            .insert("parent.rst".to_string(), "Parent".to_string());
        index
            .document_titles
            .insert("child.rst".to_string(), "Child".to_string());
        let current_dir = std::path::Path::new("");
        let mut html = String::new();

        // When — rendered at depth 1 with maxdepth 1, children should not appear
        render_nav_entry(&mut html, &entry, &index, current_dir, 1, Some(1));

        // Then
        assert!(html.contains("Parent"));
        assert!(
            !html.contains("Child"),
            "child must be suppressed when depth == maxdepth"
        );
        assert!(
            !html.contains("<ul>"),
            "nested list must not be emitted when depth == maxdepth"
        );
    }

    #[test]
    fn test_render_nav_entry_skips_entirely_when_depth_exceeds_maxdepth() {
        // Given — called at depth 2 with maxdepth 1 (already exceeded)
        let entry = make_entry("page.rst", vec![]);
        let index = ProjectIndex::default();
        let current_dir = std::path::Path::new("");
        let mut html = String::new();

        // When
        render_nav_entry(&mut html, &entry, &index, current_dir, 2, Some(1));

        // Then — nothing rendered
        assert!(
            html.is_empty(),
            "nothing should be rendered when depth > maxdepth"
        );
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
                body: vec![Node::Paragraph(vec![crate::ast::InlineNode::Text(
                    "Note body".to_string(),
                )])],
            })],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path);

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
                body: vec![Node::Paragraph(vec![crate::ast::InlineNode::Text(
                    "Warning body".to_string(),
                )])],
            })],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path);

        // Then
        assert!(result.contains("<details class=\"admonition warning\">"));
        assert!(result.contains("<summary class=\"admonition-title\">Custom Warning</summary>"));
        assert!(result.contains("<p>Warning body</p>"));
    }

    #[test]
    fn test_render_nav_entry_computes_relative_path_from_subdirectory() {
        // Given — the current document is inside a subdirectory
        let entry = make_entry("page.rst", vec![]);
        let mut index = ProjectIndex::default();
        index
            .document_titles
            .insert("page.rst".to_string(), "Top Page".to_string());
        let current_dir = std::path::Path::new("sub/dir");
        let mut html = String::new();

        // When
        render_nav_entry(&mut html, &entry, &index, current_dir, 1, None);

        // Then — href should be relative (../../page.html)
        // Then — href should be relative (../../page.html)
        assert!(
            html.contains("../../page.html"),
            "href must be relative to current_dir"
        );
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
        let result = render(&doc, &index, &doc.path);

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
        let result = render(&doc, &index, &doc.path);

        // Then
        assert!(result.contains("<a href=\"https://embedded.com\">Embedded</a>"));
        assert!(result.contains("<a href=\"https://target.com\">Reference</a>"));
    }

    #[test]
    fn test_render_admonition_static() {
        // Given
        let mut html = String::new();
        let kind = crate::ast::AdmonitionKind::Note;
        let title: Option<String> = None;
        let collapsible: Option<bool> = None;
        let body = vec![Node::Paragraph(vec![crate::ast::InlineNode::Text(
            "Body".to_string(),
        )])];
        let index = ProjectIndex::default();
        let doc_path = "test.rst";
        let anon_targets = vec![];
        let mut anon_index = 0;
        let original_doc_path = "test.rst";

        // When
        render_admonition(
            &mut html,
            kind,
            title.as_deref(),
            collapsible,
            &body,
            &index,
            doc_path,
            &anon_targets,
            &mut anon_index,
            original_doc_path,
        );

        // Then
        assert!(html.contains("<div class=\"admonition note\">"));
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
        let doc_path = "test.rst";
        let anon_targets = vec![];
        let mut anon_index = 0;
        let original_doc_path = "test.rst";

        // When
        render_admonition(
            &mut html,
            kind,
            title.as_deref(),
            collapsible,
            &body,
            &index,
            doc_path,
            &anon_targets,
            &mut anon_index,
            original_doc_path,
        );

        // Then
        assert!(html.contains("<details class=\"admonition warning\" open>"));
        assert!(html.contains("<summary class=\"admonition-title\">Custom Title</summary>"));
    }

    #[test]
    fn test_render_admonition_collapsible_closed() {
        // Given
        let mut html = String::new();
        let kind = crate::ast::AdmonitionKind::Hint;
        let title: Option<String> = None;
        let collapsible = Some(false);
        let body = vec![];
        let index = ProjectIndex::default();
        let doc_path = "test.rst";
        let anon_targets = vec![];
        let mut anon_index = 0;
        let original_doc_path = "test.rst";

        // When
        render_admonition(
            &mut html,
            kind,
            title.as_deref(),
            collapsible,
            &body,
            &index,
            doc_path,
            &anon_targets,
            &mut anon_index,
            original_doc_path,
        );

        // Then
        assert!(html.contains("<details class=\"admonition hint\">"));
        assert!(!html.contains(" open>"));
        assert!(html.contains("<summary class=\"admonition-title\">Hint</summary>"));
    }

    #[test]
    fn test_render_broken_anonymous_link_fallback() {
        // Given — more references than targets
        let doc = Document::new(
            "test.rst".to_string(),
            vec![
                Node::Paragraph(vec![InlineNode::AnonymousReference("Missing".to_string())]),
                // No targets
            ],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path);

        // Then
        assert!(result.contains("class=\"broken-link\""));
    }

    #[test]
    fn test_render_formats_emphasis_and_strong_nodes() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Paragraph(vec![
                InlineNode::Text("Go to ".to_string()),
                InlineNode::Emphasis("emphasis".to_string()),
                InlineNode::Text(" or ".to_string()),
                InlineNode::Strong("strong".to_string()),
                InlineNode::Text(".".to_string()),
            ])],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path);

        // Then
        assert_eq!(
            result,
            "<p>Go to <em>emphasis</em> or <strong>strong</strong>.</p>\n"
        );
    }

    #[test]
    fn test_render_formats_literal_nodes() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Paragraph(vec![
                InlineNode::Text("Run ".to_string()),
                InlineNode::Literal("cargo build".to_string()),
                InlineNode::Text(" now.".to_string()),
            ])],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path);

        // Then
        assert_eq!(result, "<p>Run <code>cargo build</code> now.</p>\n");
    }

    #[test]
    fn test_render_literal_nodes_escapes_html() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Paragraph(vec![InlineNode::Literal(
                "Vec<T>".to_string(),
            )])],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path);

        // Then
        assert_eq!(result, "<p><code>Vec&lt;T&gt;</code></p>\n");
    }

    #[test]
    fn test_render_escapes_xss_in_hyperlink_target() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Paragraph(vec![crate::ast::InlineNode::Hyperlink {
                text: "Click Here".to_string(),
                target: "https://example.com/\"><script>alert('xss')</script>".to_string(),
            }])],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index, &doc.path);

        // Then
        assert!(result.contains("<a href=\"https://example.com/&quot;&gt;&lt;script&gt;alert('xss')&lt;/script&gt;\">Click Here</a>"));
    }
}

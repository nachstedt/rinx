//! The renderer module converts the AST and `ProjectIndex` into HTML.

use crate::analyzer::ProjectIndex;
use crate::ast::{Directive, Document, Node};
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
    children: Vec<ResolvedNavEntry>,
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
            let href = pathdiff::diff_paths(target, doc_dir).map_or_else(
                || target_html.clone(),
                |p| p.to_string_lossy().replace('\\', "/"),
            );

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
pub fn render(doc: &Document, index: &ProjectIndex) -> String {
    let mut html = String::new();

    for node in &doc.nodes {
        match node {
            Node::Heading { level, text } => {
                let tag = format!("h{}", (*level).clamp(1, 6));
                let escaped_text = html_escape::encode_text(text);
                let _ = writeln!(html, "<{tag}>{escaped_text}</{tag}>");
            }
            Node::Paragraph(inlines) => {
                let _ = write!(html, "<p>");
                for inline in inlines {
                    match inline {
                        crate::ast::InlineNode::Text(text) => {
                            let escaped = html_escape::encode_text(text);
                            let _ = write!(html, "{escaped}");
                        }
                        crate::ast::InlineNode::Reference(target) => {
                            let target_escaped = html_escape::encode_text(target);
                            if let Some(target_path) = index.targets.get(target) {
                                let current_dir = std::path::Path::new(&doc.path)
                                    .parent()
                                    .unwrap_or(std::path::Path::new(""));
                                let target_html_path =
                                    std::path::Path::new(target_path).with_extension("html");

                                let relative_path =
                                    pathdiff::diff_paths(&target_html_path, current_dir)
                                        .unwrap_or(target_html_path);

                                // display() on Unix uses `/`, so it maps correctly to URLs
                                let href =
                                    format!("{}#{}", relative_path.display(), target_escaped);
                                let _ = write!(html, "<a href=\"{href}\">{target_escaped}</a>");
                            } else {
                                // Fallback, could print warning
                                let _ = write!(
                                    html,
                                    "<a href=\"#{target_escaped}\" class=\"broken-link\">{target_escaped}</a>"
                                );
                            }
                        }
                    }
                }
                let _ = writeln!(html, "</p>");
            }
            Node::Target(name) => {
                let escaped_name = html_escape::encode_text(name);
                let _ = writeln!(html, "<a name=\"{escaped_name}\"></a>");
            }
            Node::Directive(directive) => match directive {
                Directive::Toctree { paths } => {
                    let _ = writeln!(html, "<ul>");
                    let current_dir = std::path::Path::new(&doc.path)
                        .parent()
                        .unwrap_or(std::path::Path::new(""));
                    for path in paths {
                        let target_rst = current_dir.join(path).with_extension("rst");
                        let target_entry = target_rst.display().to_string();

                        let link_text = index.document_titles.get(&target_entry).unwrap_or(path);

                        let target_html = current_dir.join(path).with_extension("html");
                        let relative_path =
                            pathdiff::diff_paths(&target_html, current_dir).unwrap_or(target_html);

                        let href = format!("{}", relative_path.display());
                        let escaped_text = html_escape::encode_text(link_text);
                        let _ = writeln!(html, "  <li><a href=\"{href}\">{escaped_text}</a></li>");
                    }
                    let _ = writeln!(html, "</ul>");
                }
                Directive::PlantUml(content) => {
                    let escaped_hash = html_escape::encode_text(content.hash());

                    let current_dir = std::path::Path::new(&doc.path)
                        .parent()
                        .unwrap_or(std::path::Path::new(""));
                    let image_path =
                        std::path::Path::new("_images").join(format!("{escaped_hash}.svg"));
                    let relative_path =
                        pathdiff::diff_paths(&image_path, current_dir).unwrap_or(image_path);
                    let src = format!("{}", relative_path.display());

                    let _ = writeln!(html, "<div class=\"plantuml-diagram\">");
                    let _ = writeln!(html, "  <img src=\"{src}\" alt=\"PlantUML Diagram\" />");
                    let _ = writeln!(html, "</div>");
                }
                Directive::Unknown { .. } => {}
            },
        }
    }

    html
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::HashedContent;

    #[test]
    fn test_render_returns_empty_string_for_empty_document() {
        // Given
        let doc = Document::new("test.rst".to_string(), vec![]);
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index);

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
        let result = render(&doc, &index);

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
        let result = render(&doc, &index);

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
            })],
        );
        let mut index = ProjectIndex::default();
        index
            .document_titles
            .insert("team_a/index.rst".to_string(), "Team A Module".to_string());
        // team_b is missing, so it should fallback

        // When
        let result = render(&doc, &index);

        // Then
        assert_eq!(
            result,
            "<ul>\n  <li><a href=\"team_a/index.html\">Team A Module</a></li>\n  <li><a href=\"team_b/index.html\">team_b/index</a></li>\n</ul>\n"
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
        let result = render(&doc, &index);

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
        let result = render(&doc, &index);

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
        let result = render(&doc, &index);

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
        let result = render(&doc, &index);

        // Then
        assert_eq!(result, "<h6>VeryDeep</h6>\n");
    }
    #[test]
    fn test_render_formats_target_node_as_html_anchor() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Target("section-1".to_string())],
        );
        let index = ProjectIndex::default();

        // When
        let result = render(&doc, &index);

        // Then
        assert_eq!(result, "<a name=\"section-1\"></a>\n");
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
        index
            .targets
            .insert("other-section".to_string(), "other_file.rst".to_string());

        // When
        let result = render(&doc, &index);

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
            "target-in-a".to_string(),
            "examples/team_a/index.rst".to_string(),
        );

        // When
        let result = render(&doc, &index);

        // Then the link should point backwards up out of team_b/ and into team_a/
        assert_eq!(
            result,
            "<p><a href=\"../team_a/index.html#target-in-a\">target-in-a</a></p>\n"
        );
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
        let result = render(&doc, &index);

        // Then the image src should point backwards up out of team_b/ and examples/ and into _images/
        let expected = format!(
            "<div class=\"plantuml-diagram\">\n  <img src=\"../../_images/{expected_hash}.svg\" alt=\"PlantUML Diagram\" />\n</div>\n"
        );
        assert_eq!(result, expected);
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
}

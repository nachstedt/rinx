//! Page-level rendering: CSS path computation, template rendering, and nav href resolution.

use crate::config::SiteConfig;
use anyhow::{Context, Result};

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
/// The `body` is the inner HTML produced by [`super::render()`]. The `config` provides
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
struct ResolvedNavEntry<'a> {
    title: &'a str,
    href: String,
    children: Vec<ResolvedNavEntry<'a>>,
}

/// Converts a nav tree's `.rst` paths into relative `.html` hrefs
/// based on the current document's location.
fn resolve_nav_hrefs<'a>(
    entries: &'a [crate::analyzer::NavEntry],
    doc_path: &str,
) -> Vec<ResolvedNavEntry<'a>> {
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
                title: &entry.title,
                href,
                children: resolve_nav_hrefs(&entry.children, doc_path),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SiteConfig;

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
    fn test_resolve_nav_hrefs_same_directory() {
        // Given
        let entries = vec![crate::analyzer::NavEntry {
            title: "Index".to_string(),
            path: "index.rst".to_string(),
            children: vec![],
        }];

        // When
        let resolved = resolve_nav_hrefs(&entries, "doc.rst");

        // Then
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].title, "Index");
        assert_eq!(resolved[0].href, "index.html");
    }

    #[test]
    fn test_resolve_nav_hrefs_document_in_subdirectory() {
        // Given
        let entries = vec![crate::analyzer::NavEntry {
            title: "Index".to_string(),
            path: "index.rst".to_string(),
            children: vec![],
        }];

        // When
        let resolved = resolve_nav_hrefs(&entries, "sub/doc.rst");

        // Then
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].href, "../index.html");
    }

    #[test]
    fn test_resolve_nav_hrefs_target_in_subdirectory() {
        // Given
        let entries = vec![crate::analyzer::NavEntry {
            title: "Nested".to_string(),
            path: "sub/nested.rst".to_string(),
            children: vec![],
        }];

        // When
        let resolved = resolve_nav_hrefs(&entries, "doc.rst");

        // Then
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].href, "sub/nested.html");
    }

    #[test]
    fn test_resolve_nav_hrefs_nested_entries() {
        // Given
        let entries = vec![crate::analyzer::NavEntry {
            title: "Root".to_string(),
            path: "root.rst".to_string(),
            children: vec![crate::analyzer::NavEntry {
                title: "Child".to_string(),
                path: "sub/child.rst".to_string(),
                children: vec![],
            }],
        }];

        // When
        let resolved = resolve_nav_hrefs(&entries, "doc.rst");

        // Then
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].href, "root.html");
        assert_eq!(resolved[0].children.len(), 1);
        assert_eq!(resolved[0].children[0].href, "sub/child.html");
    }
}

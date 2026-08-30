//! Page-level rendering: CSS path computation and template rendering.

use super::nav_hrefs::resolve_nav_hrefs;
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

/// Per-page metadata [`render_page`] needs beyond the body/template/config
/// every page shares — bundled into one struct to keep `render_page`'s own
/// argument count down.
#[derive(Default)]
pub struct PageMeta<'a> {
    /// The relative path to the stylesheet.
    pub css_path: &'a str,
    pub page_title: &'a str,
    /// The `.rst` path of the page being rendered (used to compute relative
    /// links to other pages, e.g. via [`css_relative_path`]).
    pub doc_path: &'a str,
    /// Hierarchical sidebar navigation.
    pub nav_tree: &'a [rusty_sphinx_index::NavEntry],
    /// Whether a `genindex_href` link (computed via [`css_relative_path`],
    /// since `genindex.html` always lives at the site root like
    /// `default.css`) is made available to the template — sites with no
    /// index entries get no dead link.
    pub has_genindex: bool,
}

/// Renders a page body into a full HTML document using a `MiniJinja` template.
///
/// The `template_str` is Jinja2-compatible markup loaded from an `.html` file.
/// The `body` is the inner HTML produced by [`crate::render()`]. The `config`
/// provides project metadata; `meta` provides everything specific to this
/// one page (see [`PageMeta`]).
///
/// # Errors
///
/// Returns an error if the template cannot be parsed or rendered.
pub fn render_page(
    body: &str,
    template_str: &str,
    config: &SiteConfig,
    meta: &PageMeta<'_>,
) -> Result<String> {
    let resolved_nav = resolve_nav_hrefs(meta.nav_tree, meta.doc_path);
    let genindex_href = meta.has_genindex.then(|| {
        minijinja::Value::from_safe_string(css_relative_path(meta.doc_path, "genindex.html"))
    });

    let mut env = minijinja::Environment::new();
    env.add_template("page.html", template_str)
        .context("Failed to parse template")?;
    let tmpl = env
        .get_template("page.html")
        .context("Template not found")?;
    let ctx = minijinja::context! {
        body => minijinja::Value::from_safe_string(body.to_string()),
        project => &config.project,
        version => &config.version,
        css_path => minijinja::Value::from_safe_string(meta.css_path.to_string()),
        page_title => meta.page_title,
        nav_tree => resolved_nav,
        genindex_href => genindex_href,
    };
    tmpl.render(ctx).context("Failed to render template")
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
        let result = render_page(
            body,
            template,
            &config,
            &PageMeta {
                css_path: "default.css",
                page_title: "Title",
                ..PageMeta::default()
            },
        )
        .unwrap();

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
        let result = render_page(
            "body",
            template,
            &config,
            &PageMeta {
                css_path: "default.css",
                ..PageMeta::default()
            },
        )
        .unwrap();

        // Then
        assert!(result.contains("<title>MyProject</title>"));
    }

    #[test]
    fn test_render_page_injects_css_path() {
        // Given
        let template = r#"<link href="{{ css_path }}">{{ body }}"#;
        let config = SiteConfig::default();

        // When
        let result = render_page(
            "body",
            template,
            &config,
            &PageMeta {
                css_path: "../../default.css",
                ..PageMeta::default()
            },
        )
        .unwrap();

        // Then
        assert!(result.contains(r#"<link href="../../default.css">"#));
    }

    #[test]
    fn test_render_page_escapes_html_in_page_title() {
        // Given a page title containing raw HTML
        let template = "<title>{{ page_title }}</title>{{ body }}";
        let config = SiteConfig::default();
        let page_title = "Title <script>alert(1)</script>";

        // When
        let result = render_page(
            "body",
            template,
            &config,
            &PageMeta {
                css_path: "default.css",
                page_title,
                ..PageMeta::default()
            },
        )
        .unwrap();

        // Then
        assert!(!result.contains("<script>"));
        assert!(result.contains("&lt;script&gt;"));
    }

    #[test]
    fn test_render_page_escapes_html_in_project_name() {
        // Given a project name containing raw HTML
        let template = "<title>{{ project }}</title>{{ body }}";
        let config = SiteConfig {
            project: "A & <b>B</b>".to_string(),
            ..SiteConfig::default()
        };

        // When
        let result = render_page(
            "body",
            template,
            &config,
            &PageMeta {
                css_path: "default.css",
                ..PageMeta::default()
            },
        )
        .unwrap();

        // Then
        assert!(!result.contains("<b>"));
        assert!(result.contains("A &amp; &lt;b&gt;B&lt;&#x2f;b&gt;"));
    }

    #[test]
    fn test_render_page_escapes_html_in_version() {
        // Given a version string containing raw HTML
        let template = "<p>{{ version }}</p>{{ body }}";
        let config = SiteConfig {
            version: "1.0 <script>".to_string(),
            ..SiteConfig::default()
        };

        // When
        let result = render_page(
            "body",
            template,
            &config,
            &PageMeta {
                css_path: "default.css",
                ..PageMeta::default()
            },
        )
        .unwrap();

        // Then
        assert!(!result.contains("<script>"));
        assert!(result.contains("1.0 &lt;script&gt;"));
    }

    #[test]
    fn test_render_page_escapes_html_in_nav_entry_title() {
        // Given a nav entry whose title contains raw HTML
        let template = "{% for entry in nav_tree %}{{ entry.title }}{% endfor %}{{ body }}";
        let config = SiteConfig::default();
        let entries = vec![rusty_sphinx_index::NavEntry {
            title: "Evil <script>alert(1)</script>".to_string(),
            path: "evil.rst".to_string(),
            children: vec![],
        }];

        // When
        let result = render_page(
            "body",
            template,
            &config,
            &PageMeta {
                css_path: "default.css",
                nav_tree: &entries,
                ..PageMeta::default()
            },
        )
        .unwrap();

        // Then
        assert!(!result.contains("<script>"));
        assert!(result.contains("&lt;script&gt;"));
    }

    #[test]
    fn test_render_page_does_not_escape_body_html() {
        // Given a body containing pre-rendered, trusted HTML
        let template = "<main>{{ body }}</main>";
        let config = SiteConfig::default();
        let body = "<h1>Title</h1>\n<p>A &amp; B</p>\n";

        // When
        let result = render_page(
            body,
            template,
            &config,
            &PageMeta {
                css_path: "default.css",
                ..PageMeta::default()
            },
        )
        .unwrap();

        // Then
        assert!(result.contains("<h1>Title</h1>"));
        assert!(result.contains("<p>A &amp; B</p>"));
    }

    #[test]
    fn test_render_page_does_not_escape_nav_entry_href_slashes() {
        // Given a nested nav entry whose computed href contains slashes
        let template = "{% for entry in nav_tree %}{{ entry.href }}{% endfor %}{{ body }}";
        let config = SiteConfig::default();
        let entries = vec![rusty_sphinx_index::NavEntry {
            title: "Nested".to_string(),
            path: "sub/nested.rst".to_string(),
            children: vec![],
        }];

        // When
        let result = render_page(
            "body",
            template,
            &config,
            &PageMeta {
                css_path: "default.css",
                nav_tree: &entries,
                ..PageMeta::default()
            },
        )
        .unwrap();

        // Then
        assert!(result.contains("sub/nested.html"));
        assert!(!result.contains("&#x2f;"));
    }

    #[test]
    fn test_render_page_injects_genindex_href_when_has_genindex_true() {
        // Given a document one directory deep
        let template =
            "{% if genindex_href %}<a href=\"{{ genindex_href }}\">Index</a>{% endif %}{{ body }}";
        let config = SiteConfig::default();

        // When
        let result = render_page(
            "body",
            template,
            &config,
            &PageMeta {
                css_path: "default.css",
                doc_path: "team_a/index.rst",
                has_genindex: true,
                ..PageMeta::default()
            },
        )
        .unwrap();

        // Then
        assert!(result.contains("<a href=\"../genindex.html\">Index</a>"));
    }

    #[test]
    fn test_render_page_omits_genindex_href_when_has_genindex_false() {
        // Given
        let template =
            "{% if genindex_href %}<a href=\"{{ genindex_href }}\">Index</a>{% endif %}{{ body }}";
        let config = SiteConfig::default();

        // When
        let result = render_page(
            "body",
            template,
            &config,
            &PageMeta {
                css_path: "default.css",
                ..PageMeta::default()
            },
        )
        .unwrap();

        // Then
        assert!(!result.contains("Index</a>"));
    }

    #[test]
    fn test_render_page_returns_error_for_invalid_template() {
        // Given
        let template = "{% if unclosed";
        let config = SiteConfig::default();

        // When
        let result = render_page(
            "body",
            template,
            &config,
            &PageMeta {
                css_path: "default.css",
                ..PageMeta::default()
            },
        );

        // Then
        assert!(result.is_err());
    }
}

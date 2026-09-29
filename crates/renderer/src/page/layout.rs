//! Page-level rendering: CSS path computation and template rendering.

use crate::config::SiteConfig;
use crate::nav::{PageLink, ResolvedNavEntry, page_neighbors};
use anyhow::{Context, Result};
use rinx_index::ProjectIndex;

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
    /// The `.rst` path this page was parsed from, which is the key the
    /// navigation graph and the reading order are stored under.
    pub source_path: &'a str,
    /// The sidebar's navigation entries, already expanded and resolved.
    pub nav_tree: Vec<ResolvedNavEntry>,
    /// Whether a `genindex_href` link (computed via [`css_relative_path`],
    /// since `genindex.html` always lives at the site root like
    /// `default.css`) is made available to the template — sites with no
    /// index entries get no dead link.
    pub has_genindex: bool,
    /// The previous and next pages in reading order, for the template's
    /// page-relation links. Both `None` for a page no toctree reaches.
    pub previous: Option<PageLink>,
    pub next: Option<PageLink>,
}

impl PageMeta<'_> {
    /// Fills in everything that can be derived from the project index: the
    /// sidebar's entries and the page's neighbours.
    ///
    /// The sidebar expands the *root documents'* toctrees, which is what makes
    /// it the same tree on every page while still marking the current one;
    /// `config` decides how much of it is nested (see
    /// [`SiteConfig::sidebar_tree`]).
    #[must_use]
    pub fn with_navigation(mut self, index: &ProjectIndex, config: &SiteConfig) -> Self {
        self.nav_tree = crate::nav::sidebar_entries(
            index,
            self.doc_path,
            self.source_path,
            config.sidebar_tree,
        );
        let (previous, next) = page_neighbors(index, self.source_path, self.doc_path);
        self.previous = previous;
        self.next = next;
        self
    }
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
    let genindex_href = meta.has_genindex.then(|| {
        minijinja::Value::from_safe_string(css_relative_path(meta.doc_path, "genindex.html"))
    });

    let version_switcher = version_switcher_context(config, meta.doc_path);

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
        nav_tree => &meta.nav_tree,
        genindex_href => genindex_href,
        prev => &meta.previous,
        next => &meta.next,
        version_switcher => version_switcher,
    };
    tmpl.render(ctx).context("Failed to render template")
}

/// The name `rinx_site` copies the version switcher's script under, next to
/// the stylesheet at the site root.
const VERSION_SWITCHER_SCRIPT: &str = "version_switcher.js";

/// What the template needs to draw a version switcher on the page at
/// `doc_path` — where to fetch the version list and where the script is,
/// relative to this page — or nothing when the site configures none.
fn version_switcher_context(config: &SiteConfig, doc_path: &str) -> Option<minijinja::Value> {
    config.version_switcher.as_ref().map(|switcher| {
        minijinja::context! {
            // Escaped by the template like any text: a query string's `&`
            // must not be read as the start of a character reference.
            json_url => switcher.json_url.as_str(),
            script => minijinja::Value::from_safe_string(
                css_relative_path(doc_path, VERSION_SWITCHER_SCRIPT),
            ),
        }
    })
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
        let entries = vec![crate::nav::ResolvedNavEntry::Page {
            title: "Evil <script>alert(1)</script>".to_string(),
            href: minijinja::Value::from_safe_string("evil.html".to_string()),
            anchor: None,
            secnumber: None,
            is_current: false,
            is_ancestor: false,
            children: vec![],
        }];

        // When
        let result = render_page(
            "body",
            template,
            &config,
            &PageMeta {
                css_path: "default.css",
                nav_tree: entries,
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
        let entries = vec![crate::nav::ResolvedNavEntry::Page {
            title: "Nested".to_string(),
            href: minijinja::Value::from_safe_string("sub/nested.html".to_string()),
            anchor: None,
            secnumber: None,
            is_current: false,
            is_ancestor: false,
            children: vec![],
        }];

        // When
        let result = render_page(
            "body",
            template,
            &config,
            &PageMeta {
                css_path: "default.css",
                nav_tree: entries,
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

    fn config_with_switcher(json_url: &str) -> SiteConfig {
        SiteConfig {
            version_switcher: Some(crate::config::VersionSwitcher {
                json_url: crate::config::SwitcherUrl::parse(json_url).unwrap(),
            }),
            ..SiteConfig::default()
        }
    }

    #[test]
    fn test_version_switcher_context_is_absent_without_config() {
        // Given
        let config = SiteConfig::default();

        // When
        let context = version_switcher_context(&config, "index.rst");

        // Then
        assert!(context.is_none());
    }

    #[test]
    fn test_version_switcher_context_points_at_the_script_relative_to_the_page() {
        // Given — a page two directories below the site root
        let config = config_with_switcher("/versions.json");

        // When
        let context = version_switcher_context(&config, "docs/guide/intro.rst").unwrap();

        // Then
        assert_eq!(
            context.get_attr("script").unwrap().as_str(),
            Some("../../version_switcher.js")
        );
        assert_eq!(
            context.get_attr("json_url").unwrap().as_str(),
            Some("/versions.json")
        );
    }

    #[test]
    fn test_render_page_exposes_the_version_switcher_to_the_template() {
        // Given
        let template = r#"{% if version_switcher %}<script src="{{ version_switcher.script }}" data-url="{{ version_switcher.json_url }}"></script>{% endif %}"#;
        let config = config_with_switcher("https://example.org/versions.json");

        // When
        let result = render_page(
            "body",
            template,
            &config,
            &PageMeta {
                doc_path: "team/index.rst",
                ..PageMeta::default()
            },
        )
        .unwrap();

        // Then — the URL is escaped as attribute text, which the browser
        // decodes back to what was configured
        assert_eq!(
            result,
            r#"<script src="../version_switcher.js" data-url="https:&#x2f;&#x2f;example.org&#x2f;versions.json"></script>"#
        );
    }

    #[test]
    fn test_render_page_mentions_no_switcher_without_config() {
        // Given — the default template's own guard
        let template = "{% if version_switcher %}switcher{% endif %}{{ body }}";

        // When
        let result = render_page(
            "body",
            template,
            &SiteConfig::default(),
            &PageMeta::default(),
        )
        .unwrap();

        // Then
        assert_eq!(result, "body");
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

    /// The theme's own page template, so the sidebar's folding is tested
    /// against the markup that actually ships.
    const DEFAULT_TEMPLATE: &str = include_str!("../../../../templates/default.html");

    fn nav_page(
        title: &str,
        is_current: bool,
        children: Vec<ResolvedNavEntry>,
    ) -> ResolvedNavEntry {
        let is_ancestor = children.iter().any(ResolvedNavEntry::contains_current);
        ResolvedNavEntry::Page {
            title: title.to_string(),
            href: minijinja::Value::from_safe_string(format!("{title}.html")),
            anchor: None,
            secnumber: None,
            is_current,
            is_ancestor,
            children,
        }
    }

    /// Renders `nav_tree` through the default template and returns the
    /// sidebar's `<nav>` alone.
    fn render_default_sidebar(nav_tree: Vec<ResolvedNavEntry>) -> String {
        let page = render_page(
            "body",
            DEFAULT_TEMPLATE,
            &SiteConfig::default(),
            &PageMeta {
                css_path: "default.css",
                nav_tree,
                ..PageMeta::default()
            },
        )
        .unwrap();
        let start = page.find("<nav class=\"sidebar-nav\">").unwrap();
        let end = start + page[start..].find("</nav>").unwrap();
        page[start..end].to_string()
    }

    #[test]
    fn test_default_template_unfolds_the_path_to_the_current_page() {
        // Given — the current page sits two levels down.
        let nav_tree = vec![nav_page(
            "guide",
            false,
            vec![nav_page(
                "setup",
                true,
                vec![nav_page("setup-linux", false, vec![])],
            )],
        )];

        // When
        let sidebar = render_default_sidebar(nav_tree);

        // Then — the ancestor and the current page are both open.
        assert_eq!(sidebar.matches("<details open>").count(), 2, "{sidebar}");
        assert!(!sidebar.contains("<details>"), "{sidebar}");
    }

    #[test]
    fn test_default_template_folds_a_branch_off_the_current_path() {
        // Given
        let nav_tree = vec![
            nav_page("intro", true, vec![]),
            nav_page("guide", false, vec![nav_page("setup", false, vec![])]),
        ];

        // When
        let sidebar = render_default_sidebar(nav_tree);

        // Then
        assert!(sidebar.contains("<details>"), "{sidebar}");
        assert!(!sidebar.contains("<details open>"), "{sidebar}");
    }

    #[test]
    fn test_default_template_gives_a_leaf_no_toggle() {
        // Given
        let nav_tree = vec![nav_page("intro", false, vec![])];

        // When
        let sidebar = render_default_sidebar(nav_tree);

        // Then
        assert!(!sidebar.contains("<details"), "{sidebar}");
        assert!(!sidebar.contains("has-children"), "{sidebar}");
    }

    #[test]
    fn test_default_template_keeps_a_branch_link_outside_its_toggle() {
        // Given
        let nav_tree = vec![nav_page(
            "guide",
            false,
            vec![nav_page("setup", false, vec![])],
        )];

        // When
        let sidebar = render_default_sidebar(nav_tree);

        // Then — the title link comes before the `<details>`, so clicking it
        // navigates rather than folding.
        let link = sidebar.find("<a href=\"guide.html\">").unwrap();
        let details = sidebar.find("<details").unwrap();
        assert!(link < details, "{sidebar}");
    }

    #[test]
    fn test_default_template_numbers_nesting_levels_as_sphinx_does() {
        // Given
        let nav_tree = vec![nav_page(
            "guide",
            false,
            vec![nav_page("setup", false, vec![])],
        )];

        // When
        let sidebar = render_default_sidebar(nav_tree);

        // Then
        assert!(
            sidebar.contains("class=\"toctree-l1 has-children\""),
            "{sidebar}"
        );
        assert!(sidebar.contains("class=\"toctree-l2\""), "{sidebar}");
    }
}

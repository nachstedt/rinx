//! `:download:` rendering: a link to a file the site serves from its
//! `_downloads/` directory.
//!
//! Nothing here consults the project index. The file is not something a
//! document defines, so there is nothing to resolve: its `href` follows from
//! where the page sits and where the file was written, through the same
//! [`AssetUri::resolve`] the site's validation action checks the declaration
//! with — so the page cannot link a path the build did not copy.

use std::fmt::Write as _;

use rinx_ast::AssetUri;

use crate::asset_href::{AssetDir, relative_asset_href};

/// A `:download:` as the author wrote it.
#[derive(Debug, Clone, Copy)]
pub(super) struct DownloadRef<'a> {
    /// The explicit title of the `Title <file>` form, if one was written.
    pub title: Option<&'a str>,
    /// The file, as written: an external URL or a project file.
    pub target: &'a AssetUri,
    /// `false` for the `!` form, which is neither linked nor copied.
    pub link: bool,
}

/// Renders a `:download:` reference as Sphinx 9.1's HTML builder does: an
/// `<a download>` around a `xref download` literal showing the explicit title,
/// else the target as written.
///
/// A project file links into `_downloads/` at its source-root-relative path;
/// an external URL is linked as written and marked `external`, and never
/// copied. The `!` form is the literal alone.
pub(super) fn render_inline_download_reference(
    html: &mut String,
    reference: DownloadRef<'_>,
    doc_path: &str,
) {
    let DownloadRef {
        title,
        target,
        link,
    } = reference;
    let text = title.unwrap_or_else(|| target.as_written());
    if !link {
        write_download_literal(html, text);
        return;
    }
    let (locality, href) = match target {
        AssetUri::External(uri) => ("external", uri.clone()),
        AssetUri::Document(_) => {
            // `resolve` answers `None` only for an external URI.
            let resolved = target.resolve(doc_path).unwrap_or_default();
            (
                "internal",
                relative_asset_href(AssetDir::Downloads, &resolved, doc_path),
            )
        }
    };
    let _ = write!(
        html,
        "<a class=\"reference download {locality}\" download=\"\" href=\"{}\">",
        html_escape::encode_double_quoted_attribute(&href)
    );
    write_download_literal(html, text);
    html.push_str("</a>");
}

/// Writes `text` as Sphinx's `xref download` literal, each run of
/// non-whitespace in its own `<span class="pre">` as docutils' HTML writer
/// protects literal text, so a long path can still break between words.
fn write_download_literal(html: &mut String, text: &str) {
    html.push_str("<code class=\"xref download docutils literal notranslate\">");
    let mut rest = text;
    while !rest.is_empty() {
        let word_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        let (word, after) = rest.split_at(word_end);
        if !word.is_empty() {
            let _ = write!(
                html,
                "<span class=\"pre\">{}</span>",
                html_escape::encode_text(word)
            );
        }
        let space_end = after
            .find(|character: char| !character.is_whitespace())
            .unwrap_or(after.len());
        if space_end > 0 {
            // One space stays a space; a longer run keeps its width, as
            // docutils writes it: non-breaking spaces and a final plain one.
            html.push_str(&"&#160;".repeat(after[..space_end].chars().count() - 1));
            html.push(' ');
        }
        rest = &after[space_end..];
    }
    html.push_str("</code>");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(title: Option<&str>, target: &str, link: bool, doc_path: &str) -> String {
        let target = AssetUri::new(target);
        let mut html = String::new();
        render_inline_download_reference(
            &mut html,
            DownloadRef {
                title,
                target: &target,
                link,
            },
            doc_path,
        );
        html
    }

    #[test]
    fn test_links_a_file_beside_a_root_level_page() {
        // Given / When
        let html = render(None, "sample.csv", true, "index.rst");

        // Then
        assert_eq!(
            html,
            "<a class=\"reference download internal\" download=\"\" \
             href=\"_downloads/sample.csv\"><code class=\"xref download docutils literal \
             notranslate\"><span class=\"pre\">sample.csv</span></code></a>"
        );
    }

    #[test]
    fn test_climbs_out_of_a_nested_page_to_the_downloads_directory() {
        // Given / When
        let html = render(None, "../data/sample.csv", true, "guide/intro.rst");

        // Then — the file keeps its source-root-relative path
        assert!(
            html.contains("href=\"../_downloads/data/sample.csv\""),
            "{html}"
        );
    }

    #[test]
    fn test_reads_a_leading_slash_from_the_source_root() {
        // Given / When
        let html = render(None, "/shared/tool.py", true, "a/b/page.rst");

        // Then
        assert!(
            html.contains("href=\"../../_downloads/shared/tool.py\""),
            "{html}"
        );
    }

    #[test]
    fn test_shows_the_explicit_title() {
        // Given / When
        let html = render(Some("the data"), "sample.csv", true, "index.rst");

        // Then
        assert!(
            html.contains("<span class=\"pre\">the</span> <span class=\"pre\">data</span>"),
            "{html}"
        );
    }

    #[test]
    fn test_links_an_external_url_as_written() {
        // Given / When
        let html = render(None, "https://example.com/tool.zip", true, "a/page.rst");

        // Then
        assert!(
            html.starts_with(
                "<a class=\"reference download external\" download=\"\" \
                 href=\"https://example.com/tool.zip\">"
            ),
            "{html}"
        );
    }

    #[test]
    fn test_draws_the_bang_form_as_an_unlinked_literal() {
        // Given / When
        let html = render(None, "sample.csv", false, "index.rst");

        // Then
        assert_eq!(
            html,
            "<code class=\"xref download docutils literal notranslate\">\
             <span class=\"pre\">sample.csv</span></code>"
        );
    }

    #[test]
    fn test_escapes_the_text_and_the_href() {
        // Given / When
        let html = render(Some("<a&b>"), "x\"y.txt", true, "index.rst");

        // Then
        assert!(html.contains("&lt;a&amp;b&gt;"), "{html}");
        assert!(html.contains("href=\"_downloads/x&quot;y.txt\""), "{html}");
    }

    #[test]
    fn test_write_download_literal_keeps_a_run_of_spaces_wide() {
        // Given
        let mut html = String::new();

        // When
        write_download_literal(&mut html, "a   b");

        // Then
        assert_eq!(
            html,
            "<code class=\"xref download docutils literal notranslate\">\
             <span class=\"pre\">a</span>&#160;&#160; <span class=\"pre\">b</span></code>"
        );
    }
}

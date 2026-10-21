//! Following a link of the preview (roadmap #40): the source a page's `href`
//! leads to.
//!
//! The preview is the page the build would write, so its links carry the
//! site's addresses — `../library/os.html#os.getcwd`, `../_downloads/a.csv`
//! — which a webview has no site to follow into. The editor sends the `href`
//! as written back with a `rinx/previewLink` request, and the server reads it
//! as the inverse of the two functions that wrote it:
//! `rinx_index::relative_doc_href` for a document, and the renderer's
//! `relative_asset_href` for a `:download:` file. Neither encodes a path, so
//! neither is decoded here. A page the build writes on its own (`genindex`,
//! the domain indices) has no source, and is followed nowhere.

use lsp_types::{TextDocumentIdentifier, Uri};
use serde::{Deserialize, Serialize};

/// The request asking where a link of the preview leads.
pub const PREVIEW_LINK_METHOD: &str = "rinx/previewLink";

/// What a `rinx/previewLink` request asks for.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewLinkParams {
    /// The open document whose page holds the link.
    pub text_document: TextDocumentIdentifier,
    /// The link's `href`, as the page writes it.
    pub href: String,
}

/// What a link leads to: a document's source, or a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PreviewLinkKind {
    /// A document's source, whose page the preview can show.
    Document,
    /// A file a `:download:` names.
    File,
}

/// The answer to a `rinx/previewLink` request, when the link leads to a
/// source; `null` when it leads to none.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct PreviewLinkResult {
    pub kind: PreviewLinkKind,
    pub uri: Uri,
    /// The anchor on the document's page, when the link names one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fragment: Option<String>,
}

/// Where in the site an `href` leads, before anything says it exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageHref {
    /// The page of the document named `doc_path` (`<docname>.rst`).
    Document {
        doc_path: String,
        fragment: Option<String>,
    },
    /// A `:download:` file, at its path under the source root.
    Download { source_relative: String },
}

/// Where `href`, written on the page of the document `doc_path`, leads in
/// the site, or `None` for a link with a scheme, one climbing above the
/// site's root, or one to a file that is neither a page nor a download.
#[must_use]
pub fn resolve_page_href(doc_path: &str, href: &str) -> Option<PageHref> {
    if has_scheme(href) {
        return None;
    }
    let (address, fragment) = match href.split_once('#') {
        Some((address, fragment)) => (address, Some(fragment)),
        None => (href, None),
    };
    let address = address.split_once('?').map_or(address, |(path, _)| path);
    let fragment = fragment
        .filter(|fragment| !fragment.is_empty())
        .map(str::to_string);
    if address.is_empty() {
        return Some(PageHref::Document {
            doc_path: doc_path.to_string(),
            fragment,
        });
    }
    let path = join_normalized(doc_path, address)?;
    if let Some(source_relative) = path.strip_prefix("_downloads/") {
        return Some(PageHref::Download {
            source_relative: source_relative.to_string(),
        });
    }
    let docname = path.strip_suffix(".html")?;
    Some(PageHref::Document {
        doc_path: format!("{docname}.rst"),
        fragment,
    })
}

/// Whether `href` starts with a URI scheme (`https:`, `mailto:`), which no
/// relative link does.
fn has_scheme(href: &str) -> bool {
    let Some((scheme, _)) = href.split_once(':') else {
        return false;
    };
    let mut characters = scheme.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        && characters.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

/// `relative`, a `/`-separated path, joined to the directory of `doc_path`
/// and normalised, or `None` when it climbs above the site's root or is
/// absolute.
fn join_normalized(doc_path: &str, relative: &str) -> Option<String> {
    if relative.starts_with('/') {
        return None;
    }
    let mut segments: Vec<&str> = doc_path.split('/').collect();
    segments.pop();
    for segment in relative.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop()?;
            }
            segment => segments.push(segment),
        }
    }
    Some(segments.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document(doc_path: &str, fragment: Option<&str>) -> PageHref {
        PageHref::Document {
            doc_path: doc_path.to_string(),
            fragment: fragment.map(str::to_string),
        }
    }

    #[test]
    fn test_resolve_page_href_reads_a_sibling_page() {
        // When
        let resolved = resolve_page_href("guide/intro.rst", "setup.html");

        // Then
        assert_eq!(resolved, Some(document("guide/setup.rst", None)));
    }

    #[test]
    fn test_resolve_page_href_reads_a_page_a_directory_down() {
        // When
        let resolved = resolve_page_href("index.rst", "library/os.html");

        // Then
        assert_eq!(resolved, Some(document("library/os.rst", None)));
    }

    #[test]
    fn test_resolve_page_href_reads_a_page_a_directory_up() {
        // When
        let resolved = resolve_page_href("library/os.rst", "../index.html");

        // Then
        assert_eq!(resolved, Some(document("index.rst", None)));
    }

    #[test]
    fn test_resolve_page_href_refuses_a_link_above_the_sites_root() {
        // When
        let resolved = resolve_page_href("index.rst", "../outside.html");

        // Then
        assert_eq!(resolved, None);
    }

    #[test]
    fn test_resolve_page_href_refuses_an_absolute_path() {
        // When
        let resolved = resolve_page_href("index.rst", "/index.html");

        // Then
        assert_eq!(resolved, None);
    }

    #[test]
    fn test_resolve_page_href_keeps_the_fragment() {
        // When
        let resolved = resolve_page_href("index.rst", "library/os.html#os.getcwd");

        // Then
        assert_eq!(
            resolved,
            Some(document("library/os.rst", Some("os.getcwd")))
        );
    }

    #[test]
    fn test_resolve_page_href_drops_a_query() {
        // When
        let resolved = resolve_page_href("index.rst", "setup.html?x=1#top");

        // Then
        assert_eq!(resolved, Some(document("setup.rst", Some("top"))));
    }

    #[test]
    fn test_resolve_page_href_reads_a_fragment_alone_as_the_document_itself() {
        // When
        let resolved = resolve_page_href("guide/intro.rst", "#usage");

        // Then
        assert_eq!(resolved, Some(document("guide/intro.rst", Some("usage"))));
    }

    #[test]
    fn test_resolve_page_href_reads_an_empty_href_as_the_document_itself() {
        // When
        let resolved = resolve_page_href("guide/intro.rst", "");

        // Then
        assert_eq!(resolved, Some(document("guide/intro.rst", None)));
    }

    #[test]
    fn test_resolve_page_href_keeps_a_docname_with_a_dot() {
        // When
        let resolved = resolve_page_href("index.rst", "whatsnew/3.12.html");

        // Then
        assert_eq!(resolved, Some(document("whatsnew/3.12.rst", None)));
    }

    #[test]
    fn test_resolve_page_href_reads_a_built_index_as_a_document_name() {
        // When — whether it is one is the project's to say
        let resolved = resolve_page_href("library/os.rst", "../genindex.html");

        // Then
        assert_eq!(resolved, Some(document("genindex.rst", None)));
    }

    #[test]
    fn test_resolve_page_href_reads_a_download() {
        // When
        let resolved = resolve_page_href("guide/intro.rst", "../_downloads/data/sample.csv");

        // Then
        assert_eq!(
            resolved,
            Some(PageHref::Download {
                source_relative: "data/sample.csv".to_string()
            })
        );
    }

    #[test]
    fn test_resolve_page_href_refuses_a_built_image() {
        // When
        let resolved = resolve_page_href("index.rst", "_images/logo.png");

        // Then
        assert_eq!(resolved, None);
    }

    #[test]
    fn test_resolve_page_href_refuses_a_link_with_a_scheme() {
        // When
        let resolved: Vec<_> = ["https://docs.python.org/3/", "mailto:a@b.c", "data:x"]
            .iter()
            .map(|href| resolve_page_href("index.rst", href))
            .collect();

        // Then
        assert_eq!(resolved, vec![None, None, None]);
    }

    #[test]
    fn test_has_scheme_tells_a_scheme_from_a_path() {
        // When
        let answers: Vec<_> = ["https://x", "vscode-webview://x", "a.html", "x/y:z", ":x"]
            .iter()
            .map(|href| has_scheme(href))
            .collect();

        // Then
        assert_eq!(answers, vec![true, true, false, false, false]);
    }

    #[test]
    fn test_join_normalized_skips_empty_and_current_segments() {
        // When
        let joined = join_normalized("a/b.rst", "./c//d.html");

        // Then
        assert_eq!(joined.as_deref(), Some("a/c/d.html"));
    }
}

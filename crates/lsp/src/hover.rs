//! What hovering a cross-reference shows: where it leads.
//!
//! The answer is the renderer's own (`rinx_renderer::ReferenceResolver`), so
//! the hover names exactly what the built page links to — and a reference the
//! page would draw broken shows no hover, since its diagnostic already says
//! why. A reference leading to an entity shows the entity's fields instead
//! of its title alone (`entity_hover`).

use lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Range, Uri};
use rinx_renderer::{Destination, ReferenceTarget};

/// The hover for a reference at `range` that leads to `target`, which is
/// at `location` (see [`location_markdown`]): `entity_text` when the target
/// is an entity whose fields were found, else the target's title.
#[must_use]
pub fn reference_hover(
    target: &ReferenceTarget,
    range: Range,
    location: &str,
    entity_text: Option<String>,
) -> Hover {
    let value = entity_text
        .unwrap_or_else(|| format!("**{}**\n\n{location}", escape_markdown(&target.title)));
    markdown_hover(value, range)
}

/// A hover showing the Markdown `value` over `range`.
#[must_use]
pub fn markdown_hover(value: String, range: Range) -> Hover {
    Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value,
        }),
        range: Some(range),
    }
}

/// Where a target is, as a hover's line: a link to the document's file, the
/// address of another site's page, or the page the build writes — where
/// `file_link` gives the URI a document of the target's workspace folder is
/// opened by, given its `.rst` path.
pub fn location_markdown(
    destination: &Destination,
    file_link: impl Fn(&str) -> Option<Uri>,
) -> String {
    match destination {
        Destination::Document { doc_path, .. } => {
            let shown = escape_markdown(doc_path);
            match file_link(doc_path) {
                Some(uri) => format!("[{shown}]({})", uri.as_str()),
                None => shown,
            }
        }
        Destination::GeneratedPage { path } => {
            format!("{}, written by the build", escape_markdown(path))
        }
        Destination::External { url, site } => {
            format!("<{url}> {}", escape_markdown(site))
        }
    }
}

/// `text` with every character Markdown would read as markup escaped, so a
/// title like `__init__` or `a*b` shows as written.
pub(crate) fn escape_markdown(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        if "\\`*_{}[]()<>#+-.!|~".contains(character) {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn target(destination: Destination) -> ReferenceTarget {
        ReferenceTarget {
            title: "Installing".to_string(),
            destination,
            entity: None,
        }
    }

    /// The hover of `target`, whose file has no URI and which shows no
    /// entity's fields.
    fn plain_hover(target: &ReferenceTarget) -> Hover {
        let location = location_markdown(&target.destination, |_| None);
        reference_hover(target, Range::default(), &location, None)
    }

    fn markdown_of(hover: &Hover) -> &str {
        match &hover.contents {
            HoverContents::Markup(markup) => &markup.value,
            other => panic!("not markup: {other:?}"),
        }
    }

    #[test]
    fn test_reference_hover_links_the_file_of_a_document_target() {
        // Given
        let target = target(Destination::Document {
            doc_path: "guide/setup.rst".to_string(),
            anchor: Some("install".to_string()),
        });
        let range = Range::default();

        // When
        let location = location_markdown(&target.destination, |doc_path| {
            Uri::from_str(&format!("file:///project/{doc_path}")).ok()
        });
        let hover = reference_hover(&target, range, &location, None);

        // Then
        assert_eq!(
            markdown_of(&hover),
            "**Installing**\n\n[guide/setup\\.rst](file:///project/guide/setup.rst)"
        );
        assert_eq!(hover.range, Some(range));
    }

    #[test]
    fn test_reference_hover_names_the_file_when_it_has_no_uri() {
        // Given
        let target = target(Destination::Document {
            doc_path: "setup.rst".to_string(),
            anchor: None,
        });

        // When
        let hover = plain_hover(&target);

        // Then
        assert_eq!(markdown_of(&hover), "**Installing**\n\nsetup\\.rst");
    }

    #[test]
    fn test_reference_hover_names_a_page_the_build_writes() {
        // Given
        let target = target(Destination::GeneratedPage {
            path: "genindex.html".to_string(),
        });

        // When
        let hover = plain_hover(&target);

        // Then
        assert_eq!(
            markdown_of(&hover),
            "**Installing**\n\ngenindex\\.html, written by the build"
        );
    }

    #[test]
    fn test_reference_hover_gives_another_sites_address_and_name() {
        // Given
        let target = target(Destination::External {
            url: "https://docs.python.org/3/library/stdtypes.html#dict".to_string(),
            site: "(in Python v3.12)".to_string(),
        });

        // When
        let hover = plain_hover(&target);

        // Then
        assert_eq!(
            markdown_of(&hover),
            "**Installing**\n\n<https://docs.python.org/3/library/stdtypes.html#dict> \
             \\(in Python v3\\.12\\)"
        );
    }

    #[test]
    fn test_reference_hover_shows_an_entity_s_text_in_place_of_the_title() {
        // Given
        let target = target(Destination::Document {
            doc_path: "reqs.rst".to_string(),
            anchor: None,
        });

        // When
        let hover = reference_hover(
            &target,
            Range::default(),
            "reqs\\.rst",
            Some("**Requirement** REQ\\_1".to_string()),
        );

        // Then
        assert_eq!(markdown_of(&hover), "**Requirement** REQ\\_1");
    }

    #[test]
    fn test_escape_markdown_escapes_what_markdown_reads_as_markup() {
        // Given / When / Then
        assert_eq!(
            escape_markdown("__init__ *a* [b]"),
            "\\_\\_init\\_\\_ \\*a\\* \\[b\\]"
        );
        assert_eq!(escape_markdown("plain words"), "plain words");
    }
}

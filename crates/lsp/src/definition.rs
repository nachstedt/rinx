//! Where going to a cross-reference's definition leads: the line its target
//! was written on, or the file of the document it names.
//!
//! The target is the renderer's own (`rinx_renderer::ReferenceResolver`), as
//! for a hover, so the editor opens exactly what the built page links to — and
//! a reference the page would draw broken leads nowhere. The target names the
//! definition it is by its index key, and the line comes from where the
//! defining document's analysis recorded it (ADR-045); a document itself, or
//! a definition with no recorded site, leads to the start of its file.

use std::str::FromStr;

use lsp_types::{GotoDefinitionResponse, Location, LocationLink, Range, Uri};
use rinx_ast::{Position, Span};
use rinx_index::DefinitionKey;
use rinx_renderer::{Destination, ReferenceTarget};

use crate::position::{PositionEncoding, to_lsp_range};

/// Where `target` was written: the location `site` gives for its definition,
/// given the defining document's `.rst` path, else the start of that
/// document's file, whose URI `file_link` gives.
///
/// `None` for a target that is no document of this site: another site's page
/// or a page the build writes has no source to open.
#[must_use]
pub fn target_location(
    target: &ReferenceTarget,
    file_link: impl Fn(&str) -> Option<Uri>,
    site: impl Fn(&str, &DefinitionKey) -> Option<Location>,
) -> Option<Location> {
    let Destination::Document { doc_path, .. } = &target.destination else {
        return None;
    };
    if let Some(location) = target
        .definition
        .as_ref()
        .and_then(|key| site(doc_path, key))
    {
        return Some(location);
    }
    Some(Location::new(file_link(doc_path)?, Range::default()))
}

/// The location of a definition written from `start` to just before `end`
/// in the file at `uri`, whose text is `text` — or, when the text could not
/// be read, the start of the definition's line, since its columns cannot be
/// counted in the client's encoding.
#[must_use]
pub fn site_location(
    uri: Uri,
    start: Position,
    end: Position,
    text: Option<&str>,
    encoding: PositionEncoding,
) -> Location {
    let range = text.map_or_else(
        || {
            let line_start = lsp_types::Position::new(start.line.saturating_sub(1), 0);
            Range::new(line_start, line_start)
        },
        |text| to_lsp_range(Span::new(start, end), text, encoding),
    );
    Location::new(uri, range)
}

/// `location`'s URI with its line as the `#L<line>` fragment a Markdown link
/// opens it at — or the bare URI for the start of a file, which needs none.
#[must_use]
pub fn line_link(location: &Location) -> Uri {
    if location.range == Range::default() {
        return location.uri.clone();
    }
    let line = location.range.start.line + 1;
    Uri::from_str(&format!("{}#L{line}", location.uri.as_str()))
        .unwrap_or_else(|_| location.uri.clone())
}

/// The definition of a reference at `origin` that leads to `location` — as a
/// link carrying `origin` when the client takes links, so the whole reference
/// is underlined, and as a plain location otherwise.
#[must_use]
pub fn reference_definition(
    location: Location,
    origin: Range,
    links: bool,
) -> GotoDefinitionResponse {
    if links {
        GotoDefinitionResponse::Link(vec![LocationLink {
            origin_selection_range: Some(origin),
            target_uri: location.uri,
            target_range: location.range,
            target_selection_range: location.range,
        }])
    } else {
        GotoDefinitionResponse::Scalar(location)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::TargetName;

    fn target(destination: Destination) -> ReferenceTarget {
        ReferenceTarget {
            title: "Installing".to_string(),
            destination,
            entity: None,
            definition: None,
        }
    }

    fn install() -> DefinitionKey {
        DefinitionKey::Label(TargetName::new("install"))
    }

    fn document_target() -> ReferenceTarget {
        ReferenceTarget {
            definition: Some(install()),
            ..target(Destination::Document {
                doc_path: "guide/setup.rst".to_string(),
                anchor: Some("install".to_string()),
            })
        }
    }

    fn project_uri(doc_path: &str) -> Option<Uri> {
        Uri::from_str(&format!("file:///project/{doc_path}")).ok()
    }

    fn label_line() -> Range {
        Range::new(
            lsp_types::Position::new(4, 0),
            lsp_types::Position::new(4, 12),
        )
    }

    /// The site of `install` in `guide/setup.rst`, and of nothing else.
    fn install_site(doc_path: &str, key: &DefinitionKey) -> Option<Location> {
        (doc_path == "guide/setup.rst" && *key == install())
            .then(|| Location::new(project_uri(doc_path).unwrap(), label_line()))
    }

    fn origin() -> Range {
        Range::new(
            lsp_types::Position::new(0, 4),
            lsp_types::Position::new(0, 18),
        )
    }

    #[test]
    fn test_target_location_is_where_the_definition_was_written() {
        // When
        let location = target_location(&document_target(), project_uri, install_site);

        // Then
        assert_eq!(
            location,
            Some(Location::new(
                project_uri("guide/setup.rst").unwrap(),
                label_line()
            ))
        );
    }

    #[test]
    fn test_target_location_is_the_file_start_for_a_document_itself() {
        // Given — a `:doc:` names no definition
        let document = ReferenceTarget {
            definition: None,
            ..document_target()
        };

        // When
        let location = target_location(&document, project_uri, install_site);

        // Then
        assert_eq!(
            location,
            Some(Location::new(
                project_uri("guide/setup.rst").unwrap(),
                Range::default()
            ))
        );
    }

    #[test]
    fn test_target_location_is_the_file_start_for_a_definition_with_no_site() {
        // When
        let location = target_location(&document_target(), project_uri, |_, _| None);

        // Then
        assert_eq!(
            location.map(|location| location.range),
            Some(Range::default())
        );
    }

    #[test]
    fn test_target_location_is_none_without_a_uri_for_the_file() {
        // When
        let location = target_location(&document_target(), |_| None, |_, _| None);

        // Then
        assert_eq!(location, None);
    }

    #[test]
    fn test_target_location_is_none_for_a_target_that_is_no_document() {
        // Given
        let external = target(Destination::External {
            url: "https://docs.python.org/3/library/stdtypes.html#dict".to_string(),
            site: "(in Python v3.12)".to_string(),
        });
        let generated = target(Destination::GeneratedPage {
            path: "genindex.html".to_string(),
        });

        // When / Then
        assert_eq!(target_location(&external, project_uri, install_site), None);
        assert_eq!(target_location(&generated, project_uri, install_site), None);
    }

    #[test]
    fn test_site_location_counts_columns_in_the_client_encoding() {
        // Given — a character outside the BMP before the label's end
        let text = "Intro\n\n.. _\u{1F600}:\n";

        // When
        let location = site_location(
            project_uri("a.rst").unwrap(),
            Position::new(3, 1),
            Position::new(3, 7),
            Some(text),
            PositionEncoding::Utf16,
        );

        // Then — the emoji is two UTF-16 units
        assert_eq!(
            location.range,
            Range::new(
                lsp_types::Position::new(2, 0),
                lsp_types::Position::new(2, 7)
            )
        );
    }

    #[test]
    fn test_site_location_is_the_line_start_when_the_text_is_unread() {
        // When
        let location = site_location(
            project_uri("a.rst").unwrap(),
            Position::new(3, 4),
            Position::new(3, 9),
            None,
            PositionEncoding::Utf16,
        );

        // Then
        let line_start = lsp_types::Position::new(2, 0);
        assert_eq!(location.range, Range::new(line_start, line_start));
    }

    #[test]
    fn test_line_link_names_the_line_of_a_definition() {
        // Given
        let location = Location::new(project_uri("guide/setup.rst").unwrap(), label_line());

        // When / Then
        assert_eq!(
            line_link(&location).as_str(),
            "file:///project/guide/setup.rst#L5"
        );
    }

    #[test]
    fn test_line_link_is_the_bare_uri_for_the_start_of_a_file() {
        // Given
        let location = Location::new(project_uri("setup.rst").unwrap(), Range::default());

        // When / Then
        assert_eq!(line_link(&location).as_str(), "file:///project/setup.rst");
    }

    #[test]
    fn test_reference_definition_is_the_location_for_a_client_taking_no_links() {
        // Given
        let location = Location::new(project_uri("guide/setup.rst").unwrap(), label_line());

        // When
        let definition = reference_definition(location.clone(), origin(), false);

        // Then
        assert_eq!(definition, GotoDefinitionResponse::Scalar(location));
    }

    #[test]
    fn test_reference_definition_links_from_the_whole_reference_to_the_definition() {
        // Given
        let location = Location::new(project_uri("guide/setup.rst").unwrap(), label_line());

        // When
        let definition = reference_definition(location, origin(), true);

        // Then
        assert_eq!(
            definition,
            GotoDefinitionResponse::Link(vec![LocationLink {
                origin_selection_range: Some(origin()),
                target_uri: project_uri("guide/setup.rst").unwrap(),
                target_range: label_line(),
                target_selection_range: label_line(),
            }])
        );
    }
}

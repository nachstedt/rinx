//! Where going to a cross-reference's definition leads: the file of the
//! document it names.
//!
//! The target is the renderer's own (`rinx_renderer::ReferenceResolver`), as
//! for a hover, so the editor opens exactly the document the built page links
//! to — and a reference the page would draw broken leads nowhere. Only the
//! file is known yet: the line a label or object is written on comes with
//! definition spans (roadmap #28, #29), so the definition is the file's start.

use lsp_types::{GotoDefinitionResponse, Location, LocationLink, Range, Uri};
use rinx_renderer::{Destination, ReferenceTarget};

/// The definition of a reference at `origin` that leads to `target`, where
/// `file_link` gives the URI a document of the target's workspace folder is
/// opened by, given its `.rst` path — as a link carrying `origin` when the
/// client takes links, so the whole reference is underlined, and as a plain
/// location otherwise.
///
/// `None` for a target that is no document of this site: another site's page
/// or a page the build writes has no source to open.
#[must_use]
pub fn reference_definition(
    target: &ReferenceTarget,
    origin: Range,
    file_link: impl Fn(&str) -> Option<Uri>,
    links: bool,
) -> Option<GotoDefinitionResponse> {
    let Destination::Document { doc_path, .. } = &target.destination else {
        return None;
    };
    let uri = file_link(doc_path)?;
    let start = Range::default();
    Some(if links {
        GotoDefinitionResponse::Link(vec![LocationLink {
            origin_selection_range: Some(origin),
            target_uri: uri,
            target_range: start,
            target_selection_range: start,
        }])
    } else {
        GotoDefinitionResponse::Scalar(Location::new(uri, start))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::Position;
    use std::str::FromStr;

    fn target(destination: Destination) -> ReferenceTarget {
        ReferenceTarget {
            title: "Installing".to_string(),
            destination,
        }
    }

    fn document_target() -> ReferenceTarget {
        target(Destination::Document {
            doc_path: "guide/setup.rst".to_string(),
            anchor: Some("install".to_string()),
        })
    }

    fn project_uri(doc_path: &str) -> Option<Uri> {
        Uri::from_str(&format!("file:///project/{doc_path}")).ok()
    }

    fn origin() -> Range {
        Range::new(Position::new(0, 4), Position::new(0, 18))
    }

    #[test]
    fn test_reference_definition_is_the_start_of_the_target_documents_file() {
        // When
        let definition = reference_definition(&document_target(), origin(), project_uri, false);

        // Then
        assert_eq!(
            definition,
            Some(GotoDefinitionResponse::Scalar(Location::new(
                project_uri("guide/setup.rst").unwrap(),
                Range::default()
            )))
        );
    }

    #[test]
    fn test_reference_definition_links_from_the_whole_reference_when_links_are_taken() {
        // When
        let definition = reference_definition(&document_target(), origin(), project_uri, true);

        // Then
        assert_eq!(
            definition,
            Some(GotoDefinitionResponse::Link(vec![LocationLink {
                origin_selection_range: Some(origin()),
                target_uri: project_uri("guide/setup.rst").unwrap(),
                target_range: Range::default(),
                target_selection_range: Range::default(),
            }]))
        );
    }

    #[test]
    fn test_reference_definition_is_none_without_a_uri_for_the_file() {
        // When
        let definition = reference_definition(&document_target(), origin(), |_| None, false);

        // Then
        assert_eq!(definition, None);
    }

    #[test]
    fn test_reference_definition_is_none_for_a_target_that_is_no_document() {
        // Given
        let external = target(Destination::External {
            url: "https://docs.python.org/3/library/stdtypes.html#dict".to_string(),
            site: "(in Python v3.12)".to_string(),
        });
        let generated = target(Destination::GeneratedPage {
            path: "genindex.html".to_string(),
        });

        // When / Then
        assert_eq!(
            reference_definition(&external, origin(), project_uri, true),
            None
        );
        assert_eq!(
            reference_definition(&generated, origin(), project_uri, true),
            None
        );
    }
}

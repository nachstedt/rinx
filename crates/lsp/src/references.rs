//! What "find all references" answers: the locations of a definition's uses,
//! as the project resolved them (`project/references.rs`), in the client's
//! position encoding.

use lsp_types::{Location, Range, Uri};
use rinx_ast::Span;

use crate::position::{PositionEncoding, to_lsp_range};
use crate::project::{UseForm, UseSite, relation_id_columns};

/// The location of `site` in the file at `uri`, whose text is `text`: the use
/// itself, or the id alone on a relation option's line — or, when the text
/// could not be read, the start of its line, since its columns cannot be
/// counted in the client's encoding.
#[must_use]
pub fn use_location(
    uri: Uri,
    site: &UseSite,
    text: Option<&str>,
    encoding: PositionEncoding,
) -> Location {
    let line_start = || {
        let start = lsp_types::Position::new(site.start.line.saturating_sub(1), 0);
        Range::new(start, start)
    };
    let range = text.map_or_else(line_start, |text| {
        let UseForm::RelationId(id) = &site.form else {
            return to_lsp_range(Span::new(site.start, site.end), text, encoding);
        };
        let Some(line) = text.lines().nth(site.start.line.saturating_sub(1) as usize) else {
            return line_start();
        };
        relation_id_columns(line, id).map_or_else(line_start, |columns| {
            let at = |index: usize| {
                let column = u32::try_from(index + 1).unwrap_or(u32::MAX);
                rinx_ast::Position::new(site.start.line, column)
            };
            to_lsp_range(
                Span::new(at(columns.start), at(columns.end)),
                text,
                encoding,
            )
        })
    });
    Location::new(uri, range)
}

/// `locations` in the order an editor lists them — by file, then by place in
/// it — each once: a fragment two documents include holds the same use for
/// both.
#[must_use]
pub fn ordered_locations(mut locations: Vec<Location>) -> Vec<Location> {
    let key = |location: &Location| {
        let Range { start, end } = location.range;
        (
            location.uri.as_str().to_string(),
            (start.line, start.character),
            (end.line, end.character),
        )
    };
    locations.sort_by_key(key);
    locations.dedup();
    locations
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::{EntityId, Position};
    use std::path::PathBuf;
    use std::str::FromStr;

    fn uri(name: &str) -> Uri {
        Uri::from_str(&format!("file:///project/{name}")).unwrap()
    }

    fn site(line: u32, start: u32, end: u32) -> UseSite {
        UseSite {
            path: Some(PathBuf::from("/project/a.rst")),
            start: Position::new(line, start),
            end: Position::new(line, end),
            form: UseForm::Role,
        }
    }

    fn range(line: u32, start: u32, end: u32) -> Range {
        Range::new(
            lsp_types::Position::new(line, start),
            lsp_types::Position::new(line, end),
        )
    }

    #[test]
    fn test_use_location_is_the_use_in_the_client_encoding() {
        // Given — a character outside the BMP before the reference
        let text = "\u{1F600} :ref:`install`\n";

        // When
        let location = use_location(
            uri("a.rst"),
            &site(1, 3, 17),
            Some(text),
            PositionEncoding::Utf16,
        );

        // Then — the emoji is two UTF-16 units
        assert_eq!(location.range, range(0, 3, 17));
    }

    #[test]
    fn test_use_location_is_the_id_alone_on_a_relation_line() {
        // Given
        let text = ".. req:: Two\n   :links: REQ_0, REQ_1\n";
        let relation = UseSite {
            form: UseForm::RelationId(EntityId::new("REQ_1").unwrap()),
            ..site(2, 4, 24)
        };

        // When
        let location = use_location(uri("a.rst"), &relation, Some(text), PositionEncoding::Utf16);

        // Then
        assert_eq!(location.range, range(1, 18, 23));
    }

    #[test]
    fn test_use_location_is_the_line_start_when_the_text_is_unread() {
        // When
        let location = use_location(uri("a.rst"), &site(3, 4, 9), None, PositionEncoding::Utf16);

        // Then
        assert_eq!(location.range, range(2, 0, 0));
    }

    #[test]
    fn test_ordered_locations_sorts_by_file_and_place_and_drops_repeats() {
        // Given
        let late = Location::new(uri("a.rst"), range(4, 0, 3));
        let early = Location::new(uri("a.rst"), range(1, 2, 5));
        let other = Location::new(uri("b.rst"), range(0, 0, 1));

        // When
        let ordered = ordered_locations(vec![
            other.clone(),
            late.clone(),
            early.clone(),
            late.clone(),
        ]);

        // Then
        assert_eq!(ordered, vec![early, late, other]);
    }
}

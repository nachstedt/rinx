//! Finding the entity a directive marker declares.
//!
//! What hovering an entity's own `.. req::` shows is read from the parse
//! rather than from the text, so an id the schema composes or generates —
//! one no `:id:` line spells — is found too, and so is an entity only an
//! unsaved buffer holds.

use rinx_ast::{Directive, Document, EntityBody, Node, walk_nodes};

/// The entity whose directive marker is on line `line` (1-based) of
/// `document`'s own text, or `None` when no entity's marker is there.
///
/// An entity in an `.. include::`d fragment counts lines in that file, not
/// in this one, so it is not found.
#[must_use]
pub fn entity_marked_at(document: &Document, line: u32) -> Option<&EntityBody> {
    let mut found = None;
    walk_nodes(&document.nodes, &mut |node| {
        if let Node::Directive(Directive::Entity(entity)) = node
            && entity
                .span
                .is_some_and(|span| span.file.is_none() && span.start.line == line)
        {
            found = Some(&**entity);
        }
    });
    found
}

#[cfg(test)]
mod tests {
    use rinx_ast::Domain;
    use rinx_entity::{EntitySchema, NoReservedNames, load_schema};
    use rinx_parser::{ParseCtx, RejectParseFiles, parse_with_ctx};

    use super::*;

    fn schema() -> EntitySchema {
        load_schema(
            "[[entity_type]]\nname = \"req\"\n\n[[entity_type]]\nname = \"spec\"\n",
            &NoReservedNames,
        )
        .unwrap()
    }

    /// The id of the entity marked on `line` of `text`.
    fn id_marked_at(text: &str, line: u32) -> Option<String> {
        let schema = schema();
        let ctx = ParseCtx::new(Domain::Py, &RejectParseFiles).with_schema(&schema);
        let document = parse_with_ctx("index.rst", text, &ctx);
        entity_marked_at(&document, line).map(|entity| entity.id.to_string())
    }

    #[test]
    fn test_entity_marked_at_finds_the_entity_whose_marker_is_on_the_line() {
        // Given
        let text = "Intro.\n\n.. req:: Brakes\n   :id: R_1\n\n.. spec:: Pads\n   :id: S_1\n";

        // When / Then
        assert_eq!(id_marked_at(text, 3), Some("R_1".to_string()));
        assert_eq!(id_marked_at(text, 6), Some("S_1".to_string()));
    }

    #[test]
    fn test_entity_marked_at_finds_none_off_a_marker() {
        // Given
        let text = "Intro.\n\n.. req:: Brakes\n   :id: R_1\n";

        // When / Then
        assert_eq!(id_marked_at(text, 1), None);
        assert_eq!(id_marked_at(text, 4), None);
    }
}

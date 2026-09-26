//! One entity, as `PlantUML` draws it.
//!
//! Shared by the template function `flow(id)` and by the generated flowchart,
//! which is the whole reason it is a module rather than a helper inside either:
//! an entity should look the same and land on the same anchor whether an author
//! drew it by hand or asked `.. entity-flow::` to.

use rinx_ast::HashedContent;

use crate::snapshot::Snapshot;

/// How many hex characters of an id's hash disambiguate a sanitized alias.
///
/// Eight is far beyond what a document's worth of entities needs, and the
/// alias is never shown — it appears only in the generated `PlantUML`, where
/// its whole job is to be a name `PlantUML` will accept.
const ALIAS_HASH_LENGTH: usize = 8;

/// The `PlantUML` node text for one entity.
///
/// A `rectangle` with the entity's id as its alias, so a template — or a
/// generated flowchart — can draw edges between nodes by id. `flow('A')` and
/// `flow('B')` followed by `A --> B` is the shape every sphinx-needs diagram is
/// written in.
pub(crate) fn node_for(snapshot: &Snapshot, id: &str) -> String {
    let label = quote_safe(snapshot.title(id));
    let type_name = quote_safe(snapshot.type_name(id));
    let link = snapshot
        .href(id)
        .map_or_else(String::new, |href| format!(" [[{href}]]"));
    format!(
        "rectangle \"{label}\\n<size:10>{type_name}: {id}</size>\" as {}{link}",
        alias(id)
    )
}

/// An entity's id as a `PlantUML` alias.
///
/// An id may carry `-`, `.` and `:` besides the letters, digits and
/// underscores `PlantUML` accepts, and none of those three survives: an
/// unquoted `REQ-1` in an arrow is a syntax error, and `as "REQ-1"` is not
/// alias syntax at all — `PlantUML` reads the quotes as part of the *label*,
/// which is how this surfaced, as a node captioned `… as "REQ-1"`.
///
/// So an id that is not already a name is rewritten into one, with a short
/// hash of the original appended: sanitizing alone would map `REQ-1` and
/// `REQ_1` onto one alias, silently drawing two entities as a single node. An
/// id that needs no rewriting keeps its own spelling, so the common case reads
/// as the author wrote it and no existing diagram's bytes move.
///
/// Every edge must spell an alias exactly as its node did, which is why both
/// go through here.
pub(crate) fn alias(id: &str) -> String {
    if is_plain_name(id) {
        return id.to_string();
    }
    let sanitized: String = id
        .chars()
        .map(|c| if is_name_char(c) { c } else { '_' })
        .collect();
    format!("{sanitized}_{}", &id_hash(id)[..ALIAS_HASH_LENGTH])
}

/// Whether `id` is already a name `PlantUML` accepts bare.
fn is_plain_name(id: &str) -> bool {
    !id.is_empty() && id.chars().all(is_name_char)
}

/// Whether `c` may appear in a bare `PlantUML` alias.
fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// A stable hex digest of `id`.
///
/// Through [`HashedContent`] rather than a hashing crate of this crate's own:
/// it is the project's one content hash, it is already a dependency here, and
/// an alias must be stable across builds for exactly the reason a diagram's
/// own hash must be — the bytes it appears in name the compiled picture.
fn id_hash(id: &str) -> String {
    HashedContent::new(id.to_string()).hash().to_string()
}

/// Text that cannot end a `PlantUML` quoted string early.
///
/// A title is prose an author wrote, so it may hold anything; a stray `"` would
/// not corrupt the page but the diagram, where the failure surfaces as
/// `PlantUML`'s own syntax error about a line nobody wrote.
pub(crate) fn quote_safe(text: &str) -> String {
    text.replace('"', "'").replace('\n', " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_a_plain_identifier_is_its_own_alias() {
        // Given
        let id = "REQ_001";

        // When
        let alias = alias(id);

        // Then
        assert_eq!(alias, "REQ_001");
    }

    #[test]
    fn test_an_id_plantuml_cannot_read_bare_is_rewritten_into_a_name() {
        // Given — an entity id may carry `-`, `.` and `:` too, and none of the
        // three is legal in an alias
        let ids = ["REQ-1", "spec.boot", "ns:req"];

        // When
        let aliases: Vec<String> = ids.iter().map(|id| alias(id)).collect();

        // Then
        for (id, alias) in ids.iter().zip(&aliases) {
            assert!(
                alias.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
                "{id} produced {alias}"
            );
        }
    }

    #[test]
    fn test_two_ids_that_sanitize_alike_still_get_different_aliases() {
        // Given — without the hash these would be one node, silently drawn as
        // a single entity
        let first = alias("REQ-1");
        let second = alias("REQ_1");

        // When / Then
        assert_ne!(first, second);
        assert_eq!(second, "REQ_1", "a plain name keeps its own spelling");
    }

    #[test]
    fn test_an_alias_is_the_same_on_every_build() {
        // Given — the alias lands in bytes that are hashed into the compiled
        // picture's filename
        let id = "os.system";

        // When / Then
        assert_eq!(alias(id), alias(id));
    }

    #[test]
    fn test_a_quote_in_text_cannot_end_the_string_it_sits_in() {
        // Given
        let text = "The \"login\" screen\nand its form";

        // When
        let safe = quote_safe(text);

        // Then
        assert_eq!(safe, "The 'login' screen and its form");
    }
}

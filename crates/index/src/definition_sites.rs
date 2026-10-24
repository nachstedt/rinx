//! Where each definition of one document was written: the line a label, a
//! section, a term, a domain object, an equation or an entity stands on.
//!
//! Kept apart from [`crate::ProjectIndex`] on purpose. Every render action
//! of a Bazel build takes the index as an input, so a position in it would
//! re-render the whole site whenever a line moved anywhere; these are what
//! only an editor asks, so they travel beside a document's index fragment in
//! memory and are never written to the `.index` (see
//! `docs/decisions/045-definition-spans.md`).

use std::collections::BTreeMap;

use rinx_ast::{EntityId, ObjectType, Position, SectionId, TargetName};

/// What a definition is, as the index names it: the key a lookup by the
/// index's own maps arrives with.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum DefinitionKey {
    /// An internal target or a directive's `:name:` — a key of
    /// [`crate::ProjectIndex::targets`].
    Label(TargetName),
    /// A glossary term — a key of [`crate::ProjectIndex::glossary_terms`].
    Term(TargetName),
    /// A domain object of one type — a key of
    /// [`crate::ProjectIndex::domain_objects`] and one of its types.
    Object(ObjectType, TargetName),
    /// A `.. math::` `:label:` — a key of [`crate::ProjectIndex::equations`].
    Equation(TargetName),
    /// An entity — a key of [`crate::ProjectIndex::entities`].
    Entity(EntityId),
    /// A section of the document, by the id its anchor is.
    Section(SectionId),
}

/// Where one definition was written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionSite {
    /// The file it was written in, by its source-root path, when that is an
    /// `.. include::`d fragment rather than the document itself.
    pub file: Option<String>,
    /// Its first character, in that file.
    pub start: Position,
    /// Just past its last character, in that file.
    pub end: Position,
}

/// Every definition one document writes, by what the index names it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DefinitionSites(BTreeMap<DefinitionKey, DefinitionSite>);

impl DefinitionSites {
    /// Records `site` for `key`. A later definition under the same key wins,
    /// as a later insertion into the index's own maps does.
    pub fn insert(&mut self, key: DefinitionKey, site: DefinitionSite) {
        self.0.insert(key, site);
    }

    /// Where `key` was written, if this document defines it.
    #[must_use]
    pub fn get(&self, key: &DefinitionKey) -> Option<&DefinitionSite> {
        self.0.get(key)
    }

    /// Every definition, in key order.
    pub fn iter(&self) -> impl Iterator<Item = (&DefinitionKey, &DefinitionSite)> {
        self.0.iter()
    }

    /// Whether the document defines nothing with a known position.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn site(line: u32) -> DefinitionSite {
        DefinitionSite {
            file: None,
            start: Position::new(line, 1),
            end: Position::new(line, 10),
        }
    }

    #[test]
    fn test_get_finds_a_recorded_site_by_its_key() {
        // Given
        let mut sites = DefinitionSites::default();
        sites.insert(DefinitionKey::Label(TargetName::new("install")), site(3));

        // When / Then
        assert_eq!(
            sites.get(&DefinitionKey::Label(TargetName::new("install"))),
            Some(&site(3))
        );
        assert_eq!(
            sites.get(&DefinitionKey::Term(TargetName::new("install"))),
            None
        );
    }

    #[test]
    fn test_insert_keeps_the_later_site_of_a_key_defined_twice() {
        // Given
        let mut sites = DefinitionSites::default();
        let key = DefinitionKey::Label(TargetName::new("install"));

        // When
        sites.insert(key.clone(), site(3));
        sites.insert(key.clone(), site(8));

        // Then
        assert_eq!(sites.get(&key), Some(&site(8)));
        assert_eq!(sites.iter().count(), 1);
    }

    #[test]
    fn test_is_empty_until_a_site_is_recorded() {
        // Given
        let mut sites = DefinitionSites::default();
        assert!(sites.is_empty());

        // When
        sites.insert(
            DefinitionKey::Section(SectionId::from_title("Usage")),
            site(1),
        );

        // Then
        assert!(!sites.is_empty());
    }
}

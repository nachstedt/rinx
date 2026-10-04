use rinx_ast::{EntityId, ObjectType, TargetName};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Every definition that two or more documents claim, with the documents
/// claiming it — and which therefore defines nothing.
///
/// A name defined twice is not resolved to either definition: whichever won
/// would depend on the order the documents were merged in, which is the order
/// of a build's inputs or of an editor's edits. So a contested name is taken
/// out of its family's map in [`crate::ProjectIndex`] and kept here instead,
/// where the analyzer finds it to report every claimant, and the renderer to
/// say why a reference to it links nowhere.
///
/// One map per family, since each family is its own namespace: a label and a
/// glossary term may share a name.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AmbiguousDefinitions {
    /// Labels, `:name:`s and entity ids — the `:ref:` namespace.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub targets: BTreeMap<TargetName, BTreeSet<String>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub glossary_terms: BTreeMap<TargetName, BTreeSet<String>>,
    /// Domain objects, by qualified name and then object type: a function and
    /// a class of one name are two definitions, not a contest.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub domain_objects: BTreeMap<TargetName, BTreeMap<ObjectType, BTreeSet<String>>>,
    /// `.. math::` labels.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub equations: BTreeMap<TargetName, BTreeSet<String>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub entities: BTreeMap<EntityId, BTreeSet<String>>,
}

impl AmbiguousDefinitions {
    /// Whether no definition is contested — every project without a clash,
    /// whose serialized index then leaves this out entirely.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.targets.is_empty()
            && self.glossary_terms.is_empty()
            && self.domain_objects.is_empty()
            && self.equations.is_empty()
            && self.entities.is_empty()
    }

    /// The documents claiming the domain object `name` of `object_type`, if
    /// more than one does.
    #[must_use]
    pub fn domain_object(
        &self,
        name: &TargetName,
        object_type: ObjectType,
    ) -> Option<&BTreeSet<String>> {
        self.domain_objects.get(name)?.get(&object_type)
    }
}

/// Merges one family of definitions: `incoming` and its own contested keys
/// into `defined` and `contested`, where `document_of` says which document a
/// definition belongs to.
///
/// The rule is a union of the documents claiming each key, which is why the
/// result does not depend on the order documents are merged in: a key claimed
/// by one document is defined; a key claimed by several is contested and
/// defined by none. A definition from the document that already holds the key
/// replaces it — that is a fresh analysis of the same document, as the live
/// preview merges into a stale index, not a second claimant.
pub(crate) fn merge_claims<K: Ord + Clone, V>(
    defined: &mut BTreeMap<K, V>,
    contested: &mut BTreeMap<K, BTreeSet<String>>,
    incoming: BTreeMap<K, V>,
    incoming_contested: BTreeMap<K, BTreeSet<String>>,
    document_of: impl Fn(&V) -> &str,
) {
    for (key, value) in incoming {
        let document = document_of(&value).to_string();
        if let Some(claimants) = contested.get_mut(&key) {
            claimants.insert(document);
            continue;
        }
        match defined.get(&key) {
            Some(existing) if document_of(existing) != document => {
                let first = document_of(existing).to_string();
                defined.remove(&key);
                contested.insert(key, BTreeSet::from([first, document]));
            }
            _ => {
                defined.insert(key, value);
            }
        }
    }
    for (key, claimants) in incoming_contested {
        let entry = contested.entry(key.clone()).or_default();
        if let Some(existing) = defined.remove(&key) {
            entry.insert(document_of(&existing).to_string());
        }
        entry.extend(claimants);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type Defined = BTreeMap<&'static str, String>;
    type Contested = BTreeMap<&'static str, BTreeSet<String>>;

    /// The definitions of one document: each key, defined by `document`.
    fn document(document: &str, keys: &[&'static str]) -> Defined {
        keys.iter()
            .map(|key| (*key, document.to_string()))
            .collect()
    }

    /// `parts` merged into an empty family, in order.
    fn merge_all(parts: Vec<(Defined, Contested)>) -> (Defined, Contested) {
        let mut defined = Defined::new();
        let mut contested = Contested::new();
        for (incoming, incoming_contested) in parts {
            merge_claims(
                &mut defined,
                &mut contested,
                incoming,
                incoming_contested,
                String::as_str,
            );
        }
        (defined, contested)
    }

    fn claimants(documents: &[&str]) -> BTreeSet<String> {
        documents.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn test_merge_claims_defines_a_key_one_document_claims() {
        // When
        let (defined, contested) = merge_all(vec![
            (document("a.rst", &["intro"]), Contested::new()),
            (document("b.rst", &["usage"]), Contested::new()),
        ]);

        // Then
        assert_eq!(defined.len(), 2);
        assert!(contested.is_empty());
    }

    #[test]
    fn test_merge_claims_contests_a_key_two_documents_claim_in_either_order() {
        // Given
        let forwards = vec![
            (document("a.rst", &["intro"]), Contested::new()),
            (document("b.rst", &["intro"]), Contested::new()),
        ];
        let backwards = vec![
            (document("b.rst", &["intro"]), Contested::new()),
            (document("a.rst", &["intro"]), Contested::new()),
        ];

        // When / Then — neither wins, and both are named either way.
        for parts in [forwards, backwards] {
            let (defined, contested) = merge_all(parts);
            assert!(defined.is_empty());
            assert_eq!(contested["intro"], claimants(&["a.rst", "b.rst"]));
        }
    }

    #[test]
    fn test_merge_claims_adds_a_third_claimant() {
        // When
        let (defined, contested) = merge_all(vec![
            (document("a.rst", &["intro"]), Contested::new()),
            (document("b.rst", &["intro"]), Contested::new()),
            (document("c.rst", &["intro"]), Contested::new()),
        ]);

        // Then
        assert!(defined.is_empty());
        assert_eq!(contested["intro"], claimants(&["a.rst", "b.rst", "c.rst"]));
    }

    #[test]
    fn test_merge_claims_lets_a_document_replace_its_own_definition() {
        // Given — the live preview merging a fresh analysis of `a.rst` into
        // an index that already holds the old one.
        let mut stale = document("a.rst", &["intro"]);
        stale.insert("intro", "a.rst".to_string());

        // When
        let (defined, contested) = merge_all(vec![
            (stale, Contested::new()),
            (document("a.rst", &["intro"]), Contested::new()),
        ]);

        // Then
        assert_eq!(defined["intro"], "a.rst");
        assert!(contested.is_empty());
    }

    #[test]
    fn test_merge_claims_unions_two_indexes_contested_keys() {
        // Given — two partial indexes, each already contesting `intro`, and
        // the first also defining `usage` that the second contests.
        let first = (
            document("a.rst", &["usage"]),
            Contested::from([("intro", claimants(&["a.rst", "b.rst"]))]),
        );
        let second = (
            Defined::new(),
            Contested::from([
                ("intro", claimants(&["c.rst", "d.rst"])),
                ("usage", claimants(&["e.rst", "f.rst"])),
            ]),
        );

        // When
        let (defined, contested) = merge_all(vec![first, second]);

        // Then
        assert!(defined.is_empty());
        assert_eq!(
            contested["intro"],
            claimants(&["a.rst", "b.rst", "c.rst", "d.rst"])
        );
        assert_eq!(contested["usage"], claimants(&["a.rst", "e.rst", "f.rst"]));
    }
}

//! Resolving a cross-reference against the other sites' inventories the
//! index holds — Sphinx's intersphinx — once no document of this site has
//! defined the target.
//!
//! # Search order
//!
//! Sphinx's `_resolve_reference_detect_inventory`, in two steps:
//!
//! 1. the target exactly as written, in every inventory, in the order the
//!    build declared them — the first inventory listing it wins, so the
//!    order is the author's to choose;
//! 2. only if that fails and the target has a `name:` prefix naming a
//!    declared inventory, the rest of the target in that inventory alone.
//!
//! Step 1 comes first so that a target which merely *contains* a colon
//! (`std:label`-style names are common) still resolves as written.
//!
//! Within one inventory, the entry types a role accepts are tried in order —
//! the same [`ObjectType::role_alias_candidates`] a local lookup uses, plus
//! the one type other sites publish that this build never defines (see
//! [`external_entry_types`]).

use rusty_sphinx_ast::{InventorySelector, ObjectType, PyObjectType};
use rusty_sphinx_index::{ExternalInventory, ExternalTarget};

use crate::BrokenLinkKind;

/// A target another site's inventory lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExternalHit<'a> {
    pub inventory: &'a ExternalInventory,
    pub target: &'a ExternalTarget,
}

impl ExternalHit<'_> {
    /// The href a page at `doc_path` links the target by.
    pub(crate) fn href(&self, doc_path: &str) -> String {
        self.inventory.href(self.target, doc_path)
    }

    /// The tooltip Sphinx gives an intersphinx link, naming where it leads:
    /// `(in Python v3.12)`.
    pub(crate) fn tooltip(&self) -> String {
        if self.inventory.version.is_empty() {
            format!("(in {})", self.inventory.project)
        } else {
            format!(
                "(in {} v{})",
                self.inventory.project, self.inventory.version
            )
        }
    }
}

/// Searches `inventories` for `target` under any of `entry_types`, in the
/// order the module documentation gives — or, for an `:external+name:`
/// role, in the one inventory it names and nowhere else.
pub(crate) fn resolve_external<'a>(
    inventories: &'a [ExternalInventory],
    entry_types: &[String],
    target: &str,
    selector: &InventorySelector,
) -> Option<ExternalHit<'a>> {
    if let InventorySelector::Named(name) = selector {
        let inventory = inventories
            .iter()
            .find(|inventory| &inventory.name == name)?;
        return lookup_in(inventory, entry_types, target);
    }
    inventories
        .iter()
        .find_map(|inventory| lookup_in(inventory, entry_types, target))
        .or_else(|| {
            let (name, rest) = target.split_once(':')?;
            let inventory = inventories
                .iter()
                .find(|inventory| inventory.name.as_str() == name)?;
            lookup_in(inventory, entry_types, rest)
        })
}

/// What an unresolved reference is reported as: `local` — the kind the role
/// reports anyway — unless the role named an inventory the build never
/// declared, which is the more useful thing to say, since then nothing was
/// searched at all.
pub(crate) fn unresolved_kind(
    selector: &InventorySelector,
    inventories: &[ExternalInventory],
    local: BrokenLinkKind,
) -> BrokenLinkKind {
    match selector {
        InventorySelector::Named(name)
            if !inventories.iter().any(|inventory| &inventory.name == name) =>
        {
            BrokenLinkKind::UnknownInventory(name.clone())
        }
        _ => local,
    }
}

/// The first of `entry_types` under which `inventory` lists `target`.
fn lookup_in<'a>(
    inventory: &'a ExternalInventory,
    entry_types: &[String],
    target: &str,
) -> Option<ExternalHit<'a>> {
    entry_types.iter().find_map(|entry_type| {
        inventory
            .lookup(entry_type, target)
            .map(|found| ExternalHit {
                inventory,
                target: found,
            })
    })
}

/// The inventory entry types a role asking for `object_type` accepts.
///
/// The local aliases first, in the same preference order a local lookup
/// uses, then — for `:attr:` alone — `py:property`, which Sphinx's Python
/// domain lets that role reach but this build has no directive for. A local
/// lookup can never meet one, but the inventory of any project documenting a
/// property with `.. py:property::` lists it. (C needs no such extra: Sphinx
/// files a `.. c:var::` under `c:member`, which the local aliases cover.)
pub(crate) fn external_entry_types(object_type: ObjectType) -> Vec<String> {
    let mut types: Vec<String> = object_type
        .role_alias_candidates()
        .iter()
        .map(ObjectType::domain_qualified_str)
        .collect();
    if object_type == ObjectType::Py(PyObjectType::Attribute) {
        types.push("py:property".to_string());
    }
    types
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_sphinx_ast::CObjectType;
    use rusty_sphinx_inventory::{EntryType, Inventory, InventoryEntry, InventoryName};

    fn inventory(name: &str, entries: &[(&str, &str, &str)]) -> ExternalInventory {
        ExternalInventory::new(
            InventoryName::new(name).unwrap(),
            format!("https://{name}.org/"),
            Inventory {
                project: name.to_string(),
                version: "1.0".to_string(),
                entries: entries
                    .iter()
                    .map(|(entry_name, entry_type, uri)| InventoryEntry {
                        name: (*entry_name).to_string(),
                        entry_type: EntryType::new(entry_type).unwrap(),
                        priority: 1,
                        uri: (*uri).to_string(),
                        display_name: None,
                    })
                    .collect(),
            },
        )
    }

    fn labels() -> Vec<String> {
        vec!["std:label".to_string()]
    }

    #[test]
    fn test_resolve_external_takes_the_first_declared_inventory_listing_the_target() {
        // Given — two inventories both listing `intro`
        let inventories = vec![
            inventory("first", &[("intro", "std:label", "a.html#intro")]),
            inventory("second", &[("intro", "std:label", "b.html#intro")]),
        ];

        // When
        let hit =
            resolve_external(&inventories, &labels(), "intro", &InventorySelector::Any).unwrap();

        // Then
        assert_eq!(hit.inventory.name.as_str(), "first");
    }

    #[test]
    fn test_resolve_external_honours_an_inventory_prefix() {
        // Given
        let inventories = vec![
            inventory("first", &[("intro", "std:label", "a.html#intro")]),
            inventory("second", &[("intro", "std:label", "b.html#intro")]),
        ];

        // When
        let hit = resolve_external(
            &inventories,
            &labels(),
            "second:intro",
            &InventorySelector::Any,
        )
        .unwrap();

        // Then
        assert_eq!(hit.inventory.name.as_str(), "second");
        assert_eq!(hit.href("index.rst"), "https://second.org/b.html#intro");
    }

    #[test]
    fn test_resolve_external_tries_a_colon_target_as_written_first() {
        // Given — a label that itself contains a colon
        let inventories = vec![inventory(
            "first",
            &[("first:intro", "std:label", "a.html#first-intro")],
        )];

        // When
        let hit = resolve_external(
            &inventories,
            &labels(),
            "first:intro",
            &InventorySelector::Any,
        )
        .unwrap();

        // Then
        assert_eq!(hit.target.uri, "a.html#first-intro");
    }

    #[test]
    fn test_resolve_external_ignores_a_prefix_naming_no_inventory() {
        // Given
        let inventories = vec![inventory("first", &[("intro", "std:label", "a.html")])];

        // When / Then
        assert!(
            resolve_external(
                &inventories,
                &labels(),
                "nope:intro",
                &InventorySelector::Any
            )
            .is_none()
        );
    }

    #[test]
    fn test_resolve_external_tries_entry_types_in_order() {
        // Given — an exception, asked for by a class role
        let inventories = vec![inventory(
            "python",
            &[("ValueError", "py:exception", "exceptions.html#ValueError")],
        )];
        let types = external_entry_types(ObjectType::Py(PyObjectType::Class));

        // When
        let hit = resolve_external(&inventories, &types, "ValueError", &InventorySelector::Any);

        // Then
        assert!(hit.is_some());
    }

    #[test]
    fn test_resolve_external_finds_nothing_in_no_inventories() {
        // Given / When / Then
        assert!(resolve_external(&[], &labels(), "intro", &InventorySelector::Any).is_none());
    }

    #[test]
    fn test_resolve_external_searches_only_a_named_inventory() {
        // Given — both list `intro`, and the role names the second
        let inventories = vec![
            inventory("first", &[("intro", "std:label", "a.html#intro")]),
            inventory("second", &[("intro", "std:label", "b.html#intro")]),
        ];
        let selector = InventorySelector::Named(InventoryName::new("second").unwrap());

        // When
        let hit = resolve_external(&inventories, &labels(), "intro", &selector).unwrap();

        // Then
        assert_eq!(hit.inventory.name.as_str(), "second");
    }

    #[test]
    fn test_resolve_external_does_not_fall_back_from_a_named_inventory() {
        // Given — only the first lists `intro`, and the role names the second
        let inventories = vec![
            inventory("first", &[("intro", "std:label", "a.html#intro")]),
            inventory("second", &[]),
        ];
        let selector = InventorySelector::Named(InventoryName::new("second").unwrap());

        // When / Then
        assert!(resolve_external(&inventories, &labels(), "intro", &selector).is_none());
    }

    #[test]
    fn test_unresolved_kind_reports_an_undeclared_inventory() {
        // Given
        let inventories = vec![inventory("python", &[])];
        let selector = InventorySelector::Named(InventoryName::new("numpy").unwrap());

        // When
        let kind = unresolved_kind(&selector, &inventories, BrokenLinkKind::Reference);

        // Then
        assert_eq!(
            kind,
            BrokenLinkKind::UnknownInventory(InventoryName::new("numpy").unwrap())
        );
    }

    #[test]
    fn test_unresolved_kind_keeps_the_local_kind_otherwise() {
        // Given
        let inventories = vec![inventory("python", &[])];
        let declared = InventorySelector::Named(InventoryName::new("python").unwrap());

        // When / Then
        assert_eq!(
            unresolved_kind(&declared, &inventories, BrokenLinkKind::Reference),
            BrokenLinkKind::Reference
        );
        assert_eq!(
            unresolved_kind(
                &InventorySelector::ExternalOnly,
                &inventories,
                BrokenLinkKind::TermReference
            ),
            BrokenLinkKind::TermReference
        );
    }

    #[test]
    fn test_external_entry_types_adds_property_for_an_attribute_role() {
        // Given / When
        let types = external_entry_types(ObjectType::Py(PyObjectType::Attribute));

        // Then
        assert_eq!(types, ["py:attribute", "py:property"]);
    }

    #[test]
    fn test_external_entry_types_adds_nothing_for_a_c_member_role() {
        // Given / When
        let types = external_entry_types(ObjectType::C(CObjectType::Member));

        // Then
        assert_eq!(types, ["c:member", "c:macro"]);
    }

    #[test]
    fn test_external_entry_types_keeps_the_local_alias_order() {
        // Given / When
        let types = external_entry_types(ObjectType::Py(PyObjectType::Exception));

        // Then
        assert_eq!(types, ["py:exception", "py:class"]);
    }

    #[test]
    fn test_tooltip_names_project_and_version() {
        // Given
        let inventories = vec![inventory("python", &[("intro", "std:label", "a.html")])];
        let hit =
            resolve_external(&inventories, &labels(), "intro", &InventorySelector::Any).unwrap();

        // When / Then
        assert_eq!(hit.tooltip(), "(in python v1.0)");
    }
}

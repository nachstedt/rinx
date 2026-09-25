//! Another site's `objects.inv`, as this site's index holds it: declared
//! under a local name, keyed for lookup, and knowing where its pages live.

use std::collections::{BTreeMap, BTreeSet};

use rinx_ast::TargetName;
use rinx_inventory::{EntryType, Inventory, InventoryName};
use serde::{Deserialize, Serialize};

/// An inventory this site links into — Sphinx's `intersphinx_mapping` entry.
///
/// Held in the index rather than handed to each render action separately,
/// so that every phase resolving a reference searches one universe: the
/// live preview, which merges a fresh document into a stale index, finds
/// external targets without being told about them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalInventory {
    /// The name a document writes to pick this inventory out.
    pub name: InventoryName,
    /// Where the inventory's pages are published: an absolute URL
    /// (`https://docs.python.org/3/`) or a path relative to *this* site's
    /// root (`../api/`) for a sibling site deployed next to it.
    pub base_url: String,
    pub project: String,
    pub version: String,
    /// Every target, by type and then by [`lookup_key`].
    pub targets: BTreeMap<EntryType, BTreeMap<String, ExternalTarget>>,
}

/// One target of an [`ExternalInventory`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalTarget {
    /// The name as the inventory spells it.
    pub name: String,
    /// Where the target lives, relative to the inventory's `base_url`.
    pub uri: String,
    /// The text a reference with no explicit title shows, if not the name.
    pub display_name: Option<String>,
}

impl ExternalTarget {
    /// The text a reference to this target shows when the author wrote none.
    #[must_use]
    pub fn display_text(&self) -> &str {
        self.display_name.as_deref().unwrap_or(&self.name)
    }
}

impl ExternalInventory {
    /// Keys a read inventory for lookup. When one name appears twice under
    /// the same type the first entry wins, as Sphinx keeps a module's first
    /// entry.
    #[must_use]
    pub fn new(name: InventoryName, base_url: String, inventory: Inventory) -> Self {
        let mut targets: BTreeMap<EntryType, BTreeMap<String, ExternalTarget>> = BTreeMap::new();
        for entry in inventory.entries {
            let key = lookup_key(&entry.entry_type, &entry.name);
            targets
                .entry(entry.entry_type)
                .or_default()
                .entry(key)
                .or_insert(ExternalTarget {
                    name: entry.name,
                    uri: entry.uri,
                    display_name: entry.display_name,
                });
        }
        Self {
            name,
            base_url,
            project: inventory.project,
            version: inventory.version,
            targets,
        }
    }

    /// The target of type `entry_type` (`py:class`) named `name`, if listed.
    #[must_use]
    pub fn lookup(&self, entry_type: &str, name: &str) -> Option<&ExternalTarget> {
        let entry_type = EntryType::new(entry_type).ok()?;
        self.targets
            .get(&entry_type)?
            .get(&lookup_key(&entry_type, name))
    }

    /// The href a page at `doc_path` links `target` by.
    ///
    /// An absolute base is simply joined. A site-relative one is first joined
    /// and then made relative to the page, since a page two directories deep
    /// must climb out of them to reach a sibling site — the same
    /// relativization [`crate::relative_doc_href`] gives an internal link.
    #[must_use]
    pub fn href(&self, target: &ExternalTarget, doc_path: &str) -> String {
        let joined = join_uri(&self.base_url, &target.uri);
        if is_absolute(&self.base_url) {
            return joined;
        }
        let (path, fragment) = match joined.split_once('#') {
            Some((path, fragment)) => (path, Some(fragment)),
            None => (joined.as_str(), None),
        };
        let page_dir = std::path::Path::new(doc_path)
            .parent()
            .unwrap_or_else(|| std::path::Path::new(""));
        let relative = pathdiff::diff_paths(path, page_dir)
            .map_or_else(|| path.to_string(), |p| p.display().to_string());
        match fragment {
            Some(fragment) => format!("{relative}#{fragment}"),
            None => relative,
        }
    }
    /// Keeps only the targets some reference could resolve to, given every
    /// target a document of the site writes — so the index every render action
    /// loads carries what the site links to rather than the whole of every
    /// inventory it declares.
    ///
    /// Exact rather than heuristic, because an external lookup is: it tries the
    /// target as written, the part after a `name:` prefix, and — for an option —
    /// the target qualified by a program, never a scope-qualified name. So a
    /// target no written reference names can never be found, and dropping it
    /// changes no link. The one thing it costs is the live preview, which resolves
    /// against a stale index: a reference to an external target that no document
    /// named when the index was last built stays unresolved there until the next
    /// build.
    pub fn retain_referenced(&mut self, written: &BTreeSet<String>) {
        let normalized: BTreeSet<String> = written
            .iter()
            .map(|name| TargetName::new(name).as_str().to_string())
            .collect();
        for (entry_type, targets) in &mut self.targets {
            targets.retain(|key, target| match entry_type.as_str() {
                "std:label" | "std:term" => normalized.contains(key),
                // `program.option`, reached from an `:option:` written bare
                // under that program's `.. program::`.
                "std:cmdoption" => {
                    written.contains(key)
                        || key
                            .match_indices('.')
                            .any(|(dot, _)| written.contains(&key[dot + 1..]))
                }
                _ => written.contains(&target.name),
            });
        }
        self.targets.retain(|_, targets| !targets.is_empty());
    }
}

/// The key a name is filed and looked up under: normalized like a
/// [`TargetName`] for the two types Sphinx matches case-insensitively
/// (`std:label`, `std:term`), and verbatim for everything else — a Python
/// name is case-sensitive.
fn lookup_key(entry_type: &EntryType, name: &str) -> String {
    match entry_type.as_str() {
        "std:label" | "std:term" => TargetName::new(name).as_str().to_string(),
        _ => name.to_string(),
    }
}

/// Joins an inventory's base with an entry's location the way Sphinx's
/// `posixpath.join` does: an absolute location replaces the base, and
/// otherwise exactly one `/` separates them.
fn join_uri(base: &str, location: &str) -> String {
    if base.is_empty() || location.starts_with('/') || location.contains("://") {
        location.to_string()
    } else if base.ends_with('/') {
        format!("{base}{location}")
    } else {
        format!("{base}/{location}")
    }
}

/// Whether a base URL names a place independent of the linking page: a URL
/// with a scheme, or a server-absolute path.
fn is_absolute(base: &str) -> bool {
    base.contains("://") || base.starts_with('/')
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_inventory::InventoryEntry;

    fn inventory(entries: &[(&str, &str, &str, Option<&str>)]) -> Inventory {
        Inventory {
            project: "Python".to_string(),
            version: "3.12".to_string(),
            entries: entries
                .iter()
                .map(|(name, entry_type, uri, display)| InventoryEntry {
                    name: (*name).to_string(),
                    entry_type: EntryType::new(entry_type).unwrap(),
                    priority: 1,
                    uri: (*uri).to_string(),
                    display_name: display.map(str::to_string),
                })
                .collect(),
        }
    }

    fn python(base_url: &str) -> ExternalInventory {
        ExternalInventory::new(
            InventoryName::new("python").unwrap(),
            base_url.to_string(),
            inventory(&[
                ("dict", "py:class", "library/stdtypes.html#dict", None),
                (
                    "Tut-Intro",
                    "std:label",
                    "tutorial/index.html#tut-intro",
                    Some("An Informal Introduction"),
                ),
            ]),
        )
    }

    #[test]
    fn test_lookup_finds_a_python_name_exactly() {
        // Given
        let inventory = python("https://docs.python.org/3/");

        // When / Then
        assert!(inventory.lookup("py:class", "dict").is_some());
        assert!(inventory.lookup("py:class", "Dict").is_none());
    }

    #[test]
    fn test_lookup_matches_a_label_case_insensitively() {
        // Given
        let inventory = python("https://docs.python.org/3/");

        // When
        let target = inventory.lookup("std:label", "tut-intro").unwrap();

        // Then
        assert_eq!(target.name, "Tut-Intro");
        assert_eq!(target.display_text(), "An Informal Introduction");
    }

    #[test]
    fn test_lookup_refuses_a_malformed_type() {
        // Given
        let inventory = python("https://docs.python.org/3/");

        // When / Then
        assert!(inventory.lookup("class", "dict").is_none());
    }

    #[test]
    fn test_new_keeps_the_first_of_two_entries_with_one_name() {
        // Given
        let inventory = ExternalInventory::new(
            InventoryName::new("x").unwrap(),
            String::new(),
            inventory(&[
                ("m", "py:module", "first.html", None),
                ("m", "py:module", "second.html", None),
            ]),
        );

        // When / Then
        assert_eq!(
            inventory.lookup("py:module", "m").unwrap().uri,
            "first.html"
        );
    }

    fn written(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|name| (*name).to_string()).collect()
    }

    #[test]
    fn test_retain_referenced_keeps_only_written_names() {
        // Given
        let mut inventory = python("https://docs.python.org/3/");

        // When
        inventory.retain_referenced(&written(&["dict"]));

        // Then
        assert!(inventory.lookup("py:class", "dict").is_some());
        assert!(inventory.lookup("std:label", "tut-intro").is_none());
    }

    #[test]
    fn test_retain_referenced_matches_a_label_as_a_lookup_would() {
        // Given — written in another case than the inventory's key
        let mut inventory = python("https://docs.python.org/3/");

        // When
        inventory.retain_referenced(&written(&["TUT-INTRO"]));

        // Then
        assert!(inventory.lookup("std:label", "tut-intro").is_some());
    }

    #[test]
    fn test_retain_referenced_keeps_a_program_option_named_bare() {
        // Given — `:option:`-X`` written under `.. program:: python`
        let mut inventory = ExternalInventory::new(
            InventoryName::new("python").unwrap(),
            String::new(),
            inventory(&[
                ("python.-X", "std:cmdoption", "cmdline.html#x", None),
                ("python.-O", "std:cmdoption", "cmdline.html#o", None),
            ]),
        );

        // When
        inventory.retain_referenced(&written(&["-X"]));

        // Then
        assert!(inventory.lookup("std:cmdoption", "python.-X").is_some());
        assert!(inventory.lookup("std:cmdoption", "python.-O").is_none());
    }

    #[test]
    fn test_retain_referenced_drops_emptied_types() {
        // Given
        let mut inventory = python("https://docs.python.org/3/");

        // When
        inventory.retain_referenced(&written(&[]));

        // Then
        assert!(inventory.targets.is_empty());
    }

    #[test]
    fn test_href_joins_an_absolute_base() {
        // Given
        let inventory = python("https://docs.python.org/3");
        let target = inventory.lookup("py:class", "dict").unwrap();

        // When
        let href = inventory.href(target, "guide/deep/page.rst");

        // Then
        assert_eq!(href, "https://docs.python.org/3/library/stdtypes.html#dict");
    }

    #[test]
    fn test_href_relativizes_a_site_relative_base_to_the_page() {
        // Given — a sibling site deployed at `../python/` from this site's root
        let inventory = python("../python/");
        let target = inventory.lookup("py:class", "dict").unwrap();

        // When
        let href = inventory.href(target, "guide/page.rst");

        // Then
        assert_eq!(href, "../../python/library/stdtypes.html#dict");
    }

    #[test]
    fn test_href_keeps_a_site_relative_base_as_is_from_the_root() {
        // Given
        let inventory = python("../python/");
        let target = inventory.lookup("py:class", "dict").unwrap();

        // When
        let href = inventory.href(target, "index.rst");

        // Then
        assert_eq!(href, "../python/library/stdtypes.html#dict");
    }

    #[test]
    fn test_join_uri_follows_posixpath_join() {
        // Given / When / Then
        assert_eq!(
            join_uri("https://a.org/3", "x.html"),
            "https://a.org/3/x.html"
        );
        assert_eq!(
            join_uri("https://a.org/3/", "x.html"),
            "https://a.org/3/x.html"
        );
        assert_eq!(join_uri("https://a.org/3/", "/abs.html"), "/abs.html");
        assert_eq!(join_uri("", "x.html"), "x.html");
    }

    #[test]
    fn test_is_absolute_recognizes_urls_and_rooted_paths() {
        // Given / When / Then
        assert!(is_absolute("https://docs.python.org/3/"));
        assert!(is_absolute("/docs/"));
        assert!(!is_absolute("../sibling/"));
    }

    #[test]
    fn test_serialization_roundtrip() {
        // Given
        let inventory = python("https://docs.python.org/3/");

        // When
        let json = serde_json::to_string(&inventory).unwrap();
        let back: ExternalInventory = serde_json::from_str(&json).unwrap();

        // Then
        assert_eq!(back, inventory);
    }
}

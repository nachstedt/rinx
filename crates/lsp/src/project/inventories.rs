//! Loading a project's intersphinx inventories (ADR-038 §6).
//!
//! Each `intersphinx_mapping` entry names its inventory's sources in order,
//! and the first that loads is the entry's inventory, as in Sphinx. A file is
//! read here; an address is the editor's to download — the server never
//! touches the network — so an entry stops at one until the server learns
//! what became of it, through the `fetched` answer every step is given.
//!
//! An entry is loaded, waiting, or unavailable. Only a loaded inventory
//! reaches the project index; the others are what the strictness filter
//! lowers a reference for (see [`super::strictness`]), since the reference
//! may well be in the inventory the editor does not have.

use std::path::{Path, PathBuf};

use rinx_ast::{Diagnostic, DiagnosticCode};
use rinx_index::ExternalInventory;
use rinx_inventory::{InventoryName, read_inventory};

use super::intersphinx::{IntersphinxEntry, IntersphinxSettings, InventorySource};

/// What became of a download the server asked the editor for: the file it
/// was saved to, or why there is none.
pub type Fetched = Result<PathBuf, String>;

/// Why an entry the editor has not answered for is not loaded.
pub const NOT_DOWNLOADED: &str = "not downloaded yet";

/// The inventories of one project's `intersphinx_mapping`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProjectInventories {
    entries: Vec<EntryLoad>,
}

/// One entry, and how far loading it has come.
#[derive(Debug, Clone, PartialEq, Eq)]
struct EntryLoad {
    entry: IntersphinxEntry,
    /// The source tried next.
    next: usize,
    state: InventoryState,
    /// What made each source tried so far fail.
    problems: Vec<Problem>,
}

/// Where loading one entry stands.
#[derive(Debug, Clone, PartialEq, Eq)]
enum InventoryState {
    /// Not tried yet, or set back to its first source.
    Pending,
    /// Waiting for the editor's download of this address.
    Awaiting(String),
    Loaded(ExternalInventory),
    /// Every source failed.
    Unavailable,
}

/// Why one source of an entry did not load.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Problem {
    /// The file, or the download, is missing or is no inventory: something
    /// the author can fix, reported in `conf.py`.
    Unreadable(String),
    /// The editor did not download it — declined, offline, or unable to.
    NotFetched(String),
}

impl ProjectInventories {
    /// The inventories `settings` declares, none of them loaded yet.
    #[must_use]
    pub fn new(settings: &IntersphinxSettings) -> Self {
        Self {
            entries: settings
                .entries
                .iter()
                .map(|entry| EntryLoad {
                    entry: entry.clone(),
                    next: 0,
                    state: InventoryState::Pending,
                    problems: Vec::new(),
                })
                .collect(),
        }
    }

    /// Loads what can be loaded now: each source of each waiting entry in
    /// turn, a file `read` relative to `conf_dir`, an address by what
    /// `fetched` says became of its download (`None` while unknown, which
    /// the entry then waits for). Returns whether any entry changed.
    pub fn advance(
        &mut self,
        conf_dir: &Path,
        read: &impl Fn(&Path) -> std::io::Result<Vec<u8>>,
        fetched: &impl Fn(&str) -> Option<Fetched>,
    ) -> bool {
        let mut changed = false;
        for load in &mut self.entries {
            changed |= load.advance(conf_dir, read, fetched);
        }
        changed
    }

    /// The addresses some entry waits for the editor to download.
    #[must_use]
    pub fn awaited(&self) -> Vec<&str> {
        self.entries
            .iter()
            .filter_map(|load| match &load.state {
                InventoryState::Awaiting(url) => Some(url.as_str()),
                _ => None,
            })
            .collect()
    }

    /// Sets every entry reading the file at `path` back to its first source,
    /// so it is read again — returning whether one did.
    pub fn reset_reading(&mut self, conf_dir: &Path, path: &Path) -> bool {
        let mut reset = false;
        for load in &mut self.entries {
            if load.files(conf_dir).any(|file| file == path) {
                load.next = 0;
                load.state = InventoryState::Pending;
                load.problems.clear();
                reset = true;
            }
        }
        reset
    }

    /// The loaded inventories, in the order the mapping declares them.
    pub fn loaded(&self) -> impl Iterator<Item = &ExternalInventory> {
        self.entries.iter().filter_map(|load| match &load.state {
            InventoryState::Loaded(inventory) => Some(inventory),
            _ => None,
        })
    }

    /// Every entry not loaded, with why.
    #[must_use]
    pub fn unloaded(&self) -> Vec<(InventoryName, String)> {
        self.entries
            .iter()
            .filter_map(|load| match &load.state {
                InventoryState::Loaded(_) => None,
                InventoryState::Pending | InventoryState::Awaiting(_) => {
                    Some((load.entry.name.clone(), NOT_DOWNLOADED.to_string()))
                }
                InventoryState::Unavailable => {
                    let reasons: Vec<&str> = load
                        .problems
                        .iter()
                        .map(|problem| match problem {
                            Problem::Unreadable(reason) | Problem::NotFetched(reason) => {
                                reason.as_str()
                            }
                        })
                        .collect();
                    Some((load.entry.name.clone(), reasons.join("; ")))
                }
            })
            .collect()
    }

    /// What `conf.py` shows about its inventories: each entry none of whose
    /// sources loaded, when one of them was unreadable. One the editor only
    /// did not download is not the author's to fix there.
    #[must_use]
    pub fn findings(&self) -> Vec<Diagnostic> {
        self.entries
            .iter()
            .filter(|load| load.state == InventoryState::Unavailable)
            .filter_map(|load| {
                let unreadable: Vec<&str> = load
                    .problems
                    .iter()
                    .filter_map(|problem| match problem {
                        Problem::Unreadable(reason) => Some(reason.as_str()),
                        Problem::NotFetched(_) => None,
                    })
                    .collect();
                (!unreadable.is_empty()).then(|| {
                    Diagnostic::new(
                        DiagnosticCode::ConfInventoryUnreadable,
                        format!(
                            "the inventory of `{}` could not be read: {}",
                            load.entry.name.as_str(),
                            unreadable.join("; ")
                        ),
                        load.entry.span,
                    )
                })
            })
            .collect()
    }
}

impl EntryLoad {
    fn advance(
        &mut self,
        conf_dir: &Path,
        read: &impl Fn(&Path) -> std::io::Result<Vec<u8>>,
        fetched: &impl Fn(&str) -> Option<Fetched>,
    ) -> bool {
        let before = self.state.clone();
        while matches!(
            self.state,
            InventoryState::Pending | InventoryState::Awaiting(_)
        ) {
            let Some(source) = self.entry.sources.get(self.next) else {
                self.state = InventoryState::Unavailable;
                break;
            };
            let outcome = match source {
                InventorySource::File(file) => {
                    let path = conf_dir.join(file);
                    Some(self.read_file(&path, read, &path.display().to_string()))
                }
                InventorySource::Url(url) => match fetched(url) {
                    None => {
                        self.state = InventoryState::Awaiting(url.clone());
                        break;
                    }
                    Some(Ok(path)) => Some(self.read_file(&path, read, url)),
                    Some(Err(reason)) => Some(Err(Problem::NotFetched(format!("{url}: {reason}")))),
                },
            };
            match outcome {
                Some(Ok(inventory)) => self.state = InventoryState::Loaded(inventory),
                Some(Err(problem)) => {
                    self.problems.push(problem);
                    self.next += 1;
                }
                None => break,
            }
        }
        self.state != before
    }

    /// The inventory in the file at `path`, read from `source`.
    fn read_file(
        &self,
        path: &Path,
        read: &impl Fn(&Path) -> std::io::Result<Vec<u8>>,
        source: &str,
    ) -> Result<ExternalInventory, Problem> {
        let bytes =
            read(path).map_err(|error| Problem::Unreadable(format!("{source}: {error}")))?;
        let inventory = read_inventory(&bytes)
            .map_err(|error| Problem::Unreadable(format!("{source}: {error}")))?;
        Ok(ExternalInventory::new(
            self.entry.name.clone(),
            self.entry.base_url.clone(),
            inventory.inventory,
        ))
    }

    /// The files this entry may be read from.
    fn files<'a>(&'a self, conf_dir: &'a Path) -> impl Iterator<Item = PathBuf> + 'a {
        self.entry
            .sources
            .iter()
            .filter_map(move |source| match source {
                InventorySource::File(file) => Some(conf_dir.join(file)),
                InventorySource::Url(_) => None,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::{Position, Span};
    use rinx_inventory::{EntryType, Inventory, InventoryEntry, write_inventory};
    use std::collections::BTreeMap;
    use std::io;

    const SPAN: Span = Span::new(Position::new(3, 5), Position::new(3, 13));

    fn entry(name: &str, sources: Vec<InventorySource>) -> IntersphinxEntry {
        IntersphinxEntry {
            name: InventoryName::new(name).unwrap(),
            base_url: format!("https://{name}.org/"),
            sources,
            span: SPAN,
        }
    }

    fn inventories(entries: Vec<IntersphinxEntry>) -> ProjectInventories {
        ProjectInventories::new(&IntersphinxSettings {
            entries,
            cache_limit_days: 5,
        })
    }

    /// An inventory listing the Python class `class`.
    fn inventory_bytes(class: &str) -> Vec<u8> {
        write_inventory(&Inventory {
            project: "P".to_string(),
            version: "1".to_string(),
            entries: vec![InventoryEntry {
                name: class.to_string(),
                entry_type: EntryType::new("py:class").unwrap(),
                priority: 1,
                uri: format!("{class}.html#$"),
                display_name: None,
            }],
        })
    }

    /// A disk holding `files`.
    fn disk(files: &[(&str, Vec<u8>)]) -> impl Fn(&Path) -> io::Result<Vec<u8>> + use<> {
        let files: BTreeMap<PathBuf, Vec<u8>> = files
            .iter()
            .map(|(path, bytes)| (PathBuf::from(path), bytes.clone()))
            .collect();
        move |path: &Path| {
            files
                .get(path)
                .cloned()
                .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
        }
    }

    fn unknown(_: &str) -> Option<Fetched> {
        None
    }

    fn file(path: &str) -> InventorySource {
        InventorySource::File(PathBuf::from(path))
    }

    fn url(url: &str) -> InventorySource {
        InventorySource::Url(url.to_string())
    }

    fn names(loaded: &ProjectInventories) -> Vec<&str> {
        loaded
            .loaded()
            .map(|inventory| inventory.name.as_str())
            .collect()
    }

    #[test]
    fn test_advance_reads_a_file_relative_to_the_conf_dir() {
        // Given
        let mut inventories = inventories(vec![entry("a", vec![file("_inv/a.inv")])]);
        let read = disk(&[("/docs/_inv/a.inv", inventory_bytes("A"))]);

        // When
        let changed = inventories.advance(Path::new("/docs"), &read, &unknown);

        // Then
        assert!(changed);
        assert_eq!(names(&inventories), ["a"]);
        let loaded = inventories.loaded().next().unwrap();
        assert!(loaded.lookup("py:class", "A").is_some());
        assert_eq!(loaded.base_url, "https://a.org/");
        assert!(inventories.unloaded().is_empty());
    }

    #[test]
    fn test_advance_waits_for_an_address_until_its_download_is_known() {
        // Given
        let mut inventories = inventories(vec![entry("a", vec![url("https://a.org/objects.inv")])]);
        let read = disk(&[("/cache/a.inv", inventory_bytes("A"))]);

        // When
        inventories.advance(Path::new("/docs"), &read, &unknown);

        // Then
        assert_eq!(inventories.awaited(), ["https://a.org/objects.inv"]);
        assert_eq!(
            inventories.unloaded(),
            [(InventoryName::new("a").unwrap(), NOT_DOWNLOADED.to_string())]
        );
        assert!(!inventories.advance(Path::new("/docs"), &read, &unknown));

        // When — the editor saved it
        let changed = inventories.advance(Path::new("/docs"), &read, &|_: &str| {
            Some(Ok(PathBuf::from("/cache/a.inv")))
        });

        // Then
        assert!(changed);
        assert_eq!(names(&inventories), ["a"]);
        assert!(inventories.awaited().is_empty());
    }

    #[test]
    fn test_advance_tries_the_next_source_when_one_fails() {
        // Given — a missing file, then a download
        let mut inventories = inventories(vec![entry(
            "a",
            vec![file("missing.inv"), url("https://a.org/objects.inv")],
        )]);
        let read = disk(&[("/cache/a.inv", inventory_bytes("A"))]);

        // When
        inventories.advance(Path::new("/docs"), &read, &|_: &str| {
            Some(Ok(PathBuf::from("/cache/a.inv")))
        });

        // Then — the first failure is forgotten once a source loads
        assert_eq!(names(&inventories), ["a"]);
        assert!(inventories.findings().is_empty());
    }

    #[test]
    fn test_a_declined_download_is_unavailable_but_no_finding() {
        // Given
        let mut inventories = inventories(vec![entry("a", vec![url("https://a.org/objects.inv")])]);

        // When
        inventories.advance(Path::new("/docs"), &disk(&[]), &|_: &str| {
            Some(Err("declined".to_string()))
        });

        // Then
        assert!(inventories.findings().is_empty());
        assert_eq!(
            inventories.unloaded(),
            [(
                InventoryName::new("a").unwrap(),
                "https://a.org/objects.inv: declined".to_string()
            )]
        );
    }

    #[test]
    fn test_an_unreadable_inventory_is_a_finding_on_its_entry() {
        // Given — a missing file, and a file that is no inventory
        let mut inventories = inventories(vec![
            entry("a", vec![file("missing.inv")]),
            entry("b", vec![file("b.inv")]),
        ]);
        let read = disk(&[("/docs/b.inv", b"not an inventory".to_vec())]);

        // When
        inventories.advance(Path::new("/docs"), &read, &unknown);

        // Then
        let findings = inventories.findings();
        assert_eq!(findings.len(), 2);
        assert_eq!(findings[0].code, DiagnosticCode::ConfInventoryUnreadable);
        assert_eq!(findings[0].span, Some(SPAN));
        assert!(
            findings[0].message.contains("`a`"),
            "{}",
            findings[0].message
        );
        assert!(
            findings[1].message.contains("`b`"),
            "{}",
            findings[1].message
        );
    }

    #[test]
    fn test_loaded_keeps_declaration_order() {
        // Given — `b` loads at once, `a` later
        let mut inventories = inventories(vec![
            entry("a", vec![url("https://a.org/objects.inv")]),
            entry("b", vec![file("b.inv")]),
        ]);
        let read = disk(&[
            ("/docs/b.inv", inventory_bytes("B")),
            ("/cache/a.inv", inventory_bytes("A")),
        ]);
        inventories.advance(Path::new("/docs"), &read, &unknown);

        // When
        inventories.advance(Path::new("/docs"), &read, &|_: &str| {
            Some(Ok(PathBuf::from("/cache/a.inv")))
        });

        // Then
        assert_eq!(names(&inventories), ["a", "b"]);
    }

    #[test]
    fn test_reset_reading_reads_an_edited_file_again() {
        // Given
        let mut inventories = inventories(vec![
            entry("a", vec![file("a.inv")]),
            entry("b", vec![file("b.inv")]),
        ]);
        inventories.advance(Path::new("/docs"), &disk(&[]), &unknown);
        assert!(inventories.loaded().next().is_none());

        // When — `a.inv` appears
        let reset = inventories.reset_reading(Path::new("/docs"), Path::new("/docs/a.inv"));
        let read = disk(&[("/docs/a.inv", inventory_bytes("A"))]);
        inventories.advance(Path::new("/docs"), &read, &unknown);

        // Then
        assert!(reset);
        assert_eq!(names(&inventories), ["a"]);
        assert!(!inventories.reset_reading(Path::new("/docs"), Path::new("/docs/c.inv")));
    }
}

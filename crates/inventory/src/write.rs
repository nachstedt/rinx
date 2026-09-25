//! Writing an `objects.inv` file, following `sphinx.util.inventory`'s
//! `InventoryFile.dump`.

use std::io::Write as _;

use crate::{Inventory, InventoryEntry};

/// Writes `inventory` as a version 2 inventory file.
///
/// The output is deterministic, since it is a build artefact a cache compares
/// by content: entries are sorted the way Sphinx orders them — by domain,
/// then by name — whatever order they arrive in, and the body is compressed
/// at a fixed level (Sphinx's own, 9). The `$` and `-` abbreviations are
/// re-applied exactly where Sphinx applies them.
///
/// # Panics
///
/// Never in practice: the only fallible step is compressing into an
/// in-memory `Vec`, which cannot fail to write.
#[must_use]
pub fn write_inventory(inventory: &Inventory) -> Vec<u8> {
    let mut bytes = format!(
        "# Sphinx inventory version 2\n# Project: {}\n# Version: {}\n\
         # The remainder of this file is compressed using zlib.\n",
        collapse_whitespace(&inventory.project),
        collapse_whitespace(&inventory.version),
    )
    .into_bytes();

    let mut entries: Vec<&InventoryEntry> = inventory.entries.iter().collect();
    entries.sort_by(|a, b| {
        (a.entry_type.domain(), &a.name, a.entry_type.role(), &a.uri).cmp(&(
            b.entry_type.domain(),
            &b.name,
            b.entry_type.role(),
            &b.uri,
        ))
    });
    let body: String = entries.into_iter().map(entry_line).collect();

    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::new(9));
    encoder
        .write_all(body.as_bytes())
        .expect("writing to a Vec cannot fail");
    bytes.extend(encoder.finish().expect("writing to a Vec cannot fail"));
    bytes
}

/// One body line: `name domain:role priority uri display-name`.
fn entry_line(entry: &InventoryEntry) -> String {
    let display = match entry.display_name.as_deref() {
        Some(display) if display != entry.name => collapse_whitespace(display),
        _ => "-".to_string(),
    };
    format!(
        "{} {} {} {} {}\n",
        entry.name,
        entry.entry_type.as_str(),
        entry.priority,
        abbreviate_uri(&entry.uri, &entry.name),
        display
    )
}

/// Replaces a trailing copy of `name` in the URI's *anchor* with `$`, as
/// Sphinx does — never in the page path, which Sphinx never abbreviates.
fn abbreviate_uri(uri: &str, name: &str) -> String {
    match uri.split_once('#') {
        Some((page, anchor)) if !name.is_empty() => match anchor.strip_suffix(name) {
            Some(prefix) => format!("{page}#{prefix}$"),
            None => uri.to_string(),
        },
        _ => uri.to_string(),
    }
}

/// Collapses every whitespace run to one space, as Sphinx's `escape` does for
/// header values — a newline would otherwise end the line early.
fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EntryType, read_inventory};

    fn entry(name: &str, entry_type: &str, uri: &str, display: Option<&str>) -> InventoryEntry {
        InventoryEntry {
            name: name.to_string(),
            entry_type: EntryType::new(entry_type).unwrap(),
            priority: 1,
            uri: uri.to_string(),
            display_name: display.map(str::to_string),
        }
    }

    /// The decompressed body of a written file, for asserting on its lines.
    fn body_of(bytes: &[u8]) -> String {
        use std::io::Read as _;
        let mut rest = bytes;
        for _ in 0..4 {
            let end = rest.iter().position(|&b| b == b'\n').unwrap();
            rest = &rest[end + 1..];
        }
        let mut body = String::new();
        flate2::read::ZlibDecoder::new(rest)
            .read_to_string(&mut body)
            .unwrap();
        body
    }

    #[test]
    fn test_write_inventory_roundtrips_through_the_reader() {
        // Given
        let inventory = Inventory {
            project: "Demo".to_string(),
            version: "2.0".to_string(),
            entries: vec![
                entry("pkg.Greeter", "py:class", "api.html#pkg.Greeter", None),
                entry("build step", "std:term", "index.html#term-build-step", None),
                entry(
                    "intro",
                    "std:label",
                    "index.html#intro",
                    Some("Introduction"),
                ),
            ],
        };

        // When
        let read = read_inventory(&write_inventory(&inventory)).unwrap();

        // Then
        assert!(read.malformed_lines.is_empty());
        assert_eq!(read.inventory.project, "Demo");
        assert_eq!(read.inventory.version, "2.0");
        for written in &inventory.entries {
            assert!(read.inventory.entries.contains(written), "{written:?}");
        }
    }

    #[test]
    fn test_write_inventory_reproduces_real_sphinx_lines() {
        // Given — the checked-in `sphinx-build` output, read and re-written
        let original = include_bytes!("../testdata/sphinx-9.1.0.inv");
        let inventory = read_inventory(original).unwrap().inventory;

        // When
        let rewritten = write_inventory(&inventory);

        // Then — the same lines in the same order as Sphinx wrote them
        assert_eq!(body_of(&rewritten), body_of(original));
    }

    #[test]
    fn test_write_inventory_sorts_by_domain_then_name() {
        // Given — deliberately out of order
        let inventory = Inventory {
            entries: vec![
                entry("b", "std:label", "x.html#b", None),
                entry("z", "py:function", "x.html#z", None),
                entry("a", "std:label", "x.html#a", None),
            ],
            ..Inventory::default()
        };

        // When
        let body = body_of(&write_inventory(&inventory));

        // Then
        let names: Vec<&str> = body
            .lines()
            .map(|line| line.split(' ').next().unwrap())
            .collect();
        assert_eq!(names, ["z", "a", "b"]);
    }

    #[test]
    fn test_write_inventory_is_deterministic() {
        // Given
        let inventory = Inventory {
            entries: vec![entry("a", "std:label", "x.html#a", None)],
            ..Inventory::default()
        };

        // When / Then
        assert_eq!(write_inventory(&inventory), write_inventory(&inventory));
    }

    #[test]
    fn test_entry_line_writes_a_dash_for_a_display_name_equal_to_the_name() {
        // Given
        let entry = entry("intro", "std:label", "a.html#intro", Some("intro"));

        // When / Then
        assert_eq!(entry_line(&entry), "intro std:label 1 a.html#$ -\n");
    }

    #[test]
    fn test_abbreviate_uri_replaces_the_name_at_the_anchor_end() {
        // Given / When / Then
        assert_eq!(
            abbreviate_uri("api.html#module-pkg", "pkg"),
            "api.html#module-$"
        );
    }

    #[test]
    fn test_abbreviate_uri_leaves_a_page_uri_alone() {
        // Given — the name matches the page, not an anchor
        // When / Then
        assert_eq!(abbreviate_uri("api.html", "api.html"), "api.html");
    }

    #[test]
    fn test_abbreviate_uri_leaves_an_unrelated_anchor_alone() {
        // Given / When / Then
        assert_eq!(
            abbreviate_uri("index.html#cmdoption-verbose", "--verbose"),
            "index.html#cmdoption-verbose"
        );
    }

    #[test]
    fn test_collapse_whitespace_joins_lines() {
        // Given / When / Then
        assert_eq!(collapse_whitespace("My\n  Project "), "My Project");
    }
}

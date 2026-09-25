//! Reading an `objects.inv` file, following `sphinx.util.inventory`'s
//! `InventoryFile.load_v2` line for line.

use std::io::Read as _;
use std::sync::LazyLock;

use regex::Regex;

use crate::{EntryType, Inventory, InventoryEntry, InventoryError, MalformedLine};

/// Sphinx's own entry pattern. The lazy name is what lets a name hold spaces
/// (`build step std:term -1 …`): it grows only until the rest of the line
/// parses as the four remaining fields.
static ENTRY_LINE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^(?<name>.+?)\s+(?<type>\S+)\s+(?<priority>-?\d+)\s+?(?<uri>\S*)\s+(?<display>.*)$",
    )
    .expect("entry pattern is valid")
});

/// A successfully read inventory, and the body lines Sphinx would have skipped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadInventory {
    pub inventory: Inventory,
    pub malformed_lines: Vec<MalformedLine>,
}

/// Reads a version 2 inventory file.
///
/// # Errors
///
/// Returns [`InventoryError`] when the header is not Sphinx's, the file is a
/// version 1 inventory, or the body cannot be decompressed or decoded. A
/// single unreadable *body* line is not an error: it lands in
/// [`ReadInventory::malformed_lines`] and the rest of the file is kept.
pub fn read_inventory(bytes: &[u8]) -> Result<ReadInventory, InventoryError> {
    let mut rest = bytes;
    let version_line = take_header_line(&mut rest).ok_or(InventoryError::NotAnInventory)?;
    match version_line.trim_end() {
        "# Sphinx inventory version 2" => {}
        "# Sphinx inventory version 1" => return Err(InventoryError::UnsupportedVersion1),
        _ => return Err(InventoryError::NotAnInventory),
    }
    let project = header_value(&mut rest, "# Project: ", "`# Project:`")?;
    let version = header_value(&mut rest, "# Version: ", "`# Version:`")?;
    let compression_line = take_header_line(&mut rest).ok_or(InventoryError::MalformedHeader {
        expected: "compression notice",
    })?;
    if !compression_line.contains("zlib") {
        return Err(InventoryError::MalformedHeader {
            expected: "compression notice",
        });
    }

    let mut body = Vec::new();
    flate2::read::ZlibDecoder::new(rest)
        .read_to_end(&mut body)
        .map_err(|error| InventoryError::Decompression(error.to_string()))?;
    let body = String::from_utf8(body).map_err(|_| InventoryError::NotUtf8)?;

    let mut entries = Vec::new();
    let mut malformed_lines = Vec::new();
    for (offset, line) in body.split('\n').enumerate() {
        let line = line.trim_end();
        if line.is_empty() {
            continue;
        }
        match parse_entry_line(line) {
            Ok(entry) => entries.push(entry),
            Err(reason) => malformed_lines.push(MalformedLine {
                // Four header lines, then a 1-based body line.
                line: offset + 5,
                text: line.to_string(),
                reason,
            }),
        }
    }

    Ok(ReadInventory {
        inventory: Inventory {
            project,
            version,
            entries,
        },
        malformed_lines,
    })
}

/// Splits one `\n`-terminated header line off the front of `rest`, decoded
/// as UTF-8. `None` when there is no complete line or it is not UTF-8.
fn take_header_line<'a>(rest: &mut &'a [u8]) -> Option<&'a str> {
    let end = rest.iter().position(|&byte| byte == b'\n')?;
    let line = std::str::from_utf8(&rest[..end]).ok()?;
    *rest = &rest[end + 1..];
    Some(line)
}

/// Reads the next header line, which must start with `prefix`, and returns
/// what follows it.
fn header_value(
    rest: &mut &[u8],
    prefix: &str,
    expected: &'static str,
) -> Result<String, InventoryError> {
    take_header_line(rest)
        .and_then(|line| line.strip_prefix(prefix))
        .map(|value| value.trim_end().to_string())
        .ok_or(InventoryError::MalformedHeader { expected })
}

/// Parses one body line into an entry, expanding the `$` and `-`
/// abbreviations, or says why Sphinx would have skipped it.
fn parse_entry_line(line: &str) -> Result<InventoryEntry, &'static str> {
    let caps = ENTRY_LINE
        .captures(line)
        .ok_or("not the five fields `name domain:role priority uri display-name`")?;
    let name = &caps["name"];
    let entry_type = EntryType::new(&caps["type"]).map_err(|_| "type is not `domain:role`")?;
    let priority = caps["priority"]
        .parse::<i32>()
        .map_err(|_| "priority is out of range")?;
    let uri = match caps["uri"].strip_suffix('$') {
        Some(prefix) => format!("{prefix}{name}"),
        None => caps["uri"].to_string(),
    };
    let display_name = match &caps["display"] {
        "-" => None,
        display => Some(display.to_string()),
    };
    Ok(InventoryEntry {
        name: name.to_string(),
        entry_type,
        priority,
        uri,
        display_name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    /// An inventory file with the standard header around `body`.
    fn inventory_file(body: &str) -> Vec<u8> {
        let mut bytes = b"# Sphinx inventory version 2\n# Project: Demo\n# Version: 2.0\n\
            # The remainder of this file is compressed using zlib.\n"
            .to_vec();
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(body.as_bytes()).unwrap();
        bytes.extend(encoder.finish().unwrap());
        bytes
    }

    fn entry(name: &str, entry_type: &str, priority: i32, uri: &str) -> InventoryEntry {
        InventoryEntry {
            name: name.to_string(),
            entry_type: EntryType::new(entry_type).unwrap(),
            priority,
            uri: uri.to_string(),
            display_name: None,
        }
    }

    #[test]
    fn test_read_inventory_loads_a_file_written_by_real_sphinx() {
        // Given — the checked-in `sphinx-build` 9.1.0 output
        let bytes = include_bytes!("../testdata/sphinx-9.1.0.inv");

        // When
        let read = read_inventory(bytes).unwrap();

        // Then
        assert!(read.malformed_lines.is_empty());
        let inventory = read.inventory;
        assert_eq!(inventory.project, "Fixture");
        assert_eq!(inventory.version, "1.0");
        assert_eq!(inventory.entries.len(), 17);
        assert!(
            inventory
                .entries
                .contains(&entry("pkg", "py:module", 0, "api.html#module-pkg"))
        );
        assert!(inventory.entries.contains(&entry(
            "build step",
            "std:term",
            -1,
            "index.html#term-build-step"
        )));
        let intro = inventory
            .entries
            .iter()
            .find(|entry| entry.name == "intro")
            .unwrap();
        assert_eq!(intro.uri, "index.html#intro");
        assert_eq!(intro.display_name.as_deref(), Some("Fixture Project"));
    }

    #[test]
    fn test_read_inventory_expands_a_trailing_dollar_to_the_name() {
        // Given
        let bytes = inventory_file("pkg.Greeter py:class 1 api.html#$ -\n");

        // When
        let read = read_inventory(&bytes).unwrap();

        // Then
        assert_eq!(read.inventory.entries[0].uri, "api.html#pkg.Greeter");
    }

    #[test]
    fn test_read_inventory_reads_a_dash_display_name_as_none() {
        // Given
        let bytes = inventory_file("pkg py:module 0 api.html#module-$ -\n");

        // When
        let read = read_inventory(&bytes).unwrap();

        // Then
        assert_eq!(read.inventory.entries[0].display_name, None);
    }

    #[test]
    fn test_read_inventory_keeps_a_display_name_with_spaces() {
        // Given
        let bytes = inventory_file("index std:doc -1 index.html Fixture Project\n");

        // When
        let read = read_inventory(&bytes).unwrap();

        // Then
        assert_eq!(
            read.inventory.entries[0].display_name.as_deref(),
            Some("Fixture Project")
        );
    }

    #[test]
    fn test_read_inventory_accepts_an_empty_uri() {
        // Given — Sphinx's pattern allows an empty location
        let bytes = inventory_file("index std:doc -1  Title\n");

        // When
        let read = read_inventory(&bytes).unwrap();

        // Then
        assert_eq!(read.inventory.entries[0].uri, "");
        assert_eq!(
            read.inventory.entries[0].display_name.as_deref(),
            Some("Title")
        );
    }

    #[test]
    fn test_read_inventory_reports_and_skips_a_malformed_line() {
        // Given
        let bytes = inventory_file("good std:label -1 a.html#good -\ngarbage\n");

        // When
        let read = read_inventory(&bytes).unwrap();

        // Then
        assert_eq!(read.inventory.entries.len(), 1);
        assert_eq!(read.malformed_lines.len(), 1);
        assert_eq!(read.malformed_lines[0].line, 6);
        assert_eq!(read.malformed_lines[0].text, "garbage");
    }

    #[test]
    fn test_read_inventory_reports_a_type_without_a_colon() {
        // Given
        let bytes = inventory_file("good label -1 a.html#good -\n");

        // When
        let read = read_inventory(&bytes).unwrap();

        // Then
        assert!(read.inventory.entries.is_empty());
        assert_eq!(read.malformed_lines[0].reason, "type is not `domain:role`");
    }

    #[test]
    fn test_read_inventory_refuses_version_1_by_name() {
        // Given
        let bytes = b"# Sphinx inventory version 1\n# Project: Old\n# Version: 0.1\n";

        // When
        let result = read_inventory(bytes);

        // Then
        assert_eq!(result, Err(InventoryError::UnsupportedVersion1));
    }

    #[test]
    fn test_read_inventory_refuses_a_file_that_is_no_inventory() {
        // Given
        let bytes = b"<html>not found</html>\n";

        // When
        let result = read_inventory(bytes);

        // Then
        assert_eq!(result, Err(InventoryError::NotAnInventory));
    }

    #[test]
    fn test_read_inventory_refuses_a_missing_project_line() {
        // Given
        let bytes = b"# Sphinx inventory version 2\n# Version: 1\n";

        // When
        let result = read_inventory(bytes);

        // Then
        assert_eq!(
            result,
            Err(InventoryError::MalformedHeader {
                expected: "`# Project:`"
            })
        );
    }

    #[test]
    fn test_read_inventory_refuses_an_uncompressed_body() {
        // Given
        let bytes = b"# Sphinx inventory version 2\n# Project: P\n# Version: 1\n\
            # The remainder of this file is plain.\n";

        // When
        let result = read_inventory(bytes);

        // Then
        assert_eq!(
            result,
            Err(InventoryError::MalformedHeader {
                expected: "compression notice"
            })
        );
    }

    #[test]
    fn test_read_inventory_refuses_a_body_that_is_not_zlib() {
        // Given
        let mut bytes = b"# Sphinx inventory version 2\n# Project: P\n# Version: 1\n\
            # The remainder of this file is compressed using zlib.\n"
            .to_vec();
        bytes.extend(b"definitely not zlib");

        // When
        let result = read_inventory(&bytes);

        // Then
        assert!(matches!(result, Err(InventoryError::Decompression(_))));
    }

    #[test]
    fn test_take_header_line_needs_a_newline() {
        // Given
        let mut rest: &[u8] = b"no newline";

        // When / Then
        assert_eq!(take_header_line(&mut rest), None);
    }

    #[test]
    fn test_header_value_strips_the_prefix() {
        // Given
        let mut rest: &[u8] = b"# Project: My Docs  \nnext";

        // When
        let value = header_value(&mut rest, "# Project: ", "`# Project:`");

        // Then
        assert_eq!(value, Ok("My Docs".to_string()));
        assert_eq!(rest, b"next");
    }

    #[test]
    fn test_parse_entry_line_refuses_an_out_of_range_priority() {
        // Given
        let line = "name std:label 99999999999 a.html -";

        // When
        let result = parse_entry_line(line);

        // Then
        assert_eq!(result, Err("priority is out of range"));
    }
}

//! The document-level field list Sphinx calls file-wide metadata.
//!
//! Deliberately *not* general field-list support, which is its own unimplemented
//! feature: only a contiguous run of `:name: value` lines at the very top of a
//! document is read, and only as raw strings. That is exactly where Sphinx
//! requires `:orphan:`, the one field that currently means anything, so this is
//! the minimum that makes `:orphan:` work rather than a half-built version of a
//! construct that deserves a proper implementation.
//!
//! The lines are *consumed*, so they no longer reach the block parser and no
//! longer render as a stray paragraph — which is what they did before.

use std::collections::BTreeMap;

/// Whether `name` is a well-formed field name, by docutils' rule: non-empty,
/// and neither starting nor ending with a space.
fn is_field_name(name: &str) -> bool {
    !name.is_empty() && !name.starts_with(' ') && !name.ends_with(' ')
}

/// Splits a document's leading metadata field list off its content, returning
/// the fields and the number of lines they occupied.
///
/// Leading blank lines are allowed before the block (and counted), since an
/// author may well start a file with one. A comment, a directive or any prose
/// ends the block: metadata must come first or not at all.
pub(super) fn split_document_metadata(lines: &[&str]) -> (BTreeMap<String, String>, usize) {
    let mut fields = BTreeMap::new();

    // Skip blank lines ahead of the block, but only keep them if a field
    // actually follows — otherwise the document simply starts with blanks.
    let first_content = lines.iter().position(|line| !line.trim().is_empty());
    let Some(start) = first_content else {
        return (fields, 0);
    };

    let mut index = start;
    while index < lines.len() {
        let line = lines[index];
        // An indented line is a field's continuation or something else
        // entirely; either way this simple reader stops rather than guessing.
        if line.starts_with(char::is_whitespace) {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            break;
        }
        let Some(rest) = trimmed.strip_prefix(':') else {
            break;
        };
        let Some((name, value)) = rest.split_once(':') else {
            break;
        };
        // docutils' field marker requires the closing colon to be followed by
        // whitespace or the end of the line, and the name itself to be
        // non-empty and not to start or end with a space. Without that rule an
        // inline role opening a document — `:func:`spawn`` — reads as a field
        // named `func`, and the paragraph disappears.
        if !is_field_name(name) || !(value.is_empty() || value.starts_with(' ')) {
            break;
        }
        fields.insert(name.to_string(), value.trim().to_string());
        index += 1;
    }

    if fields.is_empty() {
        return (BTreeMap::new(), 0);
    }
    (fields, index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reads_a_leading_field() {
        // Given
        let lines = vec![":orphan:", "", "Title", "====="];

        // When
        let (fields, consumed) = split_document_metadata(&lines);

        // Then
        assert_eq!(fields.get("orphan"), Some(&String::new()));
        assert_eq!(consumed, 1);
    }

    #[test]
    fn test_reads_a_field_with_a_value() {
        // Given
        let lines = vec![":tocdepth: 2", "", "Title"];

        // When
        let (fields, consumed) = split_document_metadata(&lines);

        // Then
        assert_eq!(fields.get("tocdepth"), Some(&"2".to_string()));
        assert_eq!(consumed, 1);
    }

    #[test]
    fn test_reads_several_consecutive_fields() {
        // Given
        let lines = vec![":orphan:", ":tocdepth: 2", "", "Title"];

        // When
        let (fields, consumed) = split_document_metadata(&lines);

        // Then
        assert_eq!(fields.len(), 2);
        assert_eq!(consumed, 2);
    }

    #[test]
    fn test_allows_blank_lines_before_the_block() {
        // Given
        let lines = vec!["", "", ":orphan:", "", "Title"];

        // When
        let (fields, consumed) = split_document_metadata(&lines);

        // Then — the blanks are consumed with the block.
        assert!(fields.contains_key("orphan"));
        assert_eq!(consumed, 3);
    }

    #[test]
    fn test_ignores_a_field_list_after_content() {
        // Given — only a *leading* block is metadata, which is where Sphinx
        // requires `:orphan:`.
        let lines = vec!["Title", "=====", "", ":orphan:"];

        // When
        let (fields, consumed) = split_document_metadata(&lines);

        // Then
        assert!(fields.is_empty());
        assert_eq!(consumed, 0);
    }

    #[test]
    fn test_ignores_a_document_with_no_field_list() {
        // Given
        let lines = vec!["Title", "====="];

        // When
        let (fields, consumed) = split_document_metadata(&lines);

        // Then
        assert!(fields.is_empty());
        assert_eq!(consumed, 0);
    }

    #[test]
    fn test_stops_at_a_directive() {
        // Given — `.. note::` is not a field, so nothing here is metadata.
        let lines = vec![".. note::", "", "   Hi"];

        // When
        let (fields, consumed) = split_document_metadata(&lines);

        // Then
        assert!(fields.is_empty());
        assert_eq!(consumed, 0);
    }

    #[test]
    fn test_stops_at_an_indented_line() {
        // Given — an indented `:x:` is part of some other construct.
        let lines = vec!["   :orphan:", "", "Title"];

        // When
        let (fields, consumed) = split_document_metadata(&lines);

        // Then
        assert!(fields.is_empty());
        assert_eq!(consumed, 0);
    }

    #[test]
    fn test_ignores_a_line_missing_its_closing_colon() {
        // Given
        let lines = vec![":oops", "", "Title"];

        // When
        let (fields, consumed) = split_document_metadata(&lines);

        // Then
        assert!(fields.is_empty());
        assert_eq!(consumed, 0);
    }

    #[test]
    fn test_ignores_a_role_opening_the_document() {
        // Given — `:func:`spawn`` is an inline role, not a field: docutils
        // requires whitespace or end-of-line after a field's closing colon.
        let lines = vec![r":func:`spawn`", "", "More prose."];

        // When
        let (fields, consumed) = split_document_metadata(&lines);

        // Then
        assert!(fields.is_empty(), "{fields:?}");
        assert_eq!(consumed, 0);
    }

    #[test]
    fn test_ignores_a_name_with_a_leading_space() {
        // Given
        let lines = vec![": oops: value"];

        // When
        let (fields, _) = split_document_metadata(&lines);

        // Then
        assert!(fields.is_empty());
    }

    #[test]
    fn test_handles_an_empty_document() {
        // Given
        let lines: Vec<&str> = vec![];

        // When
        let (fields, consumed) = split_document_metadata(&lines);

        // Then
        assert!(fields.is_empty());
        assert_eq!(consumed, 0);
    }

    #[test]
    fn test_handles_an_all_blank_document() {
        // Given
        let lines = vec!["", "  ", ""];

        // When
        let (fields, consumed) = split_document_metadata(&lines);

        // Then
        assert!(fields.is_empty());
        assert_eq!(consumed, 0);
    }
}

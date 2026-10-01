//! The `:index:` role, which adds general-index entries pointing at the
//! place it is written and shows its text there, unlinked.
//!
//! Its entries are written in the `.. index::` grammar, and are parsed by
//! that directive's own functions so the two cannot read an entry
//! differently. What the role adds is the choice of *which* grammar: with an
//! explicit title the target is one whole `.. index::` line, without one it
//! is a single `single:` value, so `a, b` is one term rather than two —
//! Sphinx's `IndexRole`.

use rinx_ast::{IndexEntry, IndexEntryType, InlineNode, InvalidIndexEntry, RoleRefusal};

use crate::directives::index_directive::{parse_index_line, parse_typed_entry, strip_main_prefix};
use crate::explicit_title::split_optional_title;
use crate::inline::escapes::unescape;
use crate::inline::regexes::INDEX_ROLE_REGEX;

/// Builds the `InlineNode` for a matched `:index:` role.
///
/// Title and target are unescaped before they are read, since Sphinx's
/// `ReferenceRole` hands both over unescaped. An entry its type cannot split
/// becomes an [`InlineNode::RefusedRole`], which the whole-document pass
/// reports at this role's span and lowers to the title with no entries —
/// Sphinx warns and still shows the text.
pub(crate) fn handle_index_match(m_str: &str) -> InlineNode {
    let caps = INDEX_ROLE_REGEX.captures(m_str).unwrap();
    let (display, target) = split_optional_title(&caps["content"]);
    let (title, written) = IndexRoleTarget::read(display.as_deref(), &target);
    match written.entries() {
        Ok(entries) => InlineNode::IndexReference {
            title,
            entries,
            index_id: String::new(),
            span: None,
        },
        Err(entry) => InlineNode::RefusedRole {
            text: m_str.to_string(),
            refusal: RoleRefusal::IndexEntry { title, entry },
            span: None,
        },
    }
}

/// What an `:index:` role's target is read as.
enum IndexRoleTarget {
    /// The explicit-title form's target: one whole `.. index::` line, typed
    /// or a comma-separated list of bare terms.
    Line(String),
    /// The bare form: one `single:` value, commas included, `!` already
    /// stripped into `main`.
    Single { value: String, main: bool },
}

impl IndexRoleTarget {
    /// Reads a role's still-escaped explicit `display` title, if one was
    /// written, and `target` into the text the role shows and the target its
    /// entries come from.
    fn read(display: Option<&str>, target: &str) -> (String, Self) {
        if let Some(display) = display {
            return (unescape(display), Self::Line(unescape(target)));
        }
        let target = unescape(target);
        // Sphinx strips the `!` from the shown text without trimming what
        // follows; the entry's value is trimmed when it is split.
        let (title, main) = target
            .strip_prefix('!')
            .map_or((target.as_str(), false), |rest| (rest, true));
        (
            title.to_string(),
            Self::Single {
                value: title.to_string(),
                main,
            },
        )
    }

    /// The entries the target makes.
    fn entries(&self) -> Result<Vec<IndexEntry>, InvalidIndexEntry> {
        match self {
            Self::Line(line) => {
                // `parse_index_line` reads a blank line as no entries, which
                // is right for a directive's body but not for a role whose
                // target is all it has.
                let (rest, _) = strip_main_prefix(line.trim());
                if rest.trim().is_empty() {
                    return Err(InvalidIndexEntry {
                        entry_type: IndexEntryType::Single,
                        value: rest.trim().to_string(),
                    });
                }
                parse_index_line(line)
            }
            Self::Single { value, main } => parse_typed_entry(IndexEntryType::Single, value, *main),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn term(primary: &str, subentry: Option<&str>, main: bool) -> IndexEntry {
        IndexEntry::Term {
            primary: primary.to_string(),
            subentry: subentry.map(str::to_string),
            main,
        }
    }

    fn reference(title: &str, entries: Vec<IndexEntry>) -> InlineNode {
        InlineNode::IndexReference {
            title: title.to_string(),
            entries,
            index_id: String::new(),
            span: None,
        }
    }

    #[test]
    fn test_handle_index_match_indexes_a_bare_target_as_one_single_entry() {
        // Given / When
        let node = handle_index_match(":index:`execution`");

        // Then
        assert_eq!(
            node,
            reference("execution", vec![term("execution", None, false)])
        );
    }

    #[test]
    fn test_handle_index_match_splits_a_bare_target_into_a_subentry() {
        // Given / When
        let node = handle_index_match(":index:`execution; context`");

        // Then
        assert_eq!(
            node,
            reference(
                "execution; context",
                vec![term("execution", Some("context"), false)]
            )
        );
    }

    #[test]
    fn test_handle_index_match_does_not_split_a_bare_target_on_commas() {
        // Given / When
        let node = handle_index_match(":index:`BNF, grammar`");

        // Then
        assert_eq!(
            node,
            reference("BNF, grammar", vec![term("BNF, grammar", None, false)])
        );
    }

    #[test]
    fn test_handle_index_match_strips_the_main_marker_from_a_bare_target() {
        // Given / When
        let node = handle_index_match(":index:`!execution`");

        // Then
        assert_eq!(
            node,
            reference("execution", vec![term("execution", None, true)])
        );
    }

    #[test]
    fn test_handle_index_match_reads_an_explicit_target_as_an_index_line() {
        // Given / When
        let node = handle_index_match(":index:`loop statements <pair: loop; statement>`");

        // Then
        assert_eq!(
            node,
            reference(
                "loop statements",
                vec![
                    term("loop", Some("statement"), false),
                    term("statement", Some("loop"), false),
                ]
            )
        );
    }

    #[test]
    fn test_handle_index_match_splits_an_explicit_bare_target_on_commas() {
        // Given / When
        let node = handle_index_match(":index:`grammar <BNF, !grammar>`");

        // Then — the line grammar's comma shorthand applies
        let InlineNode::IndexReference { entries, .. } = node else {
            panic!("Expected IndexReference, got {node:?}")
        };
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn test_handle_index_match_keeps_a_see_entry() {
        // Given / When
        let node = handle_index_match(":index:`jumping <see: goto; jump>`");

        // Then
        assert_eq!(
            node,
            reference(
                "jumping",
                vec![IndexEntry::See {
                    entry: "goto".to_string(),
                    target: "jump".to_string(),
                }]
            )
        );
    }

    #[test]
    fn test_handle_index_match_refuses_a_malformed_explicit_entry() {
        // Given
        let m_str = ":index:`loops <pair: loop>`";

        // When
        let node = handle_index_match(m_str);

        // Then
        assert_eq!(
            node,
            InlineNode::RefusedRole {
                text: m_str.to_string(),
                refusal: RoleRefusal::IndexEntry {
                    title: "loops".to_string(),
                    entry: InvalidIndexEntry {
                        entry_type: IndexEntryType::Pair,
                        value: "loop".to_string(),
                    },
                },
                span: None,
            }
        );
    }

    #[test]
    fn test_handle_index_match_refuses_a_bare_main_marker_alone() {
        // Given / When
        let node = handle_index_match(":index:`!`");

        // Then
        assert!(
            matches!(
                &node,
                InlineNode::RefusedRole {
                    refusal: RoleRefusal::IndexEntry { title, entry },
                    ..
                } if title.is_empty() && entry.entry_type == IndexEntryType::Single
            ),
            "{node:?}"
        );
    }

    #[test]
    fn test_index_role_target_read_keeps_an_explicit_title_and_target_line() {
        // Given / When
        let (title, target) = IndexRoleTarget::read(Some("loops"), "!pair: a; b");

        // Then
        assert_eq!(title, "loops");
        assert!(matches!(target, IndexRoleTarget::Line(line) if line == "!pair: a; b"));
    }

    #[test]
    fn test_index_role_target_read_takes_the_main_marker_off_a_bare_target() {
        // Given / When
        let (title, target) = IndexRoleTarget::read(None, "!loop");

        // Then
        assert_eq!(title, "loop");
        assert!(matches!(
            target,
            IndexRoleTarget::Single { value, main: true } if value == "loop"
        ));
    }

    #[test]
    fn test_index_role_target_refuses_an_empty_explicit_target() {
        // Given
        let target = IndexRoleTarget::Line(" ! ".to_string());

        // When
        let result = target.entries();

        // Then
        assert_eq!(
            result,
            Err(InvalidIndexEntry {
                entry_type: IndexEntryType::Single,
                value: String::new(),
            })
        );
    }

    #[test]
    fn test_index_role_target_reads_a_single_value_with_its_commas() {
        // Given
        let target = IndexRoleTarget::Single {
            value: "a, b".to_string(),
            main: true,
        };

        // When
        let result = target.entries();

        // Then
        assert_eq!(result, Ok(vec![term("a, b", None, true)]));
    }
}

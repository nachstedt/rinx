//! Emphasis / strong / inline-literal recognition: finding where a `*`,
//! `**` or ``` `` ``` span may validly open and close, per docutils'
//! start-string and end-string context rules.

use rinx_ast::InlineNode;

use super::escapes::is_escaped_at;
use super::punctuation::{can_follow_end_string, can_precede_start_string};
use super::typography::apply_smart_typography;

/// Returns every absolute byte position in `full_text` at or after
/// `search_start` where `marker` occurs and could validly serve as a
/// *closing* marker for some inline-markup span: not preceded by whitespace,
/// followed by whitespace/allowed punctuation (or end of text), and (unless
/// `is_literal`) not itself escaped by a preceding backslash.
///
/// Whether an occurrence of `marker` is valid as a closing marker depends
/// only on the text around that occurrence, never on where a candidate
/// opening marker started searching from. Computing this table once per
/// `marker` (here, once per call to [`find_inline_markup`]) instead of
/// rescanning it from scratch for every failed candidate opening marker is
/// what keeps inline-markup parsing from being quadratic in input size.
fn find_valid_close_positions(
    full_text: &str,
    search_start: usize,
    marker: &str,
    is_literal: bool,
) -> Vec<usize> {
    let marker_len = marker.len();
    let mut positions = Vec::new();
    let mut search_pos = search_start;

    while let Some(rel_pos) = full_text[search_pos..].find(marker) {
        let abs_pos = search_pos + rel_pos;

        // End context check
        let last_inner = full_text[..abs_pos].chars().next_back();
        if last_inner.is_some_and(char::is_whitespace) {
            search_pos = abs_pos + 1;
            continue;
        }

        // Check character after end marker.
        let after_end = abs_pos + marker_len;
        if after_end < full_text.len() {
            let next_char = full_text[after_end..].chars().next().unwrap();
            if !can_follow_end_string(next_char) {
                search_pos = abs_pos + 1;
                continue;
            }
        }

        // An escaped end marker closes nothing. Literals are exempt, mirroring
        // docutils using `non_whitespace_before` for them where emphasis and
        // strong use `non_whitespace_escape_before`.
        if !is_literal && is_escaped_at(full_text, abs_pos) {
            search_pos = abs_pos + 1;
            continue;
        }

        positions.push(abs_pos);
        search_pos = abs_pos + 1;
    }

    positions
}

pub(super) fn find_inline_markup(
    full_text: &str,
    start_offset: usize,
) -> Option<(usize, usize, InlineNode)> {
    let text = &full_text[start_offset..];
    let mut best_match: Option<(usize, usize, InlineNode)> = None;

    let literal_close_positions = find_valid_close_positions(full_text, start_offset, "``", true);
    let strong_close_positions = find_valid_close_positions(full_text, start_offset, "**", false);
    let emphasis_close_positions = find_valid_close_positions(full_text, start_offset, "*", false);
    let substitution_close_positions =
        find_valid_close_positions(full_text, start_offset, "|", false);

    for (i, _) in text.char_indices() {
        let abs_i = start_offset + i;

        // An escaped character never opens markup.
        if is_escaped_at(full_text, abs_i) {
            continue;
        }

        // Try Inline Literal first (``)
        if text[i..].starts_with("``")
            && let Some((end_pos, content)) =
                try_match_inline(full_text, abs_i, 2, &literal_close_positions)
        {
            let m = (i, i + (end_pos - abs_i), InlineNode::Literal(content));
            if best_match.is_none() || m.0 < best_match.as_ref().unwrap().0 {
                best_match = Some(m);
                break; // Found the earliest match
            }
        }

        // Try Strong Emphasis next (**)
        if text[i..].starts_with("**")
            && let Some((end_pos, content)) =
                try_match_inline(full_text, abs_i, 2, &strong_close_positions)
        {
            let m = (
                i,
                i + (end_pos - abs_i),
                InlineNode::Strong(apply_smart_typography(&content)),
            );
            if best_match.is_none() || m.0 < best_match.as_ref().unwrap().0 {
                best_match = Some(m);
                break; // Found the earliest match
            }
        }

        // Try Emphasis (*)
        if text[i..].starts_with('*')
            && !text[i..].starts_with("**")
            && let Some((end_pos, content)) =
                try_match_inline(full_text, abs_i, 1, &emphasis_close_positions)
        {
            let m = (
                i,
                i + (end_pos - abs_i),
                InlineNode::Emphasis(apply_smart_typography(&content)),
            );
            if best_match.is_none() || m.0 < best_match.as_ref().unwrap().0 {
                best_match = Some(m);
                break; // Found the earliest match
            }
        }

        // Try a substitution reference (|name|) — the same start-string/
        // end-string context rules as every other marker here, since
        // docutils recognizes `|...|` as inline markup like any other. The
        // captured name is kept verbatim rather than run through
        // `apply_smart_typography`: it is a lookup key, not prose.
        if text[i..].starts_with('|')
            && let Some((end_pos, name)) =
                try_match_inline(full_text, abs_i, 1, &substitution_close_positions)
        {
            let m = (
                i,
                i + (end_pos - abs_i),
                InlineNode::SubstitutionReference { name, span: None },
            );
            if best_match.is_none() || m.0 < best_match.as_ref().unwrap().0 {
                best_match = Some(m);
                break; // Found the earliest match
            }
        }
    }

    best_match
}

pub(super) fn try_match_inline(
    full_text: &str,
    start_pos: usize,
    marker_len: usize,
    close_positions: &[usize],
) -> Option<(usize, String)> {
    // Start context check
    if start_pos > 0 {
        let prev_char = full_text[..start_pos].chars().next_back().unwrap();
        if !can_precede_start_string(prev_char) {
            return None;
        }
    }

    let after_start = start_pos + marker_len;
    if after_start >= full_text.len() {
        return None;
    }

    let first_inner = full_text[after_start..].chars().next().unwrap();
    if first_inner.is_whitespace() {
        return None;
    }

    // The closing search only ever considers positions at or after
    // `search_pos`, which is always strictly after `after_start` (it skips
    // past the first inner character), so any matched close position yields
    // non-empty content — no separate empty-content check is needed.
    let search_pos = after_start + first_inner.len_utf8();
    let idx = close_positions.partition_point(|&p| p < search_pos);
    let &abs_end_pos = close_positions.get(idx)?;
    let content = full_text[after_start..abs_end_pos].to_string();

    Some((abs_end_pos + marker_len, content))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Escapes `raw` the way [`crate::inline::text::parse_inline_text`] does
    /// before any of the helpers below see it, so a unit test exercises the
    /// form those helpers are actually handed rather than a raw backslash
    /// they never meet.
    fn escaped(raw: &str) -> String {
        crate::inline::escapes::EscapedText::new(raw)
            .as_str()
            .to_string()
    }

    #[test]
    fn test_try_match_inline_multibyte_first_inner() {
        // Given
        let input = "*π*";
        let close_positions = find_valid_close_positions(input, 0, "*", false);
        // When
        let res = try_match_inline(input, 0, 1, &close_positions);
        // Then
        assert_eq!(res, Some((4, "π".to_string())));
    }
    #[test]
    fn test_try_match_inline_rejects_space_after_open_marker() {
        // Given: space immediately after marker is not valid markup
        let input = "* not emphasis *";
        let close_positions = find_valid_close_positions(input, 0, "*", false);
        // When
        let res = try_match_inline(input, 0, 1, &close_positions);
        // Then
        assert_eq!(res, None);
    }
    #[test]
    fn test_try_match_inline_requires_valid_end_boundary() {
        // Given: no valid end boundary
        let input = "*nospace*x";
        let close_positions = find_valid_close_positions(input, 0, "*", false);
        // When
        let res = try_match_inline(input, 0, 1, &close_positions);
        // Then
        assert_eq!(res, None);
    }
    #[test]
    fn test_find_valid_close_positions_finds_marker_with_valid_boundaries() {
        // Given: a lone valid "*" close candidate, not preceded by whitespace,
        // followed by whitespace
        let input = "*word* rest";
        // When
        let positions = find_valid_close_positions(input, 0, "*", false);
        // Then
        assert_eq!(positions, vec![5]);
    }
    #[test]
    fn test_find_valid_close_positions_rejects_marker_preceded_by_whitespace() {
        // Given: the marker is preceded by a space, so it can't close content
        let input = "*word * rest*";
        // When
        let positions = find_valid_close_positions(input, 0, "*", false);
        // Then: only the final "*" (preceded by "t") qualifies
        assert_eq!(positions, vec![12]);
    }
    #[test]
    fn test_find_valid_close_positions_rejects_marker_followed_by_disallowed_char() {
        // Given: the marker is immediately followed by a word character
        let input = "*word*x more *word* here";
        // When
        let positions = find_valid_close_positions(input, 0, "*", false);
        // Then: only the second "*word*" pair's closer qualifies
        assert_eq!(positions, vec![18]);
    }
    #[test]
    fn test_find_valid_close_positions_rejects_escaped_marker_unless_literal() {
        // Given: an escaped "*" shouldn't count as a valid closer for
        // emphasis, but escaping is irrelevant for inline literals
        let input = &escaped(r"word\* more");
        // When
        let emphasis_positions = find_valid_close_positions(input, 0, "*", false);
        let literal_positions = find_valid_close_positions(input, 0, "*", true);
        // Then
        assert_eq!(emphasis_positions, Vec::<usize>::new());
        assert_eq!(literal_positions, vec![5]);
    }
    #[test]
    fn test_find_valid_close_positions_respects_search_start() {
        // Given: two valid closers, one before and one at/after search_start
        let input = "*a* *b*";
        // When
        let positions = find_valid_close_positions(input, 4, "*", false);
        // Then: the closer at byte 2 (before search_start) is excluded
        assert_eq!(positions, vec![6]);
    }
}

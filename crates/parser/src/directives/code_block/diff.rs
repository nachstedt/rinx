//! A unified diff between two files, for `.. literalinclude::`'s `:diff:`.
//!
//! Sphinx renders that option by diffing the two files at build time and
//! showing the result as a `diff`-highlighted block, so the work happens while
//! parsing here too — by the time the AST exists, a `:diff:` block is an
//! ordinary code block whose content happens to be a patch.
//!
//! Written rather than pulled from a crate because the requirement is narrow:
//! documentation examples, two files of a few hundred lines, output that only
//! has to be readable and highlightable. A dependency would have to earn the
//! `Cargo.bazel.lock` repin that adding one costs.

/// Lines of context shown either side of a change, matching `diff -u`.
const CONTEXT: usize = 3;

/// Above this many lines, the quadratic LCS below would cost more memory than
/// a documentation build should spend, so the two files are reported as one
/// wholesale replacement instead.
///
/// A deliberate ceiling rather than a smarter algorithm: `:diff:` exists to
/// show a reader a small edit, and a diff of two five-thousand-line files is
/// not something anybody reads on a documentation page.
const MAX_LINES: usize = 3_000;

/// A unified diff turning `old` into `new`, with `diff -u`'s header naming the
/// two files.
///
/// Returns an empty string when the two are identical, which the caller treats
/// as a directive with nothing to show.
pub(super) fn unified_diff(old_name: &str, old: &str, new_name: &str, new: &str) -> String {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();
    if old_lines == new_lines {
        return String::new();
    }

    let edits = if old_lines.len() > MAX_LINES || new_lines.len() > MAX_LINES {
        wholesale_replacement(&old_lines, &new_lines)
    } else {
        edit_script(&old_lines, &new_lines)
    };

    let mut out = format!("--- {old_name}\n+++ {new_name}\n");
    for hunk in hunks(&edits) {
        out.push_str(&render_hunk(&edits, &hunk, &old_lines, &new_lines));
    }
    out
}

/// One line's fate in the edit script.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Edit {
    /// Present in both files, at these 0-based line indices.
    Keep(usize, usize),
    /// Only in the old file.
    Remove(usize),
    /// Only in the new file.
    Add(usize),
}

/// Every line removed, then every line added — the answer for inputs too large
/// to diff line by line.
fn wholesale_replacement(old: &[&str], new: &[&str]) -> Vec<Edit> {
    (0..old.len())
        .map(Edit::Remove)
        .chain((0..new.len()).map(Edit::Add))
        .collect()
}

/// The edit script turning `old` into `new`, longest-common-subsequence first
/// so that unchanged lines stay unchanged.
fn edit_script(old: &[&str], new: &[&str]) -> Vec<Edit> {
    let table = lcs_table(old, new);
    let mut edits = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < old.len() && j < new.len() {
        if old[i] == new[j] {
            edits.push(Edit::Keep(i, j));
            i += 1;
            j += 1;
        } else if table[i + 1][j] >= table[i][j + 1] {
            edits.push(Edit::Remove(i));
            i += 1;
        } else {
            edits.push(Edit::Add(j));
            j += 1;
        }
    }
    edits.extend((i..old.len()).map(Edit::Remove));
    edits.extend((j..new.len()).map(Edit::Add));
    edits
}

/// `table[i][j]` is the length of the longest common subsequence of
/// `old[i..]` and `new[j..]`, so the walk above can always take the branch
/// that keeps more lines.
fn lcs_table(old: &[&str], new: &[&str]) -> Vec<Vec<usize>> {
    let mut table = vec![vec![0; new.len() + 1]; old.len() + 1];
    for i in (0..old.len()).rev() {
        for j in (0..new.len()).rev() {
            table[i][j] = if old[i] == new[j] {
                table[i + 1][j + 1] + 1
            } else {
                table[i + 1][j].max(table[i][j + 1])
            };
        }
    }
    table
}

/// A run of the edit script worth printing: the changed edits plus up to
/// [`CONTEXT`] kept lines either side, with neighbouring runs merged.
#[derive(Debug, PartialEq, Eq)]
struct Hunk {
    /// Indices into the edit script, `start` inclusive and `end` exclusive.
    start: usize,
    end: usize,
}

/// The hunks of `edits`, in order. Empty when nothing changed.
fn hunks(edits: &[Edit]) -> Vec<Hunk> {
    let changed: Vec<usize> = edits
        .iter()
        .enumerate()
        .filter(|(_, edit)| !matches!(edit, Edit::Keep(_, _)))
        .map(|(index, _)| index)
        .collect();
    let mut hunks: Vec<Hunk> = Vec::new();
    for index in changed {
        let start = index.saturating_sub(CONTEXT);
        let end = (index + CONTEXT + 1).min(edits.len());
        match hunks.last_mut() {
            // Runs that touch or overlap become one hunk, exactly as `diff -u`
            // merges nearby changes rather than repeating their shared context.
            Some(last) if start <= last.end => last.end = last.end.max(end),
            _ => hunks.push(Hunk { start, end }),
        }
    }
    hunks
}

/// One hunk as `@@ -a,b +c,d @@` followed by its lines.
fn render_hunk(edits: &[Edit], hunk: &Hunk, old: &[&str], new: &[&str]) -> String {
    let slice = &edits[hunk.start..hunk.end];
    let (old_start, old_count) = old_range(slice);
    let (new_start, new_count) = new_range(slice);

    let mut out = format!("@@ -{old_start},{old_count} +{new_start},{new_count} @@\n");
    for edit in slice {
        let (marker, text) = match edit {
            Edit::Keep(i, _) => (' ', old[*i]),
            Edit::Remove(i) => ('-', old[*i]),
            Edit::Add(j) => ('+', new[*j]),
        };
        out.push(marker);
        out.push_str(text);
        out.push('\n');
    }
    out
}

/// The 1-based start line and length of a hunk's old-file side.
fn old_range(slice: &[Edit]) -> (usize, usize) {
    let indices: Vec<usize> = slice
        .iter()
        .filter_map(|edit| match edit {
            Edit::Keep(i, _) | Edit::Remove(i) => Some(*i),
            Edit::Add(_) => None,
        })
        .collect();
    range_of(&indices)
}

/// The 1-based start line and length of a hunk's new-file side.
fn new_range(slice: &[Edit]) -> (usize, usize) {
    let indices: Vec<usize> = slice
        .iter()
        .filter_map(|edit| match edit {
            Edit::Keep(_, j) | Edit::Add(j) => Some(*j),
            Edit::Remove(_) => None,
        })
        .collect();
    range_of(&indices)
}

/// `(start, count)` for a hunk side, where a side with no lines is reported as
/// starting at 0 — `diff -u`'s convention for "nothing here".
fn range_of(indices: &[usize]) -> (usize, usize) {
    match indices.first() {
        Some(first) => (first + 1, indices.len()),
        None => (0, 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt::Write;

    /// `count` lines named `line1`, `line2`, … each ending in a newline.
    fn numbered_lines(count: usize) -> String {
        (1..=count).fold(String::new(), |mut text, n| {
            let _ = writeln!(text, "line{n}");
            text
        })
    }

    #[test]
    fn test_identical_files_produce_no_diff() {
        // Given / When
        let diff = unified_diff("a.py", "x = 1\ny = 2", "b.py", "x = 1\ny = 2");

        // Then — the caller treats an empty result as nothing to show
        assert!(diff.is_empty(), "{diff}");
    }

    #[test]
    fn test_a_changed_line_shows_as_a_removal_and_an_addition() {
        // Given
        let old = "one\ntwo\nthree";
        let new = "one\nTWO\nthree";

        // When
        let diff = unified_diff("a.py", old, "b.py", new);

        // Then
        assert!(diff.contains("-two\n"), "{diff}");
        assert!(diff.contains("+TWO\n"), "{diff}");
        assert!(diff.contains(" one\n"), "{diff}");
        assert!(diff.contains(" three\n"), "{diff}");
    }

    #[test]
    fn test_the_header_names_both_files() {
        // Given / When
        let diff = unified_diff("before.py", "a", "after.py", "b");

        // Then
        assert!(diff.starts_with("--- before.py\n+++ after.py\n"), "{diff}");
    }

    #[test]
    fn test_an_added_line_is_marked_with_a_plus() {
        // Given / When
        let diff = unified_diff("a.py", "one\ntwo", "b.py", "one\nextra\ntwo");

        // Then
        assert!(diff.contains("+extra\n"), "{diff}");
        assert!(!diff.contains("-one"), "{diff}");
    }

    #[test]
    fn test_a_removed_line_is_marked_with_a_minus() {
        // Given / When
        let diff = unified_diff("a.py", "one\ngone\ntwo", "b.py", "one\ntwo");

        // Then
        assert!(diff.contains("-gone\n"), "{diff}");
    }

    #[test]
    fn test_a_hunk_header_names_both_sides_line_ranges() {
        // Given / When
        let diff = unified_diff("a.py", "one\ntwo", "b.py", "one\nTWO");

        // Then — 1-based, as `diff -u` writes them
        assert!(diff.contains("@@ -1,2 +1,2 @@"), "{diff}");
    }

    #[test]
    fn test_distant_changes_become_separate_hunks() {
        // Given two edits far enough apart that their context does not touch
        let old = numbered_lines(20);
        let new = old
            .replace("line2\n", "CHANGED2\n")
            .replace("line19\n", "CHANGED19\n");

        // When
        let diff = unified_diff("a.py", &old, "b.py", &new);

        // Then
        assert_eq!(diff.matches("@@ ").count(), 2, "{diff}");
    }

    #[test]
    fn test_nearby_changes_are_merged_into_one_hunk() {
        // Given two edits whose context overlaps
        let old = numbered_lines(20);
        let new = old
            .replace("line5\n", "CHANGED5\n")
            .replace("line6\n", "CHANGED6\n");

        // When
        let diff = unified_diff("a.py", &old, "b.py", &new);

        // Then — one hunk, not two with repeated context
        assert_eq!(diff.matches("@@ ").count(), 1, "{diff}");
    }

    #[test]
    fn test_context_is_limited_to_three_lines_either_side() {
        // Given one change in the middle of a long file
        let old = numbered_lines(20);
        let new = old.replace("line10\n", "CHANGED\n");

        // When
        let diff = unified_diff("a.py", &old, "b.py", &new);

        // Then — lines 7..13 appear, line 6 does not
        assert!(diff.contains(" line7\n"), "{diff}");
        assert!(diff.contains(" line13\n"), "{diff}");
        assert!(!diff.contains(" line6\n"), "{diff}");
    }

    #[test]
    fn test_unchanged_lines_are_kept_rather_than_rewritten() {
        // Given a change at one end only
        let old = "keep1\nkeep2\nkeep3\nold";
        let new = "keep1\nkeep2\nkeep3\nnew";

        // When
        let diff = unified_diff("a.py", old, "b.py", new);

        // Then — the LCS keeps the shared prefix rather than replacing it all
        let body: Vec<&str> = diff.lines().skip(3).collect();
        assert_eq!(
            body,
            vec![" keep1", " keep2", " keep3", "-old", "+new"],
            "{diff}"
        );
    }

    #[test]
    fn test_an_empty_old_file_shows_every_line_as_added() {
        // Given / When
        let diff = unified_diff("a.py", "", "b.py", "one\ntwo");

        // Then
        assert!(diff.contains("+one\n"), "{diff}");
        assert!(diff.contains("+two\n"), "{diff}");
        assert!(diff.contains("@@ -0,0 +1,2 @@"), "{diff}");
    }

    #[test]
    fn test_an_emptied_file_shows_every_line_as_removed() {
        // Given / When
        let diff = unified_diff("a.py", "one\ntwo", "b.py", "");

        // Then
        assert!(diff.contains("-one\n"), "{diff}");
        assert!(diff.contains("@@ -1,2 +0,0 @@"), "{diff}");
    }

    #[test]
    fn test_edit_script_keeps_every_common_line() {
        // Given
        let old = ["a", "b", "c"];
        let new = ["a", "x", "c"];

        // When
        let edits = edit_script(&old, &new);

        // Then — `a` and `c` survive as Keeps; only `b`/`x` change
        let kept = edits
            .iter()
            .filter(|e| matches!(e, Edit::Keep(_, _)))
            .count();
        assert_eq!(kept, 2);
    }

    #[test]
    fn test_hunks_of_an_unchanged_script_is_empty() {
        // Given
        let edits = vec![Edit::Keep(0, 0), Edit::Keep(1, 1)];

        // When / Then — nothing to print
        assert!(hunks(&edits).is_empty());
    }

    #[test]
    fn test_range_of_an_empty_side_starts_at_zero() {
        // Given / When — `diff -u`'s convention for "nothing on this side"
        let range = range_of(&[]);

        // Then
        assert_eq!(range, (0, 0));
    }

    #[test]
    fn test_range_of_is_one_based() {
        // Given / When
        let range = range_of(&[4, 5, 6]);

        // Then
        assert_eq!(range, (5, 3));
    }

    #[test]
    fn test_a_file_past_the_line_ceiling_is_reported_wholesale() {
        // Given a file larger than the quadratic table should ever be built for
        let old = numbered_lines(MAX_LINES + 1);
        let new = format!("changed\n{old}");

        // When
        let diff = unified_diff("a.py", &old, "b.py", &new);

        // Then — every line removed then re-added, rather than a hung build
        assert!(diff.contains("-line1\n"), "{}", &diff[..200]);
        assert!(diff.contains("+changed\n"), "{}", &diff[..200]);
    }
}

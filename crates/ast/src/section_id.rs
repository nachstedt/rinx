//! Deriving a heading's HTML `id` the way docutils does.
//!
//! This lives in `ast`, flat beside [`crate::object_naming`], for the same
//! reason that one does: two later phases need the *same* answer and must not
//! compute it separately. The analyzer records a section's id in the document
//! outline so a toctree entry can link to it; the renderer emits that id on
//! the `<h2>` the link lands on. If the two derivations ever drifted, every
//! section link in the site would break silently — the same hazard
//! [`rinx_scope`] exists to prevent for domain objects.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use unicode_normalization::UnicodeNormalization;

/// Characters docutils replaces with a two-letter ASCII expansion before
/// normalizing. Transcribed from docutils' `_non_id_translate_digraphs`
/// rather than re-derived, so the two cannot drift as either side changes.
const DIGRAPHS: [(char, &str); 5] = [
    ('\u{00df}', "sz"), // ligature sz
    ('\u{00e6}', "ae"), // ae
    ('\u{0153}', "oe"), // ligature oe
    ('\u{0238}', "db"), // db digraph
    ('\u{0239}', "qp"), // qp digraph
];

/// Characters docutils maps to a single ASCII letter before normalizing.
///
/// These are letters whose stroke, hook or curl is *not* a combining mark, so
/// NFKD alone would leave them non-ASCII and they would simply be dropped.
/// Transcribed from docutils' `_non_id_translate`.
const TRANSLATE: [(char, char); 33] = [
    ('\u{00f8}', 'o'), // o with stroke
    ('\u{0111}', 'd'), // d with stroke
    ('\u{0127}', 'h'), // h with stroke
    ('\u{0131}', 'i'), // dotless i
    ('\u{0142}', 'l'), // l with stroke
    ('\u{0167}', 't'), // t with stroke
    ('\u{0180}', 'b'), // b with stroke
    ('\u{0183}', 'b'), // b with topbar
    ('\u{0188}', 'c'), // c with hook
    ('\u{018c}', 'd'), // d with topbar
    ('\u{0192}', 'f'), // f with hook
    ('\u{0199}', 'k'), // k with hook
    ('\u{019a}', 'l'), // l with bar
    ('\u{019e}', 'n'), // n with long right leg
    ('\u{01a5}', 'p'), // p with hook
    ('\u{01ab}', 't'), // t with palatal hook
    ('\u{01ad}', 't'), // t with hook
    ('\u{01b4}', 'y'), // y with hook
    ('\u{01b6}', 'z'), // z with stroke
    ('\u{01e5}', 'g'), // g with stroke
    ('\u{0225}', 'z'), // z with hook
    ('\u{0234}', 'l'), // l with curl
    ('\u{0235}', 'n'), // n with curl
    ('\u{0236}', 't'), // t with curl
    ('\u{0237}', 'j'), // dotless j
    ('\u{023c}', 'c'), // c with stroke
    ('\u{023f}', 's'), // s with swash tail
    ('\u{0240}', 'z'), // z with swash tail
    ('\u{0247}', 'e'), // e with stroke
    ('\u{0249}', 'j'), // j with stroke
    ('\u{024b}', 'q'), // q with hook tail
    ('\u{024d}', 'r'), // r with stroke
    ('\u{024f}', 'y'), // y with stroke
];

/// The prefix docutils falls back to when a heading's text yields no usable
/// id at all (`"123"`, `"!!!"`), from its `auto_id_prefix` of `%` resolving
/// against the `section` tag name.
const EMPTY_FALLBACK_PREFIX: &str = "section";

/// A heading's HTML `id`, conforming to docutils' `[a-z](-?[a-z0-9]+)*`.
///
/// Opaque with a smart constructor, so an id that never went through
/// [`section_slug`] cannot reach a link or an index key. Its `Deserialize`
/// re-checks the invariant on load rather than trusting the file, matching
/// [`crate::HashedContent`]: a `.ast` or index file is a build artifact, and a
/// stale or hand-edited one should fail loudly rather than emit broken links.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub struct SectionId(String);

impl SectionId {
    /// The id docutils would derive from `title` alone, before any
    /// disambiguation against the rest of the document.
    ///
    /// Prefer [`SectionIdAllocator`] over calling this directly: a bare slug
    /// collides with any other heading of the same text, and is empty for a
    /// heading with no alphanumeric characters.
    #[must_use]
    pub fn from_title(title: &str) -> Self {
        Self(section_slug(title))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether `candidate` is already in the normal form [`section_slug`]
    /// produces — the invariant `Deserialize` enforces.
    fn is_normalized(candidate: &str) -> bool {
        // An allocated id may carry a `-<n>` disambiguating suffix, which is
        // itself already slug-shaped, so re-slugging is idempotent on it.
        !candidate.is_empty() && section_slug(candidate) == candidate
    }
}

impl<'de> Deserialize<'de> for SectionId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        if Self::is_normalized(&raw) {
            Ok(Self(raw))
        } else {
            Err(serde::de::Error::custom(format!(
                "section id {raw:?} is not in normalized form"
            )))
        }
    }
}

/// Converts a heading's plain text into docutils' identifier form.
///
/// A faithful port of docutils' `make_id`: expand the digraphs, map the
/// stroked/hooked letters, NFKD-normalize and drop everything still non-ASCII,
/// collapse each run of non-`[a-z0-9]` into a single hyphen, then strip
/// leading hyphens *and digits* along with trailing hyphens.
///
/// That last step is why `"1. Introduction"` becomes `"introduction"` rather
/// than `"1-introduction"`: an HTML id may not begin with a digit, and
/// docutils drops the digits instead of prefixing them. Returns an empty
/// string when nothing usable is left, which is the caller's cue to fall back
/// to a generated name.
#[must_use]
pub fn section_slug(title: &str) -> String {
    let digraphs: BTreeMap<char, &str> = DIGRAPHS.into_iter().collect();
    let translate: BTreeMap<char, char> = TRANSLATE.into_iter().collect();

    let mut expanded = String::with_capacity(title.len());
    for ch in title.to_lowercase().chars() {
        if let Some(replacement) = digraphs.get(&ch) {
            expanded.push_str(replacement);
        } else if let Some(replacement) = translate.get(&ch) {
            expanded.push(*replacement);
        } else {
            expanded.push(ch);
        }
    }

    // NFKD splits an accented letter into its base plus a combining mark; the
    // ASCII filter then keeps the base and drops the mark, which is how `é`
    // becomes `e` rather than disappearing.
    let ascii: String = expanded.nfkd().filter(char::is_ascii).collect();

    let collapsed = ascii.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut slug = String::with_capacity(collapsed.len());
    let mut pending_hyphen = false;
    for ch in collapsed.chars() {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            if pending_hyphen && !slug.is_empty() {
                slug.push('-');
            }
            pending_hyphen = false;
            slug.push(ch);
        } else {
            pending_hyphen = true;
        }
    }

    // `^[-0-9]+|-+$` — leading hyphens and digits, trailing hyphens.
    let leading_trimmed = slug.trim_start_matches(|c: char| c == '-' || c.is_ascii_digit());
    leading_trimmed.trim_end_matches('-').to_string()
}

/// Hands out the ids of one document's headings, in document order.
///
/// Disambiguation is inherently sequential — the second "Overview" in a file
/// is what becomes `overview-1` — so it cannot be a pure function of one
/// heading's text. Both the analyzer's outline pass and the renderer's heading
/// pass run an allocator over the same document in the same order, which is
/// what guarantees the id in the index matches the id in the HTML. Do not
/// "simplify" either side into calling [`SectionId::from_title`] directly:
/// that silently reintroduces collisions in any document with two headings of
/// the same name.
#[derive(Debug, Default)]
pub struct SectionIdAllocator {
    /// Every id handed out so far, so a collision can be detected.
    used: std::collections::BTreeSet<String>,
    /// Per-prefix counters, mirroring docutils' `document.id_counter`.
    counters: BTreeMap<String, usize>,
}

impl SectionIdAllocator {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Marks `id` as already handed out, without allocating anything.
    ///
    /// For a second allocator covering the same document from a different
    /// title universe — a `.. contents::` block's own self-anchor, allocated
    /// separately from [`allocate_section_ids`]'s heading pass — so its ids
    /// still cannot collide with a heading's, without threading one allocator
    /// through two unrelated call sites.
    pub fn seed(&mut self, id: &SectionId) {
        self.used.insert(id.0.clone());
    }

    /// The id for the next heading, whose text is `title`.
    ///
    /// A heading whose text yields no usable slug gets `section-1`,
    /// `section-2`, …; a heading colliding with an earlier one gets its own
    /// slug plus `-1`, `-2`, …. Both follow docutils' `create_id`.
    pub fn allocate(&mut self, title: &str) -> SectionId {
        let base = section_slug(title);
        if !base.is_empty() && self.used.insert(base.clone()) {
            return SectionId(base);
        }

        let prefix = if base.is_empty() {
            EMPTY_FALLBACK_PREFIX.to_string()
        } else {
            base
        };
        loop {
            let counter = self.counters.entry(prefix.clone()).or_default();
            *counter += 1;
            let candidate = format!("{prefix}-{counter}");
            if self.used.insert(candidate.clone()) {
                return SectionId(candidate);
            }
        }
    }
}

/// The id of every top-level heading in `nodes`, keyed by that heading's
/// index within `nodes`.
///
/// **Both** the analyzer's outline pass and the renderer's heading pass call
/// this one function rather than each running their own allocator, which is
/// what makes it impossible for the id recorded in the index to differ from
/// the id emitted in the HTML. Keying by node index rather than by title is
/// deliberate: two headings can share a title, and it is exactly that case
/// where a mismatch would be silent.
///
/// Only *top-level* headings are sections. A heading nested inside a
/// directive body is not one — reStructuredText has no such construct — so it
/// receives no id and never appears in a document outline.
#[must_use]
pub fn allocate_section_ids(nodes: &[crate::Node]) -> BTreeMap<usize, SectionId> {
    let mut allocator = SectionIdAllocator::new();
    nodes
        .iter()
        .enumerate()
        .filter_map(|(index, node)| match node {
            crate::Node::Heading { text, .. } => {
                let title = crate::inline_plain_text(text);
                Some((index, allocator.allocate(&title)))
            }
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{InlineNode, Node};

    fn heading(level: u8, title: &str) -> Node {
        Node::Heading {
            level,
            text: vec![InlineNode::Text(title.to_string())],
        }
    }

    #[test]
    fn test_allocate_section_ids_keys_by_node_index() {
        // Given — a paragraph between the headings, so the indices are not
        // simply 0 and 1.
        let nodes = vec![
            heading(1, "Guide"),
            Node::Paragraph(vec![InlineNode::Text("Intro".to_string())]),
            heading(2, "Getting Started"),
        ];

        // When
        let ids = allocate_section_ids(&nodes);

        // Then
        assert_eq!(ids.len(), 2);
        assert_eq!(ids[&0].as_str(), "guide");
        assert_eq!(ids[&2].as_str(), "getting-started");
    }

    #[test]
    fn test_allocate_section_ids_disambiguates_repeated_titles() {
        // Given — the case where a title-keyed map would silently collide.
        let nodes = vec![heading(2, "Overview"), heading(2, "Overview")];

        // When
        let ids = allocate_section_ids(&nodes);

        // Then
        assert_eq!(ids[&0].as_str(), "overview");
        assert_eq!(ids[&1].as_str(), "overview-1");
    }

    #[test]
    fn test_allocate_section_ids_ignores_non_headings() {
        // Given
        let nodes = vec![Node::Paragraph(vec![InlineNode::Text("Hi".to_string())])];

        // When
        let ids = allocate_section_ids(&nodes);

        // Then
        assert!(ids.is_empty());
    }

    #[test]
    fn test_section_slug_lowercases_and_hyphenates() {
        // Given
        let title = "Getting Started";

        // When / Then
        assert_eq!(section_slug(title), "getting-started");
    }

    #[test]
    fn test_section_slug_collapses_runs_of_punctuation_into_one_hyphen() {
        // Given
        let title = "Config -- and!!! options";

        // When / Then
        assert_eq!(section_slug(title), "config-and-options");
    }

    #[test]
    fn test_section_slug_strips_leading_digits() {
        // Given — an HTML id may not begin with a digit, and docutils drops
        // the digits rather than prefixing them.
        let title = "1. Introduction";

        // When / Then
        assert_eq!(section_slug(title), "introduction");
    }

    #[test]
    fn test_section_slug_strips_trailing_hyphens() {
        // Given
        let title = "Why? --";

        // When / Then
        assert_eq!(section_slug(title), "why");
    }

    #[test]
    fn test_section_slug_keeps_interior_digits() {
        // Given — only *leading* digits are stripped.
        let title = "Python 3 support";

        // When / Then
        assert_eq!(section_slug(title), "python-3-support");
    }

    #[test]
    fn test_section_slug_reduces_an_accented_letter_to_its_base() {
        // Given — NFKD splits the accent off, and the ASCII filter drops it.
        let title = "Café Configuration";

        // When / Then
        assert_eq!(section_slug(title), "cafe-configuration");
    }

    #[test]
    fn test_section_slug_expands_a_digraph() {
        // Given — `ß` has no combining-mark decomposition, so the transcribed
        // digraph table is what saves it from being dropped.
        let title = "Straße";

        // When / Then
        assert_eq!(section_slug(title), "strasze");
    }

    #[test]
    fn test_section_slug_maps_a_stroked_letter() {
        // Given — `ø` likewise does not decompose.
        let title = "Ønske";

        // When / Then
        assert_eq!(section_slug(title), "onske");
    }

    #[test]
    fn test_section_slug_drops_a_character_with_no_ascii_equivalent() {
        // Given — CJK has neither a decomposition nor a table entry.
        let title = "概要 Overview";

        // When / Then
        assert_eq!(section_slug(title), "overview");
    }

    #[test]
    fn test_section_slug_is_empty_when_nothing_usable_remains() {
        // Given
        let title = "!!! 123";

        // When / Then
        assert_eq!(section_slug(title), "");
    }

    #[test]
    fn test_section_slug_is_idempotent() {
        // Given — the `Deserialize` check relies on this.
        for title in ["Getting Started", "1. Introduction", "Café", "a-b-c"] {
            let once = section_slug(title);

            // When
            let twice = section_slug(&once);

            // Then
            assert_eq!(once, twice, "{title:?}");
        }
    }

    #[test]
    fn test_allocate_returns_the_plain_slug_for_a_unique_heading() {
        // Given
        let mut allocator = SectionIdAllocator::new();

        // When
        let id = allocator.allocate("Getting Started");

        // Then
        assert_eq!(id.as_str(), "getting-started");
    }

    #[test]
    fn test_allocate_disambiguates_a_repeated_heading() {
        // Given — two sections legitimately share a name.
        let mut allocator = SectionIdAllocator::new();

        // When
        let first = allocator.allocate("Overview");
        let second = allocator.allocate("Overview");
        let third = allocator.allocate("Overview");

        // Then
        assert_eq!(first.as_str(), "overview");
        assert_eq!(second.as_str(), "overview-1");
        assert_eq!(third.as_str(), "overview-2");
    }

    #[test]
    fn test_allocate_names_a_heading_with_no_usable_text() {
        // Given
        let mut allocator = SectionIdAllocator::new();

        // When
        let first = allocator.allocate("!!!");
        let second = allocator.allocate("123");

        // Then
        assert_eq!(first.as_str(), "section-1");
        assert_eq!(second.as_str(), "section-2");
    }

    #[test]
    fn test_allocate_never_reuses_an_id_taken_by_an_earlier_heading() {
        // Given — the disambiguated form of the second heading is already the
        // plain form of the first.
        let mut allocator = SectionIdAllocator::new();

        // When
        let first = allocator.allocate("Overview 1");
        let second = allocator.allocate("Overview");
        let third = allocator.allocate("Overview");

        // Then
        assert_eq!(first.as_str(), "overview-1");
        assert_eq!(second.as_str(), "overview");
        assert_ne!(third.as_str(), "overview-1");
        assert_eq!(third.as_str(), "overview-2");
    }

    #[test]
    fn test_seed_keeps_a_later_allocation_from_reusing_it() {
        // Given — a heading id allocated by a different `SectionIdAllocator`.
        let heading_id = SectionId::from_title("Overview");
        let mut allocator = SectionIdAllocator::new();

        // When
        allocator.seed(&heading_id);
        let allocated = allocator.allocate("Overview");

        // Then
        assert_eq!(allocated.as_str(), "overview-1");
    }

    #[test]
    fn test_seed_does_not_affect_an_unrelated_title() {
        // Given
        let heading_id = SectionId::from_title("Overview");
        let mut allocator = SectionIdAllocator::new();

        // When
        allocator.seed(&heading_id);
        let allocated = allocator.allocate("Reference");

        // Then
        assert_eq!(allocated.as_str(), "reference");
    }

    #[test]
    fn test_section_id_round_trips_through_json() {
        // Given
        let id = SectionId::from_title("Getting Started");

        // When
        let json = serde_json::to_string(&id).expect("serializes");
        let restored: SectionId = serde_json::from_str(&json).expect("deserializes");

        // Then
        assert_eq!(restored, id);
    }

    #[test]
    fn test_deserialize_rejects_an_id_that_is_not_normalized() {
        // Given — a hand-edited or stale artifact.
        let json = "\"Getting Started\"";

        // When
        let restored: Result<SectionId, _> = serde_json::from_str(json);

        // Then — it fails loudly rather than emitting a broken link.
        assert!(restored.is_err());
    }

    #[test]
    fn test_deserialize_rejects_an_empty_id() {
        // Given
        let json = "\"\"";

        // When
        let restored: Result<SectionId, _> = serde_json::from_str(json);

        // Then
        assert!(restored.is_err());
    }

    #[test]
    fn test_deserialize_accepts_a_disambiguated_id() {
        // Given — `overview-1` is what the allocator hands out for a repeat.
        let json = "\"overview-1\"";

        // When
        let restored: SectionId = serde_json::from_str(json).expect("deserializes");

        // Then
        assert_eq!(restored.as_str(), "overview-1");
    }
}

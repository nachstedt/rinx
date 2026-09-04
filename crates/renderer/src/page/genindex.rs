//! Renders the site-wide general index (`genindex.html`): every
//! [`rusty_sphinx_index::GenIndexEntry`] grouped alphabetically by primary
//! term, with subentries nested underneath and a letter jump-nav at the top —
//! mirroring Sphinx's `genindex.html`, using rusty-sphinx's own template/CSS
//! system rather than literal DOM/CSS parity with Sphinx's theme.

use crate::config::SiteConfig;
use anyhow::Result;
use rusty_sphinx_index::{GenIndexEntry, ProjectIndex};
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// One resolved link target for a genindex entry.
struct Location {
    doc_path: String,
    anchor: String,
    main: bool,
}

/// A subentry (`single: primary; subentry`) nested under a primary term.
struct SubentryGroup {
    display: String,
    locations: Vec<Location>,
}

/// All occurrences of one primary term, split into locations linked directly
/// from the term itself and locations nested under a subentry.
struct PrimaryGroup {
    display: String,
    direct: Vec<Location>,
    /// Keyed by lowercased subentry text, so subentries sort case-insensitively.
    subentries: BTreeMap<String, SubentryGroup>,
}

/// Groups raw `genindex_entries` by (lowercased) primary term, and within
/// each primary, by (lowercased) subentry. The first-seen casing of each
/// term is kept for display.
fn group_by_primary(entries: &[GenIndexEntry]) -> BTreeMap<String, PrimaryGroup> {
    let mut groups: BTreeMap<String, PrimaryGroup> = BTreeMap::new();
    for entry in entries {
        let group = groups
            .entry(entry.primary.to_lowercase())
            .or_insert_with(|| PrimaryGroup {
                display: entry.primary.clone(),
                direct: Vec::new(),
                subentries: BTreeMap::new(),
            });
        let location = Location {
            doc_path: entry.doc_path.clone(),
            anchor: entry.anchor.clone(),
            main: entry.main,
        };
        match &entry.subentry {
            None => group.direct.push(location),
            Some(sub) => {
                let sub_group = group
                    .subentries
                    .entry(sub.to_lowercase())
                    .or_insert_with(|| SubentryGroup {
                        display: sub.clone(),
                        locations: Vec::new(),
                    });
                sub_group.locations.push(location);
            }
        }
    }
    groups
}

/// Buckets a primary term's display text into an uppercased first letter, or
/// `'\0'` for the "Symbols" bucket (non-alphabetic-leading terms) — `'\0'`
/// sorts before every ASCII letter, so a plain `BTreeMap<char, _>` keyed on
/// this puts Symbols first, matching Sphinx, with no separate ordering pass.
fn bucket_letter(primary: &str) -> char {
    primary
        .chars()
        .next()
        .map(|c| c.to_ascii_uppercase())
        .filter(char::is_ascii_alphabetic)
        .unwrap_or('\0')
}

/// Buckets primary-term groups by [`bucket_letter`]. Each bucket's `Vec`
/// preserves `groups`' iteration order (sorted by lowercased primary), so no
/// further sorting is needed within a bucket.
fn bucket_by_letter(groups: &BTreeMap<String, PrimaryGroup>) -> BTreeMap<char, Vec<&PrimaryGroup>> {
    let mut buckets: BTreeMap<char, Vec<&PrimaryGroup>> = BTreeMap::new();
    for group in groups.values() {
        buckets
            .entry(bucket_letter(&group.display))
            .or_default()
            .push(group);
    }
    buckets
}

/// Converts a `doc_path` (`.rst`) + `anchor` into the href genindex.html — at
/// the site root — uses to link there. No relative-path computation is
/// needed beyond the extension swap, since genindex.html always lives at the
/// site root, so every document's path relative to the root *is* its href.
fn location_href(doc_path: &str, anchor: &str) -> String {
    let html_path = doc_path.strip_suffix(".rst").unwrap_or(doc_path);
    format!("{html_path}.html#{anchor}")
}

/// Renders one comma-separated run of location links. Link text is the
/// target document's title (falling back to its path), matching the
/// existing nav-tree title-fallback convention. `main` locations are bolded.
fn render_locations(html: &mut String, locations: &[Location], index: &ProjectIndex) {
    for (i, location) in locations.iter().enumerate() {
        if i > 0 {
            let _ = write!(html, ", ");
        }
        let title = index
            .document_titles
            .get(&location.doc_path)
            .cloned()
            .unwrap_or_else(|| location.doc_path.clone());
        let title_escaped = html_escape::encode_text(&title);
        let href = location_href(&location.doc_path, &location.anchor);
        let href_attr = html_escape::encode_double_quoted_attribute(&href);
        if location.main {
            let _ = write!(
                html,
                "<strong><a href=\"{href_attr}\">{title_escaped}</a></strong>"
            );
        } else {
            let _ = write!(html, "<a href=\"{href_attr}\">{title_escaped}</a>");
        }
    }
}

/// Renders one primary term's `<li>`: its own direct locations, plus a
/// nested `<ul>` of subentries (each with their own locations), if any.
fn render_primary_group(html: &mut String, group: &PrimaryGroup, index: &ProjectIndex) {
    let _ = write!(html, "<li>");
    let display_escaped = html_escape::encode_text(&group.display);
    let _ = write!(html, "{display_escaped}");
    if !group.direct.is_empty() {
        let _ = write!(html, " ");
        render_locations(html, &group.direct, index);
    }
    if !group.subentries.is_empty() {
        let _ = writeln!(html, "<ul>");
        for sub in group.subentries.values() {
            let _ = write!(html, "<li>");
            let sub_escaped = html_escape::encode_text(&sub.display);
            let _ = write!(html, "{sub_escaped} ");
            render_locations(html, &sub.locations, index);
            let _ = writeln!(html, "</li>");
        }
        let _ = writeln!(html, "</ul>");
    }
    let _ = writeln!(html, "</li>");
}

/// Renders one letter section: a heading (`"Symbols"` for the `'\0'`
/// bucket, the letter itself otherwise) plus its primary terms.
fn render_letter_section(
    html: &mut String,
    letter: char,
    primaries: &[&PrimaryGroup],
    index: &ProjectIndex,
) {
    let heading = if letter == '\0' {
        "Symbols".to_string()
    } else {
        letter.to_string()
    };
    let heading_escaped = html_escape::encode_text(&heading);
    let _ = writeln!(html, "<h2 id=\"{heading_escaped}\">{heading_escaped}</h2>");
    let _ = writeln!(html, "<ul>");
    for group in primaries {
        render_primary_group(html, group, index);
    }
    let _ = writeln!(html, "</ul>");
}

/// Renders the letter jump-nav linking to each bucket actually present.
fn render_jump_nav(html: &mut String, letters: &[char]) {
    let _ = write!(html, "<p class=\"genindex-jumpnav\">");
    for (i, &letter) in letters.iter().enumerate() {
        if i > 0 {
            let _ = write!(html, " | ");
        }
        let label = if letter == '\0' {
            "Symbols".to_string()
        } else {
            letter.to_string()
        };
        let label_escaped = html_escape::encode_text(&label);
        let _ = write!(html, "<a href=\"#{label_escaped}\">{label_escaped}</a>");
    }
    let _ = writeln!(html, "</p>");
}

/// Renders the full `genindex.html` page: the alphabetized index body
/// wrapped in the site's page chrome via [`crate::render_page`].
///
/// # Errors
///
/// Returns an error if the template cannot be parsed or rendered.
pub fn render_genindex(
    index: &ProjectIndex,
    config: &SiteConfig,
    template_str: &str,
) -> Result<String> {
    let groups = group_by_primary(&index.genindex_entries);
    let buckets = bucket_by_letter(&groups);

    let mut body = String::new();
    let letters: Vec<char> = buckets.keys().copied().collect();
    if !letters.is_empty() {
        render_jump_nav(&mut body, &letters);
    }
    for (&letter, primaries) in &buckets {
        render_letter_section(&mut body, letter, primaries, index);
    }

    let has_genindex = !index.genindex_entries.is_empty();
    let css_path = crate::css_relative_path("genindex.html", "default.css");
    crate::render_page(
        &body,
        template_str,
        config,
        // The general index is not a document, so it has no place in the
        // reading order and gets no prev/next links; it still shows the same
        // sidebar as every other page.
        &crate::PageMeta {
            css_path: &css_path,
            page_title: "Index",
            doc_path: "genindex.html",
            source_path: "genindex.html",
            has_genindex,
            ..crate::PageMeta::default()
        }
        .with_navigation(index),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(
        primary: &str,
        subentry: Option<&str>,
        main: bool,
        doc_path: &str,
        anchor: &str,
    ) -> GenIndexEntry {
        GenIndexEntry {
            primary: primary.to_string(),
            subentry: subentry.map(str::to_string),
            main,
            doc_path: doc_path.to_string(),
            anchor: anchor.to_string(),
        }
    }

    #[test]
    fn test_group_by_primary_groups_direct_entries_case_insensitively() {
        // Given
        let entries = vec![
            entry("Python", None, false, "a.rst", "id-0"),
            entry("python", None, false, "b.rst", "id-0"),
        ];

        // When
        let groups = group_by_primary(&entries);

        // Then
        assert_eq!(groups.len(), 1);
        let group = groups.get("python").unwrap();
        assert_eq!(group.display, "Python");
        assert_eq!(group.direct.len(), 2);
    }

    #[test]
    fn test_group_by_primary_nests_subentries() {
        // Given
        let entries = vec![entry("execution", Some("context"), false, "a.rst", "id-0")];

        // When
        let groups = group_by_primary(&entries);

        // Then
        let group = groups.get("execution").unwrap();
        assert!(group.direct.is_empty());
        assert_eq!(group.subentries.len(), 1);
        assert_eq!(group.subentries.get("context").unwrap().display, "context");
    }

    #[test]
    fn test_bucket_letter_uppercases_alphabetic_leading_term() {
        // Given / When / Then
        assert_eq!(bucket_letter("python"), 'P');
        assert_eq!(bucket_letter("Python"), 'P');
    }

    #[test]
    fn test_bucket_letter_returns_symbols_sentinel_for_non_alphabetic_leading_term() {
        // Given / When / Then
        assert_eq!(bucket_letter("__init__"), '\0');
        assert_eq!(bucket_letter("3.14"), '\0');
    }

    #[test]
    fn test_bucket_by_letter_symbols_bucket_sorts_before_letters() {
        // Given
        let entries = vec![
            entry("Python", None, false, "a.rst", "id-0"),
            entry("__init__", None, false, "a.rst", "id-1"),
        ];
        let groups = group_by_primary(&entries);

        // When
        let buckets = bucket_by_letter(&groups);
        let keys: Vec<char> = buckets.keys().copied().collect();

        // Then — '\0' (Symbols) sorts first
        assert_eq!(keys, vec!['\0', 'P']);
    }

    #[test]
    fn test_location_href_swaps_rst_extension_for_html() {
        // Given / When / Then
        assert_eq!(
            location_href("api.rst", "py:function:foo"),
            "api.html#py:function:foo"
        );
    }

    #[test]
    fn test_location_href_preserves_nested_path() {
        // Given / When / Then
        assert_eq!(
            location_href("team_a/index.rst", "id-0"),
            "team_a/index.html#id-0"
        );
    }

    #[test]
    fn test_render_genindex_renders_valid_shell_for_empty_index() {
        // Given
        let index = ProjectIndex::default();
        let config = SiteConfig::default();
        let template = "<html><body>{{ body }}</body></html>";

        // When
        let html = render_genindex(&index, &config, template).unwrap();

        // Then
        assert!(html.contains("<html>"));
        assert!(!html.contains("<h2"));
    }

    #[test]
    fn test_render_genindex_renders_single_term() {
        // Given
        let mut index = ProjectIndex::default();
        index
            .genindex_entries
            .push(entry("execution", None, false, "guide.rst", "index-0"));
        index
            .document_titles
            .insert("guide.rst".to_string(), "Guide".to_string());
        let config = SiteConfig::default();
        let template = "{{ body }}";

        // When
        let html = render_genindex(&index, &config, template).unwrap();

        // Then
        assert!(html.contains("<h2 id=\"E\">E</h2>"));
        assert!(html.contains("execution"));
        assert!(html.contains("<a href=\"guide.html#index-0\">Guide</a>"));
    }

    #[test]
    fn test_render_genindex_renders_subentry_with_two_locations() {
        // Given
        let mut index = ProjectIndex::default();
        index
            .genindex_entries
            .push(entry("Python", Some("interpreter"), false, "a.rst", "id-0"));
        index
            .genindex_entries
            .push(entry("Python", Some("interpreter"), false, "b.rst", "id-0"));
        let config = SiteConfig::default();
        let template = "{{ body }}";

        // When
        let html = render_genindex(&index, &config, template).unwrap();

        // Then — two links, comma-separated, nested under the primary
        assert!(html.contains("interpreter"));
        assert!(html.contains("a.html#id-0"));
        assert!(html.contains("b.html#id-0"));
        assert!(html.contains("</a>, <a"));
    }

    #[test]
    fn test_render_genindex_symbol_leading_term_sorts_before_letters() {
        // Given
        let mut index = ProjectIndex::default();
        index
            .genindex_entries
            .push(entry("Python", None, false, "a.rst", "id-0"));
        index
            .genindex_entries
            .push(entry("__init__", None, false, "a.rst", "id-1"));
        let config = SiteConfig::default();
        let template = "{{ body }}";

        // When
        let html = render_genindex(&index, &config, template).unwrap();

        // Then
        let symbols_pos = html.find("Symbols").unwrap();
        let p_pos = html.find("<h2 id=\"P\">").unwrap();
        assert!(symbols_pos < p_pos);
    }

    #[test]
    fn test_render_genindex_main_entry_renders_as_strong() {
        // Given
        let mut index = ProjectIndex::default();
        index
            .genindex_entries
            .push(entry("Python", None, true, "a.rst", "id-0"));
        let config = SiteConfig::default();
        let template = "{{ body }}";

        // When
        let html = render_genindex(&index, &config, template).unwrap();

        // Then
        assert!(html.contains("<strong><a href=\"a.html#id-0\">"));
    }

    #[test]
    fn test_render_genindex_includes_jump_nav_only_when_entries_present() {
        // Given
        let empty_index = ProjectIndex::default();
        let mut populated_index = ProjectIndex::default();
        populated_index
            .genindex_entries
            .push(entry("Python", None, false, "a.rst", "id-0"));
        let config = SiteConfig::default();
        let template = "{{ body }}";

        // When
        let empty_html = render_genindex(&empty_index, &config, template).unwrap();
        let populated_html = render_genindex(&populated_index, &config, template).unwrap();

        // Then
        assert!(!empty_html.contains("genindex-jumpnav"));
        assert!(populated_html.contains("genindex-jumpnav"));
    }
}

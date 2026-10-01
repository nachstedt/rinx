//! Renders the site-wide general index (`genindex.html`): every
//! [`rinx_index::GenIndexEntry`] grouped alphabetically by primary
//! term, with subentries nested underneath and a letter jump-nav at the top —
//! mirroring Sphinx's `genindex.html`, using rinx's own template/CSS
//! system rather than literal DOM/CSS parity with Sphinx's theme.

use crate::config::SiteConfig;
use anyhow::Result;
use rinx_index::{GenIndexEntry, GenIndexRedirect, ProjectIndex};
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// One resolved link target for a genindex entry.
struct Location {
    doc_path: String,
    anchor: String,
    main: bool,
}

/// A subentry (`single: primary; subentry`) nested under a primary term.
///
/// A `see:`/`seealso:` redirect is one too, reading `see <target>` with no
/// locations at all — Sphinx files it the same way.
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
/// each primary, by (lowercased) subentry, then files each redirect as a
/// location-less subentry of its own primary. The first-seen casing of each
/// term is kept for display.
fn group_by_primary(
    entries: &[GenIndexEntry],
    redirects: &[GenIndexRedirect],
) -> BTreeMap<String, PrimaryGroup> {
    let mut groups: BTreeMap<String, PrimaryGroup> = BTreeMap::new();
    for entry in entries {
        let group = primary_group(&mut groups, &entry.primary);
        let location = Location {
            doc_path: entry.doc_path.clone(),
            anchor: entry.anchor.clone(),
            main: entry.main,
        };
        match &entry.subentry {
            None => group.direct.push(location),
            Some(sub) => subentry_group(group, sub).locations.push(location),
        }
    }
    for redirect in redirects {
        let group = primary_group(&mut groups, &redirect.primary);
        subentry_group(group, &redirect.subentry_text());
    }
    groups
}

/// The group for `primary`, created on first sight with its casing.
fn primary_group<'a>(
    groups: &'a mut BTreeMap<String, PrimaryGroup>,
    primary: &str,
) -> &'a mut PrimaryGroup {
    groups
        .entry(primary.to_lowercase())
        .or_insert_with(|| PrimaryGroup {
            display: primary.to_string(),
            direct: Vec::new(),
            subentries: BTreeMap::new(),
        })
}

/// The subentry `sub` of `group`, created on first sight with its casing.
fn subentry_group<'a>(group: &'a mut PrimaryGroup, sub: &str) -> &'a mut SubentryGroup {
    group
        .subentries
        .entry(sub.to_lowercase())
        .or_insert_with(|| SubentryGroup {
            display: sub.to_string(),
            locations: Vec::new(),
        })
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
            let _ = write!(html, "{sub_escaped}");
            // A redirect has no location, and so nothing after its text.
            if !sub.locations.is_empty() {
                let _ = write!(html, " ");
                render_locations(html, &sub.locations, index);
            }
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
    let groups = group_by_primary(&index.genindex_entries, &index.genindex_redirects);
    let buckets = bucket_by_letter(&groups);

    let mut body = String::new();
    let letters: Vec<char> = buckets.keys().copied().collect();
    if !letters.is_empty() {
        render_jump_nav(&mut body, &letters);
    }
    for (&letter, primaries) in &buckets {
        render_letter_section(&mut body, letter, primaries, index);
    }

    let has_genindex = index.has_genindex_entries();
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
        .with_navigation(index, config),
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
        let groups = group_by_primary(&entries, &[]);

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
        let groups = group_by_primary(&entries, &[]);

        // Then
        let group = groups.get("execution").unwrap();
        assert!(group.direct.is_empty());
        assert_eq!(group.subentries.len(), 1);
        assert_eq!(group.subentries.get("context").unwrap().display, "context");
    }

    fn redirect(
        primary: &str,
        kind: rinx_index::GenIndexRedirectKind,
        target: &str,
    ) -> GenIndexRedirect {
        GenIndexRedirect {
            primary: primary.to_string(),
            kind,
            target: target.to_string(),
        }
    }

    #[test]
    fn test_group_by_primary_files_a_redirect_as_a_location_less_subentry() {
        // Given — a linked term and a redirect from the same primary
        let entries = vec![entry("goto", None, false, "a.rst", "index-0")];
        let redirects = vec![redirect(
            "Goto",
            rinx_index::GenIndexRedirectKind::See,
            "jump",
        )];

        // When
        let groups = group_by_primary(&entries, &redirects);

        // Then
        let group = groups.get("goto").unwrap();
        assert_eq!(group.direct.len(), 1);
        let sub = group.subentries.get("see jump").unwrap();
        assert_eq!(sub.display, "see jump");
        assert!(sub.locations.is_empty());
    }

    #[test]
    fn test_group_by_primary_creates_a_primary_for_a_redirect_alone() {
        // Given
        let redirects = vec![redirect(
            "goto",
            rinx_index::GenIndexRedirectKind::SeeAlso,
            "jump",
        )];

        // When
        let groups = group_by_primary(&[], &redirects);

        // Then
        let group = groups.get("goto").unwrap();
        assert!(group.direct.is_empty());
        assert!(group.subentries.contains_key("see also jump"));
    }

    #[test]
    fn test_primary_group_keeps_the_first_seen_casing() {
        // Given
        let mut groups = BTreeMap::new();

        // When
        primary_group(&mut groups, "Python");
        primary_group(&mut groups, "python");

        // Then
        assert_eq!(groups.len(), 1);
        assert_eq!(groups["python"].display, "Python");
    }

    #[test]
    fn test_subentry_group_keeps_the_first_seen_casing() {
        // Given
        let mut groups = BTreeMap::new();
        let group = primary_group(&mut groups, "Python");

        // When
        subentry_group(group, "Interpreter");
        subentry_group(group, "interpreter");

        // Then
        assert_eq!(group.subentries.len(), 1);
        assert_eq!(group.subentries["interpreter"].display, "Interpreter");
    }

    #[test]
    fn test_render_genindex_renders_a_redirect_as_unlinked_text() {
        // Given — only a redirect
        let mut index = ProjectIndex::default();
        index.genindex_redirects.push(redirect(
            "goto",
            rinx_index::GenIndexRedirectKind::See,
            "jump",
        ));
        let template = "{{ body }}";

        // When
        let html = render_genindex(&index, &SiteConfig::default(), template).unwrap();

        // Then
        assert!(html.contains("<h2 id=\"G\">G</h2>"), "{html}");
        assert!(html.contains("<li>goto<ul>\n<li>see jump</li>"), "{html}");
        assert!(!html.contains(".html#"), "{html}");
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
        let groups = group_by_primary(&entries, &[]);

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

    #[test]
    fn test_render_genindex_links_the_module_index_when_the_site_writes_one() {
        // Given
        let enabled = ProjectIndex {
            domain_indices: [rinx_index::DomainIndex::PyModindex].into(),
            ..ProjectIndex::default()
        };
        let template =
            "{% if modindex_href %}<a href=\"{{ modindex_href }}\">Modules</a>{% endif %}";

        // When
        let with = render_genindex(&enabled, &SiteConfig::default(), template).unwrap();
        let without =
            render_genindex(&ProjectIndex::default(), &SiteConfig::default(), template).unwrap();

        // Then
        assert_eq!(with, "<a href=\"py-modindex.html\">Modules</a>");
        assert_eq!(without, "");
    }
}

//! Renders the Python Module Index (`py-modindex.html`): every module in
//! [`ProjectIndex::modules`], grouped by first letter with submodules under
//! their package, beside its platform, deprecation and synopsis — the one
//! place besides a `:mod:` tooltip that Sphinx shows a module's options
//! (ADR-032).
//!
//! [`group_modules`] is a port of Sphinx 9.1's `PythonModuleIndex.generate`
//! and the markup follows its `domainindex.html`, minus the collapse toggler
//! its JavaScript adds to a package's rows.

use crate::config::SiteConfig;
use anyhow::Result;
use rinx_ast::{ObjectType, PyObjectType, TargetName};
use rinx_index::{DomainIndex, ModuleEntry, ProjectIndex};
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// Where the page is written, at the site root as Sphinx writes it.
pub const MODINDEX_PATH: &str = DomainIndex::PyModindex.page_path();

/// The page's title.
pub const MODINDEX_TITLE: &str = DomainIndex::PyModindex.title();

/// Where a row sits in its package's group — Sphinx's `subtype` 0, 1 and 2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RowKind {
    /// A top-level module with no submodule documented.
    TopLevel,
    /// A package with at least one submodule listed below it.
    GroupHead,
    /// A module inside a package, indented under it.
    Submodule,
}

/// One row of the module index.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ModuleRow<'a> {
    /// The module's name as its definition spelled it.
    name: String,
    kind: RowKind,
    /// The module this row links, or `None` for the placeholder row Sphinx
    /// adds for a submodule whose package is not documented.
    module: Option<(&'a TargetName, &'a ModuleEntry)>,
}

/// Groups the index's modules into rows by the lower-cased first letter of
/// their name, exactly as `PythonModuleIndex.generate` does: in
/// case-insensitive name order, a module whose package is the row above turns
/// that row into a group head, and a submodule whose package is not listed at
/// all gets a placeholder row for it.
///
/// `prefixes` is Sphinx's `modindex_common_prefix`: the longest one a name
/// starts with is set aside while choosing its letter and package — so
/// `email.message` files under "m" as a top-level module — but the row still
/// shows the name in full. A prefix that would leave nothing strips nothing.
fn group_modules<'a>(
    index: &'a ProjectIndex,
    prefixes: &[String],
) -> BTreeMap<String, Vec<ModuleRow<'a>>> {
    let mut prefixes: Vec<&str> = prefixes.iter().map(String::as_str).collect();
    // Sphinx's `sorted(..., key=len, reverse=True)`, which is stable.
    prefixes.sort_by_key(|prefix| std::cmp::Reverse(prefix.len()));

    let mut content: BTreeMap<String, Vec<ModuleRow<'a>>> = BTreeMap::new();
    let mut previous = String::new();
    // `modules` is keyed by the lower-cased name, so its order is Sphinx's
    // `sorted(..., key=lambda t: t[0].lower())`.
    for (key, entry) in &index.modules {
        let full_name = index.domain_object_spelling(key);
        let (stripped, name) = prefixes
            .iter()
            .find_map(|prefix| {
                full_name
                    .strip_prefix(prefix)
                    .filter(|rest| !rest.is_empty())
                    .map(|rest| (*prefix, rest))
            })
            .unwrap_or(("", full_name));
        let letter: String = name
            .chars()
            .next()
            .map(|first| first.to_lowercase().collect())
            .unwrap_or_default();
        let rows = content.entry(letter).or_default();
        let package = name.split('.').next().unwrap_or_default();
        let kind = if package == name {
            RowKind::TopLevel
        } else {
            if previous == package {
                if let Some(head) = rows.last_mut() {
                    head.kind = RowKind::GroupHead;
                }
            } else if !previous.starts_with(package) {
                rows.push(ModuleRow {
                    name: format!("{stripped}{package}"),
                    kind: RowKind::GroupHead,
                    module: None,
                });
            }
            RowKind::Submodule
        };
        rows.push(ModuleRow {
            name: full_name.to_string(),
            kind,
            module: Some((key, entry)),
        });
        previous = name.to_string();
    }
    content
}

/// Writes the jump box linking each letter's section.
fn render_jumpbox(html: &mut String, letters: &[&String]) {
    let _ = write!(html, "<div class=\"modindex-jumpbox\">");
    for (position, letter) in letters.iter().enumerate() {
        if position > 0 {
            let _ = write!(html, " | ");
        }
        let _ = write!(
            html,
            "<a href=\"#cap-{}\"><strong>{}</strong></a>",
            html_escape::encode_double_quoted_attribute(letter),
            html_escape::encode_text(letter)
        );
    }
    let _ = writeln!(html, "</div>");
}

/// Whether the index starts with every package's submodules folded away —
/// Sphinx's heuristic: only when top-level modules outnumber submodules, so a
/// reader of a mostly flat index sees it whole. A documented package counts
/// as top-level, as it does in Sphinx; a placeholder row counts as nothing.
fn starts_collapsed(content: &BTreeMap<String, Vec<ModuleRow<'_>>>) -> bool {
    let (mut top_levels, mut submodules) = (0_usize, 0_usize);
    for row in content
        .values()
        .flatten()
        .filter(|row| row.module.is_some())
    {
        match row.kind {
            RowKind::Submodule => submodules += 1,
            RowKind::TopLevel | RowKind::GroupHead => top_levels += 1,
        }
    }
    submodules < top_levels
}

/// Writes one module's row: its linked name and platform, then its
/// deprecation and synopsis — every value escaped, never parsed.
///
/// A group head carries the toggle folding its package's submodules, checked
/// (submodules shown) unless `collapsed`; the stylesheet hides a group's
/// `modindex-submodule` rows while its toggle is unchecked, so folding needs
/// no script.
fn render_row(html: &mut String, row: &ModuleRow<'_>, collapsed: bool) {
    let name = html_escape::encode_text(&row.name);
    match row.kind {
        RowKind::TopLevel => {
            let _ = write!(html, "<tr><td>");
        }
        RowKind::GroupHead => {
            let checked = if collapsed { "" } else { " checked" };
            let _ = write!(
                html,
                "<tr><td><input type=\"checkbox\" class=\"modindex-toggle\" \
                 aria-label=\"Show the submodules of {}\"{checked}>",
                html_escape::encode_double_quoted_attribute(&row.name)
            );
        }
        RowKind::Submodule => {
            let _ = write!(
                html,
                "<tr class=\"modindex-submodule\"><td>&#160;&#160;&#160;"
            );
        }
    }
    match row.module {
        Some((key, entry)) => {
            let anchor = rinx_ast::build_domain_object_key(
                ObjectType::Py(PyObjectType::Module),
                key.as_str(),
            );
            let href = format!(
                "{}#{}",
                rinx_index::relative_doc_href(&entry.doc_path, MODINDEX_PATH),
                anchor.as_str()
            );
            let _ = write!(
                html,
                "<a href=\"{}\"><code class=\"xref\">{name}</code></a>",
                html_escape::encode_double_quoted_attribute(&href)
            );
            if let Some(platform) = &entry.platform {
                let _ = write!(html, " <em>({})</em>", html_escape::encode_text(platform));
            }
            let _ = write!(html, "</td><td>");
            if entry.deprecated {
                let _ = write!(html, "<strong>Deprecated:</strong> ");
            }
            let synopsis = entry.synopsis.as_deref().unwrap_or_default();
            let _ = write!(html, "<em>{}</em>", html_escape::encode_text(synopsis));
        }
        None => {
            let _ = write!(html, "<code class=\"xref\">{name}</code></td><td>");
        }
    }
    let _ = writeln!(html, "</td></tr>");
}

/// Writes one letter's section: its heading row, then its rows, each package
/// with its submodules in a `<tbody class="modindex-group">` of its own, which
/// is what its toggle folds.
fn render_letter(html: &mut String, letter: &str, rows: &[ModuleRow<'_>], collapsed: bool) {
    let _ = writeln!(
        html,
        "<tbody><tr class=\"cap\" id=\"cap-{}\"><td><strong>{}</strong></td><td></td></tr>",
        html_escape::encode_double_quoted_attribute(letter),
        html_escape::encode_text(letter)
    );
    let mut in_group = false;
    for row in rows {
        match row.kind {
            RowKind::GroupHead => {
                let _ = writeln!(html, "</tbody>");
                let _ = writeln!(html, "<tbody class=\"modindex-group\">");
                in_group = true;
            }
            // A top-level module after a group ends it.
            RowKind::TopLevel if in_group => {
                let _ = writeln!(html, "</tbody>");
                let _ = writeln!(html, "<tbody>");
                in_group = false;
            }
            RowKind::TopLevel | RowKind::Submodule => {}
        }
        render_row(html, row, collapsed);
    }
    let _ = writeln!(html, "</tbody>");
}

/// Writes the page body: the heading, the jump box and one table section per
/// letter, grouping names with `prefixes` set aside (see [`group_modules`]).
fn render_modindex_body(index: &ProjectIndex, prefixes: &[String]) -> String {
    let content = group_modules(index, prefixes);
    let collapsed = starts_collapsed(&content);
    let mut html = String::new();
    let _ = writeln!(html, "<h1>{MODINDEX_TITLE}</h1>");
    let letters: Vec<&String> = content.keys().collect();
    render_jumpbox(&mut html, &letters);
    let _ = writeln!(html, "<table class=\"indextable modindextable\">");
    for (letter, rows) in &content {
        render_letter(&mut html, letter, rows, collapsed);
    }
    let _ = writeln!(html, "</table>");
    html
}

/// Renders the full `py-modindex.html` page, wrapped in the site's page
/// chrome via [`crate::render_page`].
///
/// # Errors
///
/// Returns an error if the template cannot be parsed or rendered.
pub fn render_modindex(
    index: &ProjectIndex,
    config: &SiteConfig,
    template_str: &str,
) -> Result<String> {
    let body = render_modindex_body(index, &config.modindex_common_prefix);
    let css_path = crate::css_relative_path(MODINDEX_PATH, "default.css");
    crate::render_page(
        &body,
        template_str,
        config,
        // Like the general index, not a document: no place in the reading
        // order, but the same sidebar as every other page.
        &crate::PageMeta {
            css_path: &css_path,
            page_title: MODINDEX_TITLE,
            doc_path: MODINDEX_PATH,
            source_path: MODINDEX_PATH,
            has_genindex: index.has_genindex_entries(),
            ..crate::PageMeta::default()
        }
        .with_navigation(index, config),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An index documenting `modules`, each as `(name, entry)`.
    fn index_with(modules: &[(&str, ModuleEntry)]) -> ProjectIndex {
        let mut index = ProjectIndex::default();
        for (name, entry) in modules {
            index.insert_domain_object(ObjectType::Py(PyObjectType::Module), name, &entry.doc_path);
            index.modules.insert(TargetName::new(name), entry.clone());
        }
        index
    }

    /// One row reduced to what the grouping decides: its name, its place in
    /// the group, and whether it links a module.
    type RowShape = (String, RowKind, bool);

    /// Each letter's rows as [`RowShape`]s, for comparing shapes.
    fn shape(index: &ProjectIndex) -> Vec<(String, Vec<RowShape>)> {
        group_modules(index, &[])
            .into_iter()
            .map(|(letter, rows)| {
                let rows = rows
                    .into_iter()
                    .map(|row| (row.name, row.kind, row.module.is_some()))
                    .collect();
                (letter, rows)
            })
            .collect()
    }

    fn module(doc_path: &str) -> ModuleEntry {
        ModuleEntry::new(doc_path)
    }

    #[test]
    fn test_group_modules_sorts_case_insensitively_under_lower_case_letters() {
        // Given
        let index = index_with(&[
            ("zlib", module("zlib.rst")),
            ("Tkinter", module("tk.rst")),
            ("abc", module("abc.rst")),
        ]);

        // When
        let letters: Vec<String> = group_modules(&index, &[]).into_keys().collect();

        // Then
        assert_eq!(letters, ["a", "t", "z"]);
    }

    #[test]
    fn test_group_modules_keeps_the_spelling_the_definition_wrote() {
        // Given
        let index = index_with(&[("Tkinter", module("tk.rst"))]);

        // When
        let groups = shape(&index);

        // Then
        assert_eq!(groups[0].1[0].0, "Tkinter");
    }

    #[test]
    fn test_group_modules_turns_a_documented_package_into_a_group_head() {
        // Given
        let index = index_with(&[
            ("email", module("email.rst")),
            ("email.message", module("email.message.rst")),
            ("email.parser", module("email.parser.rst")),
        ]);

        // When
        let groups = shape(&index);

        // Then
        assert_eq!(
            groups,
            [(
                "e".to_string(),
                vec![
                    ("email".to_string(), RowKind::GroupHead, true),
                    ("email.message".to_string(), RowKind::Submodule, true),
                    ("email.parser".to_string(), RowKind::Submodule, true),
                ]
            )]
        );
    }

    #[test]
    fn test_group_modules_adds_a_placeholder_for_an_undocumented_package() {
        // Given — `xml` itself is never documented
        let index = index_with(&[
            ("xml.dom", module("xml.dom.rst")),
            ("xml.sax", module("xml.sax.rst")),
        ]);

        // When
        let groups = shape(&index);

        // Then — one unlinked head, not one per submodule
        assert_eq!(
            groups[0].1,
            [
                ("xml".to_string(), RowKind::GroupHead, false),
                ("xml.dom".to_string(), RowKind::Submodule, true),
                ("xml.sax".to_string(), RowKind::Submodule, true),
            ]
        );
    }

    #[test]
    fn test_group_modules_leaves_a_lone_top_level_module_ungrouped() {
        // Given
        let index = index_with(&[("abc", module("abc.rst")), ("array", module("array.rst"))]);

        // When
        let groups = shape(&index);

        // Then
        assert_eq!(
            groups[0].1,
            [
                ("abc".to_string(), RowKind::TopLevel, true),
                ("array".to_string(), RowKind::TopLevel, true),
            ]
        );
    }

    /// Each letter's rows, grouped with `prefixes` as Sphinx's
    /// `modindex_common_prefix`.
    fn shape_with_prefixes(
        index: &ProjectIndex,
        prefixes: &[&str],
    ) -> Vec<(String, Vec<RowShape>)> {
        let prefixes: Vec<String> = prefixes.iter().map(ToString::to_string).collect();
        group_modules(index, &prefixes)
            .into_iter()
            .map(|(letter, rows)| {
                let rows = rows
                    .into_iter()
                    .map(|row| (row.name, row.kind, row.module.is_some()))
                    .collect();
                (letter, rows)
            })
            .collect()
    }

    #[test]
    fn test_a_common_prefix_files_a_module_by_the_rest_of_its_name() {
        // Given
        let index = index_with(&[
            ("email", module("email.rst")),
            ("email.message", module("email.message.rst")),
        ]);

        // When
        let groups = shape_with_prefixes(&index, &["email."]);

        // Then — `email.message` is filed under "m" as a top-level module,
        // still shown under its full name, and `email` is left as it is
        assert_eq!(
            groups,
            [
                (
                    "e".to_string(),
                    vec![("email".to_string(), RowKind::TopLevel, true)]
                ),
                (
                    "m".to_string(),
                    vec![("email.message".to_string(), RowKind::TopLevel, true)]
                ),
            ]
        );
    }

    #[test]
    fn test_the_longest_common_prefix_wins() {
        // Given
        let index = index_with(&[("xml.dom.minidom", module("minidom.rst"))]);

        // When
        let groups = shape_with_prefixes(&index, &["xml.", "xml.dom."]);

        // Then — filed as `minidom`, not as `dom.minidom` under `dom`
        assert_eq!(
            groups,
            [(
                "m".to_string(),
                vec![("xml.dom.minidom".to_string(), RowKind::TopLevel, true)]
            )]
        );
    }

    #[test]
    fn test_a_prefix_equal_to_the_whole_name_strips_nothing() {
        // Given
        let index = index_with(&[("abc", module("abc.rst"))]);

        // When
        let groups = shape_with_prefixes(&index, &["abc"]);

        // Then
        assert_eq!(
            groups,
            [(
                "a".to_string(),
                vec![("abc".to_string(), RowKind::TopLevel, true)]
            )]
        );
    }

    #[test]
    fn test_a_stripped_submodule_gets_a_placeholder_named_in_full() {
        // Given — `pkg.sub` is not documented
        let index = index_with(&[("pkg.sub.leaf", module("leaf.rst"))]);

        // When
        let groups = shape_with_prefixes(&index, &["pkg."]);

        // Then — the placeholder carries the stripped prefix back, as Sphinx's
        // `stripped + package` does
        assert_eq!(
            groups,
            [(
                "s".to_string(),
                vec![
                    ("pkg.sub".to_string(), RowKind::GroupHead, false),
                    ("pkg.sub.leaf".to_string(), RowKind::Submodule, true),
                ]
            )]
        );
    }

    #[test]
    fn test_group_modules_is_empty_without_modules() {
        // Given / When / Then
        assert!(group_modules(&ProjectIndex::default(), &[]).is_empty());
    }

    #[test]
    fn test_render_row_shows_every_option_as_escaped_text() {
        // Given — markup in a synopsis stays text, as in Sphinx
        let index = index_with(&[(
            "winreg",
            ModuleEntry {
                synopsis: Some("Registry <access> via :pep:`1`.".to_string()),
                platform: Some("Windows".to_string()),
                deprecated: true,
                ..module("library/winreg.rst")
            },
        )]);
        let rows = group_modules(&index, &[]);
        let mut html = String::new();

        // When
        render_row(&mut html, &rows["w"][0], false);

        // Then
        assert_eq!(
            html,
            "<tr><td><a href=\"library/winreg.html#py:module:winreg\">\
             <code class=\"xref\">winreg</code></a> <em>(Windows)</em></td>\
             <td><strong>Deprecated:</strong> \
             <em>Registry &lt;access&gt; via :pep:`1`.</em></td></tr>\n"
        );
    }

    #[test]
    fn test_render_row_indents_a_submodule_and_leaves_a_placeholder_unlinked() {
        // Given
        let index = index_with(&[("xml.dom", module("xml.dom.rst"))]);
        let rows = group_modules(&index, &[]);
        let mut placeholder = String::new();
        let mut submodule = String::new();

        // When
        render_row(&mut placeholder, &rows["x"][0], false);
        render_row(&mut submodule, &rows["x"][1], false);

        // Then — the placeholder heads the group, so it carries the toggle
        assert_eq!(
            placeholder,
            "<tr><td><input type=\"checkbox\" class=\"modindex-toggle\" \
             aria-label=\"Show the submodules of xml\" checked>\
             <code class=\"xref\">xml</code></td><td></td></tr>\n"
        );
        assert!(
            submodule.starts_with("<tr class=\"modindex-submodule\"><td>&#160;&#160;&#160;<a ")
        );
    }

    #[test]
    fn test_render_jumpbox_links_each_letter_section() {
        // Given
        let a = "a".to_string();
        let z = "z".to_string();
        let mut html = String::new();

        // When
        render_jumpbox(&mut html, &[&a, &z]);

        // Then
        assert_eq!(
            html,
            "<div class=\"modindex-jumpbox\"><a href=\"#cap-a\"><strong>a</strong></a> | \
             <a href=\"#cap-z\"><strong>z</strong></a></div>\n"
        );
    }

    #[test]
    fn test_render_modindex_body_heads_each_letter_section() {
        // Given
        let index = index_with(&[("abc", module("abc.rst"))]);

        // When
        let html = render_modindex_body(&index, &[]);

        // Then
        assert!(html.starts_with("<h1>Python Module Index</h1>\n"));
        assert!(html.contains("<tr class=\"cap\" id=\"cap-a\"><td><strong>a</strong></td>"));
        assert!(html.contains("<table class=\"indextable modindextable\">"));
    }

    #[test]
    fn test_render_modindex_wraps_the_body_in_the_page_template() {
        // Given
        let index = index_with(&[("abc", module("abc.rst"))]);
        let template = "<title>{{ page_title }}</title>{{ body }}";

        // When
        let html = render_modindex(&index, &SiteConfig::default(), template).expect("renders");

        // Then
        assert!(html.contains("<title>Python Module Index</title>"));
        assert!(html.contains("href=\"abc.html#py:module:abc\""));
    }

    #[test]
    fn test_render_modindex_files_modules_by_the_configured_common_prefix() {
        // Given
        let index = index_with(&[("email.message", module("email.message.rst"))]);
        let config = SiteConfig {
            modindex_common_prefix: vec!["email.".to_string()],
            ..SiteConfig::default()
        };

        // When
        let html = render_modindex(&index, &config, "{{ body }}").expect("renders");

        // Then
        assert!(html.contains("id=\"cap-m\""));
        assert!(!html.contains("id=\"cap-e\""));
    }

    #[test]
    fn test_starts_collapsed_when_top_level_modules_outnumber_submodules() {
        // Given — Sphinx counts a package with submodules as top-level too
        let mostly_top = index_with(&[
            ("abc", module("abc.rst")),
            ("email", module("email.rst")),
            ("email.message", module("email.message.rst")),
        ]);
        let mostly_sub = index_with(&[
            ("email", module("email.rst")),
            ("email.message", module("email.message.rst")),
            ("email.parser", module("email.parser.rst")),
        ]);

        // When / Then — two against one collapses, one against two does not
        assert!(starts_collapsed(&group_modules(&mostly_top, &[])));
        assert!(!starts_collapsed(&group_modules(&mostly_sub, &[])));
    }

    #[test]
    fn test_a_package_and_its_submodules_share_one_collapsible_group() {
        // Given
        let index = index_with(&[
            ("abc", module("abc.rst")),
            ("email", module("email.rst")),
            ("email.message", module("email.message.rst")),
            ("enum", module("enum.rst")),
        ]);

        // When
        let html = render_modindex_body(&index, &[]);

        // Then — `email`'s rows are one group, with a toggle in its head row;
        // `enum` after it is outside that group again
        let group = html
            .split("<tbody class=\"modindex-group\">")
            .nth(1)
            .expect("a group")
            .split("</tbody>")
            .next()
            .unwrap();
        assert!(group.contains(">email</code>"));
        assert!(group.contains("<tr class=\"modindex-submodule\">"));
        assert!(group.contains(">email.message</code>"));
        assert!(group.contains("class=\"modindex-toggle\""));
        assert!(!group.contains(">enum</code>"));
        assert!(html.contains(">enum</code>"));
    }

    #[test]
    fn test_a_group_starts_open_unless_the_index_starts_collapsed() {
        // Given — one top-level module against one submodule: not collapsed
        let open = index_with(&[
            ("email", module("email.rst")),
            ("email.message", module("email.message.rst")),
        ]);
        // Three top-level modules against one submodule: collapsed
        let collapsed = index_with(&[
            ("abc", module("abc.rst")),
            ("array", module("array.rst")),
            ("email", module("email.rst")),
            ("email.message", module("email.message.rst")),
        ]);

        // When
        let open_html = render_modindex_body(&open, &[]);
        let collapsed_html = render_modindex_body(&collapsed, &[]);

        // Then — checked means the submodules are shown
        assert!(open_html.contains(" checked>"));
        assert!(collapsed_html.contains("class=\"modindex-toggle\""));
        assert!(!collapsed_html.contains(" checked>"));
    }

    #[test]
    fn test_the_toggle_names_the_package_it_folds() {
        // Given
        let index = index_with(&[("xml.dom", module("xml.dom.rst"))]);

        // When
        let html = render_modindex_body(&index, &[]);

        // Then
        assert!(html.contains("aria-label=\"Show the submodules of xml\""));
    }

    #[test]
    fn test_render_letter_keeps_ungrouped_modules_in_one_section() {
        // Given
        let index = index_with(&[("abc", module("abc.rst")), ("array", module("array.rst"))]);
        let content = group_modules(&index, &[]);
        let mut html = String::new();

        // When
        render_letter(&mut html, "a", &content["a"], false);

        // Then — one section, opened by the letter's heading, and no group
        assert!(html.starts_with("<tbody><tr class=\"cap\" id=\"cap-a\">"));
        assert_eq!(html.matches("<tbody").count(), 1);
        assert!(!html.contains("modindex-toggle"));
    }
}

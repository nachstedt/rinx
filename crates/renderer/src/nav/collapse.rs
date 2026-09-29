//! Trimming the sidebar's tree down to the branch holding the current page.
//!
//! The sidebar expands the whole site on every page. Folding it in the browser
//! hides that, but it does not shrink it: on `CPython`'s documentation the
//! sidebar was ~1.1 MB of every ~1.2 MB page. This is Sphinx's
//! `toctree(collapse=True)`, applied after the walk rather than inside it so
//! the walk stays the one shared with an in-page toctree, which never
//! collapses.

use super::entry::ResolvedNavEntry;

/// How much of the site's tree the sidebar shows.
///
/// Read from `rinx.toml`'s `collapse_navigation`; see
/// [`crate::config::SiteConfig::sidebar_tree`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SidebarTree {
    /// Every entry with every branch nested, for the template to fold.
    Full,
    /// Every top-level entry, but only the current page, its ancestors and
    /// their siblings nested — every other entry is a leaf.
    #[default]
    CurrentBranch,
}

impl SidebarTree {
    /// `entries` shaped as this asks.
    pub(crate) fn shape(self, entries: Vec<ResolvedNavEntry>) -> Vec<ResolvedNavEntry> {
        match self {
            Self::Full => entries,
            Self::CurrentBranch => prune_off_current_branch(entries),
        }
    }
}

/// Drops the children of every entry that neither is the current page nor
/// leads to it.
///
/// The current page keeps its own children, but not theirs: they are not on
/// the current branch, which is exactly where Sphinx stops as well.
fn prune_off_current_branch(entries: Vec<ResolvedNavEntry>) -> Vec<ResolvedNavEntry> {
    entries
        .into_iter()
        .map(|entry| match entry {
            ResolvedNavEntry::Page {
                title,
                href,
                anchor,
                secnumber,
                is_current,
                is_ancestor,
                children,
            } => ResolvedNavEntry::Page {
                title,
                href,
                anchor,
                secnumber,
                is_current,
                is_ancestor,
                children: if is_current || is_ancestor {
                    prune_off_current_branch(children)
                } else {
                    Vec::new()
                },
            },
            external @ ResolvedNavEntry::External { .. } => external,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(title: &str, is_current: bool, children: Vec<ResolvedNavEntry>) -> ResolvedNavEntry {
        let is_ancestor = children.iter().any(ResolvedNavEntry::contains_current);
        ResolvedNavEntry::Page {
            title: title.to_string(),
            href: minijinja::Value::from_safe_string(format!("{title}.html")),
            anchor: None,
            secnumber: None,
            is_current,
            is_ancestor,
            children,
        }
    }

    fn external(title: &str) -> ResolvedNavEntry {
        ResolvedNavEntry::External {
            title: title.to_string(),
            href: minijinja::Value::from_safe_string("https://example.org".to_string()),
        }
    }

    /// A compact shape of the tree: titles, children in parentheses.
    fn shape(entries: &[ResolvedNavEntry]) -> String {
        entries
            .iter()
            .map(|entry| {
                if entry.children().is_empty() {
                    entry.title().to_string()
                } else {
                    format!("{}({})", entry.title(), shape(entry.children()))
                }
            })
            .collect::<Vec<_>>()
            .join(",")
    }

    /// `guide` holds the current page `setup`, which has a child of its own
    /// with a grandchild; `api` is a branch off the current path.
    fn site() -> Vec<ResolvedNavEntry> {
        vec![
            page(
                "guide",
                false,
                vec![
                    page(
                        "setup",
                        true,
                        vec![page("linux", false, vec![page("debian", false, vec![])])],
                    ),
                    page("usage", false, vec![page("cli", false, vec![])]),
                ],
            ),
            page("api", false, vec![page("parser", false, vec![])]),
        ]
    }

    #[test]
    fn test_full_tree_is_left_as_it_is() {
        // Given
        let entries = site();

        // When
        let shaped = SidebarTree::Full.shape(entries.clone());

        // Then
        assert_eq!(shaped, entries);
    }

    #[test]
    fn test_current_branch_drops_the_children_of_a_branch_off_the_current_path() {
        // Given / When
        let shaped = SidebarTree::CurrentBranch.shape(site());

        // Then — `api` stays listed, but as a leaf.
        assert!(shape(&shaped).ends_with(",api"), "{}", shape(&shaped));
    }

    #[test]
    fn test_current_branch_keeps_the_ancestors_and_their_siblings() {
        // Given / When
        let shaped = SidebarTree::CurrentBranch.shape(site());

        // Then — `usage` is a sibling on the path, listed without its children.
        assert!(
            shape(&shaped).starts_with("guide(setup("),
            "{}",
            shape(&shaped)
        );
        assert!(shape(&shaped).contains("),usage)"), "{}", shape(&shaped));
    }

    #[test]
    fn test_current_branch_keeps_the_current_pages_children_but_not_theirs() {
        // Given / When
        let shaped = SidebarTree::CurrentBranch.shape(site());

        // Then
        assert_eq!(shape(&shaped), "guide(setup(linux),usage),api");
    }

    #[test]
    fn test_current_branch_passes_an_external_entry_through() {
        // Given
        let entries = vec![external("Upstream"), page("intro", true, vec![])];

        // When
        let shaped = SidebarTree::CurrentBranch.shape(entries.clone());

        // Then
        assert_eq!(shaped, entries);
    }

    #[test]
    fn test_current_branch_on_a_page_no_toctree_reaches_leaves_only_the_top_level() {
        // Given — the page being rendered is nowhere in the tree.
        let entries = vec![page("api", false, vec![page("parser", false, vec![])])];

        // When
        let shaped = SidebarTree::CurrentBranch.shape(entries);

        // Then
        assert_eq!(shape(&shaped), "api");
    }
}

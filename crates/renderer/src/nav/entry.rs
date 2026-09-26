use rinx_ast::SectionId;

/// One line of a rendered navigation tree, with everything a template or the
/// HTML writer needs already resolved.
///
/// A document entry and a section entry differ only by whether they carry an
/// `anchor`, so they share [`Self::Page`] rather than being split. An external
/// link genuinely differs — it has no anchor, no children, no section number
/// and no notion of being the current page — so it gets its own variant rather
/// than a `Page` carrying four permanently meaningless fields.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(untagged)]
pub enum ResolvedNavEntry {
    Page {
        title: String,
        /// Relative to the page being rendered, `#anchor` included when this
        /// is a section.
        href: minijinja::Value,
        /// `None` for a whole document, `Some` for one of its sections.
        #[serde(skip)]
        anchor: Option<SectionId>,
        /// The `:numbered:` section number, as its components (`[1, 2]` for
        /// section 1.2), or `None` when nothing numbers this entry.
        secnumber: Option<Vec<usize>>,
        /// Whether this entry is the page currently being rendered.
        is_current: bool,
        /// Whether this entry is an ancestor of the page being rendered, so a
        /// template can keep the path to the current page expanded.
        is_ancestor: bool,
        children: Vec<Self>,
    },
    External {
        title: String,
        href: minijinja::Value,
    },
}

impl ResolvedNavEntry {
    /// The entry's display text.
    #[must_use]
    pub fn title(&self) -> &str {
        match self {
            Self::Page { title, .. } | Self::External { title, .. } => title,
        }
    }

    /// The entries nested under this one. An external link never has any.
    #[must_use]
    pub fn children(&self) -> &[Self] {
        match self {
            Self::Page { children, .. } => children,
            Self::External { .. } => &[],
        }
    }

    /// Whether this entry, or anything below it, is the current page.
    #[must_use]
    pub fn contains_current(&self) -> bool {
        match self {
            Self::Page {
                is_current,
                children,
                ..
            } => *is_current || children.iter().any(Self::contains_current),
            Self::External { .. } => false,
        }
    }
}

/// Renders a section number as the prefix Sphinx shows: `1.2.` for `[1, 2]`.
#[must_use]
pub fn format_secnumber(secnumber: &[usize]) -> String {
    let joined = secnumber
        .iter()
        .map(usize::to_string)
        .collect::<Vec<_>>()
        .join(".");
    format!("{joined}.")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(title: &str, is_current: bool, children: Vec<ResolvedNavEntry>) -> ResolvedNavEntry {
        ResolvedNavEntry::Page {
            title: title.to_string(),
            href: minijinja::Value::from_safe_string(format!("{title}.html")),
            anchor: None,
            secnumber: None,
            is_current,
            is_ancestor: false,
            children,
        }
    }

    #[test]
    fn test_title_reads_both_variants() {
        // Given
        let document = page("Intro", false, vec![]);
        let external = ResolvedNavEntry::External {
            title: "Upstream".to_string(),
            href: minijinja::Value::from_safe_string("https://example.org".to_string()),
        };

        // When / Then
        assert_eq!(document.title(), "Intro");
        assert_eq!(external.title(), "Upstream");
    }

    #[test]
    fn test_external_entry_has_no_children() {
        // Given
        let external = ResolvedNavEntry::External {
            title: "Upstream".to_string(),
            href: minijinja::Value::from_safe_string("https://example.org".to_string()),
        };

        // When / Then
        assert!(external.children().is_empty());
    }

    #[test]
    fn test_contains_current_finds_the_current_page_itself() {
        // Given
        let entry = page("Intro", true, vec![]);

        // When / Then
        assert!(entry.contains_current());
    }

    #[test]
    fn test_contains_current_finds_a_current_descendant() {
        // Given — the current page is two levels down.
        let entry = page(
            "Guide",
            false,
            vec![page("Setup", false, vec![page("Deep", true, vec![])])],
        );

        // When / Then
        assert!(entry.contains_current());
    }

    #[test]
    fn test_contains_current_is_false_for_an_unrelated_branch() {
        // Given
        let entry = page("Guide", false, vec![page("Setup", false, vec![])]);

        // When / Then
        assert!(!entry.contains_current());
    }

    #[test]
    fn test_format_secnumber_joins_components_and_appends_a_dot() {
        // Given / When / Then
        assert_eq!(format_secnumber(&[1]), "1.");
        assert_eq!(format_secnumber(&[1, 2]), "1.2.");
        assert_eq!(format_secnumber(&[3, 1, 4]), "3.1.4.");
    }
}

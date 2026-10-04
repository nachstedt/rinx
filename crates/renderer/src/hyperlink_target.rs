//! Resolving a named hyperlink reference (`` `name`_ ``, and an image's
//! `:target: name_`) to the href it links.
//!
//! In docutils a named reference resolves within its own document only — a
//! label in another document is reached with `:ref:`, never with `name_` —
//! so a document is asked for its own targets and nothing else:
//!
//! - its **external** targets (`.. _name: https://…`), read from the
//!   document itself. They are never put in the shared [`ProjectIndex`]: two
//!   documents may each give a name a different URL. That is also why the
//!   live preview finds them without any index at all.
//! - its **internal** targets — a `.. _label:`, a `:name:`, an entity —
//!   read from the index entries recorded for this document, so the analyzer
//!   stays the one place that decides what a target is and where it anchors.
//! - its **section titles**, each an implicit target of its own section, as
//!   docutils makes them. An explicit target of the same name wins.
//!
//! A name standing for two different things within one of these kinds stands
//! for neither: picking one would depend on which was written first, and
//! docutils refuses such a reference too.
//!
//! An explicit target may also be an *alias* of another: `.. _a: b_`, or the
//! text of an embedded `` `a <b_>`_ ``. Aliases are followed within the same
//! document, and a chain that comes back on itself resolves to nothing.
//! The text of an embedded `` `text <https://…>`_ `` is an explicit target
//! too, as docutils makes it, so a later `` `text`_ `` links the same URI.

use std::collections::{BTreeMap, BTreeSet};

use rinx_ast::{
    Document, HyperlinkTarget, InlineNode, LinkDestination, Node, SectionId, TargetName,
    for_each_inline_list, inline_plain_text, walk_nodes,
};
use rinx_index::ProjectIndex;

/// The targets a named reference written in one document may reach, each
/// mapped to where it leads.
#[derive(Debug, Default)]
pub(crate) struct DocumentHyperlinkTargets {
    /// External targets, internal targets (as a `#anchor` URI) and aliases,
    /// together: docutils keeps them in one namespace, so a name that is two
    /// of them is a duplicate.
    explicit: BTreeMap<TargetName, BTreeSet<LinkDestination>>,
    /// The sections, as `#anchor` URIs, by title.
    implicit: BTreeMap<TargetName, BTreeSet<LinkDestination>>,
}

impl DocumentHyperlinkTargets {
    /// The targets of `doc`: its hyperlink targets and embedded references
    /// wherever they are nested, the internal targets `index` records for it,
    /// and its top-level section titles, anchored at `section_ids` (by node
    /// index, as [`rinx_ast::allocate_section_ids`] returns them).
    pub(crate) fn collect(
        doc: &Document,
        index: &ProjectIndex,
        section_ids: &BTreeMap<usize, SectionId>,
    ) -> Self {
        let mut explicit: BTreeMap<TargetName, BTreeSet<LinkDestination>> = BTreeMap::new();
        walk_nodes(&doc.nodes, &mut |node| {
            if let Node::Target {
                name,
                destination: Some(destination),
            } = node
            {
                explicit
                    .entry(name.clone())
                    .or_default()
                    .insert(destination.clone());
            }
        });
        for_each_inline_list(&doc.nodes, &mut |list| {
            for node in list {
                if let InlineNode::Hyperlink {
                    text,
                    target: HyperlinkTarget::Embedded(destination),
                    ..
                } = node
                {
                    explicit
                        .entry(TargetName::new(text))
                        .or_default()
                        .insert(destination.clone());
                }
            }
        });
        for (name, doc_path) in &index.targets {
            if *doc_path == doc.path {
                let anchor = format!("#{}", index.target_anchor(name));
                explicit
                    .entry(name.clone())
                    .or_default()
                    .insert(LinkDestination::Uri(anchor));
            }
        }

        let mut implicit: BTreeMap<TargetName, BTreeSet<LinkDestination>> = BTreeMap::new();
        for (node_index, id) in section_ids {
            if let Some(Node::Heading { text, .. }) = doc.nodes.get(*node_index) {
                implicit
                    .entry(TargetName::new(&inline_plain_text(text)))
                    .or_default()
                    .insert(LinkDestination::Uri(format!("#{}", id.as_str())));
            }
        }
        Self { explicit, implicit }
    }

    /// The href a reference to `name` in this document links, following
    /// aliases, or `None` when the document defines no single target of that
    /// name — or an alias chain leads back on itself.
    pub(crate) fn href(&self, name: &TargetName) -> Option<&str> {
        let mut visited = BTreeSet::new();
        let mut current = name;
        loop {
            if !visited.insert(current) {
                return None;
            }
            match self.destination_of(current)? {
                LinkDestination::Uri(uri) => return Some(uri),
                LinkDestination::Name(next) => current = next,
            }
        }
    }

    /// The href `destination` — written in a reference or a target of this
    /// document — links.
    pub(crate) fn destination_href<'a>(
        &'a self,
        destination: &'a LinkDestination,
    ) -> Option<&'a str> {
        match destination {
            LinkDestination::Uri(uri) => Some(uri),
            LinkDestination::Name(name) => self.href(name),
        }
    }

    /// The one destination this document gives `name`: an explicit target's,
    /// else a section's.
    fn destination_of(&self, name: &TargetName) -> Option<&LinkDestination> {
        match self.explicit.get(name) {
            Some(destinations) => only(destinations),
            None => self.implicit.get(name).and_then(only),
        }
    }
}

/// The one destination in `destinations`, or `None` when there are several.
fn only(destinations: &BTreeSet<LinkDestination>) -> Option<&LinkDestination> {
    match destinations.len() {
        1 => destinations.first(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::{Directive, ListItem, allocate_section_ids};

    fn external(name: &str, uri: &str) -> Node {
        Node::Target {
            name: TargetName::new(name),
            destination: Some(LinkDestination::Uri(uri.to_string())),
        }
    }

    fn alias(name: &str, of: &str) -> Node {
        Node::Target {
            name: TargetName::new(name),
            destination: Some(LinkDestination::Name(TargetName::new(of))),
        }
    }

    fn embedded(text: &str, destination: LinkDestination) -> Node {
        Node::Paragraph(vec![InlineNode::Hyperlink {
            text: text.to_string(),
            target: HyperlinkTarget::Embedded(destination),
            span: None,
        }])
    }

    fn heading(title: &str) -> Node {
        Node::Heading {
            level: 1,
            text: vec![InlineNode::Text(title.to_string())],
        }
    }

    /// The targets of a document at `page.rst` holding `nodes`, against
    /// `index`.
    fn collect(nodes: Vec<Node>, index: &ProjectIndex) -> DocumentHyperlinkTargets {
        let doc = Document::new("page.rst".to_string(), nodes);
        let section_ids = allocate_section_ids(&doc.nodes);
        DocumentHyperlinkTargets::collect(&doc, index, &section_ids)
    }

    fn href(targets: &DocumentHyperlinkTargets, name: &str) -> Option<String> {
        targets.href(&TargetName::new(name)).map(str::to_string)
    }

    #[test]
    fn test_collect_finds_an_external_target_nested_in_a_container() {
        // Given
        let nodes = vec![Node::BulletList {
            bullet: '-',
            items: vec![ListItem {
                nodes: vec![external("Python", "https://python.org")],
            }],
        }];

        // When
        let targets = collect(nodes, &ProjectIndex::default());

        // Then
        assert_eq!(
            href(&targets, "python").as_deref(),
            Some("https://python.org")
        );
    }

    #[test]
    fn test_collect_anchors_an_internal_target_of_this_document() {
        // Given — a label, and an entity, whose anchor is not its name.
        let mut index = ProjectIndex::default();
        index
            .targets
            .insert(TargetName::new("intro"), "page.rst".to_string());
        index
            .targets
            .insert(TargetName::new("REQ_1"), "page.rst".to_string());
        index
            .target_anchors
            .insert(TargetName::new("REQ_1"), "entity-REQ_1".to_string());

        // When
        let targets = collect(Vec::new(), &index);

        // Then
        assert_eq!(href(&targets, "intro").as_deref(), Some("#intro"));
        assert_eq!(href(&targets, "req_1").as_deref(), Some("#entity-REQ_1"));
    }

    #[test]
    fn test_collect_leaves_out_a_label_of_another_document() {
        // Given — docutils resolves `name_` within its own document; another
        // document's label is what `:ref:` is for.
        let mut index = ProjectIndex::default();
        index
            .targets
            .insert(TargetName::new("constants"), "other.rst".to_string());

        // When
        let targets = collect(Vec::new(), &index);

        // Then
        assert_eq!(href(&targets, "constants"), None);
    }

    #[test]
    fn test_collect_makes_a_section_title_an_implicit_target() {
        // Given
        let nodes = vec![heading("Getting Started")];

        // When
        let targets = collect(nodes, &ProjectIndex::default());

        // Then — the heading's own id, as the heading renders it.
        assert_eq!(
            href(&targets, "getting started").as_deref(),
            Some("#getting-started")
        );
    }

    #[test]
    fn test_href_prefers_an_explicit_target_over_a_section_title() {
        // Given
        let nodes = vec![heading("Python"), external("Python", "https://python.org")];

        // When
        let targets = collect(nodes, &ProjectIndex::default());

        // Then
        assert_eq!(
            href(&targets, "python").as_deref(),
            Some("https://python.org")
        );
    }

    #[test]
    fn test_href_is_none_for_two_sections_of_one_title() {
        // Given — docutils drops a duplicate implicit name.
        let nodes = vec![heading("Usage"), heading("Usage")];

        // When
        let targets = collect(nodes, &ProjectIndex::default());

        // Then
        assert_eq!(href(&targets, "usage"), None);
    }

    #[test]
    fn test_href_is_none_for_a_name_given_two_urls_in_either_order() {
        // Given
        let forwards = vec![
            external("guide", "https://a.org"),
            external("guide", "https://b.org"),
        ];
        let backwards = vec![
            external("guide", "https://b.org"),
            external("guide", "https://a.org"),
        ];

        // When / Then
        for nodes in [forwards, backwards] {
            let targets = collect(nodes, &ProjectIndex::default());
            assert_eq!(href(&targets, "guide"), None);
        }
    }

    #[test]
    fn test_href_is_none_for_a_name_both_external_and_internal() {
        // Given — one namespace for explicit targets, as in docutils.
        let mut index = ProjectIndex::default();
        index
            .targets
            .insert(TargetName::new("guide"), "page.rst".to_string());

        // When
        let targets = collect(vec![external("guide", "https://a.org")], &index);

        // Then
        assert_eq!(href(&targets, "guide"), None);
    }

    #[test]
    fn test_href_accepts_a_name_repeated_with_the_same_url() {
        // Given
        let nodes = vec![
            external("guide", "https://a.org"),
            Node::Directive(Directive::SeeAlso {
                body: vec![external("guide", "https://a.org")],
            }),
        ];

        // When
        let targets = collect(nodes, &ProjectIndex::default());

        // Then
        assert_eq!(href(&targets, "guide").as_deref(), Some("https://a.org"));
    }

    #[test]
    fn test_href_follows_an_indirect_target() {
        // Given — `tkinter.ttk.rst`'s `.. _Layout: `Layouts`_`.
        let nodes = vec![
            alias("Layout", "Layouts"),
            external("Layouts", "https://tkdocs.com/layouts"),
        ];

        // When
        let targets = collect(nodes, &ProjectIndex::default());

        // Then
        assert_eq!(
            href(&targets, "layout").as_deref(),
            Some("https://tkdocs.com/layouts")
        );
    }

    #[test]
    fn test_href_is_none_for_an_alias_cycle() {
        // Given
        let nodes = vec![alias("a", "b"), alias("b", "a")];

        // When
        let targets = collect(nodes, &ProjectIndex::default());

        // Then
        assert_eq!(href(&targets, "a"), None);
    }

    #[test]
    fn test_collect_makes_an_embedded_references_text_a_target() {
        // Given — `` `Python <https://python.org>`_ ``, later `Python`_.
        let nodes = vec![embedded(
            "Python",
            LinkDestination::Uri("https://python.org".to_string()),
        )];

        // When
        let targets = collect(nodes, &ProjectIndex::default());

        // Then
        assert_eq!(
            href(&targets, "python").as_deref(),
            Some("https://python.org")
        );
    }

    #[test]
    fn test_destination_href_resolves_an_alias_within_the_document() {
        // Given — `` `isolation <interp-isolation_>`_ `` in
        // `concurrent.interpreters.rst`, naming a label of the same page.
        let mut index = ProjectIndex::default();
        index
            .targets
            .insert(TargetName::new("interp-isolation"), "page.rst".to_string());
        let targets = collect(Vec::new(), &index);
        let destination = LinkDestination::Name(TargetName::new("interp-isolation"));

        // When / Then
        assert_eq!(
            targets.destination_href(&destination),
            Some("#interp-isolation")
        );
        assert_eq!(
            targets.destination_href(&LinkDestination::Uri("#usage".to_string())),
            Some("#usage")
        );
    }
}

//! Where a cross-reference leads, asked without rendering a page — what the
//! language server's hover shows, and what its go-to-definition opens.
//!
//! The answer comes from the same resolution each role's HTML writer uses
//! (`inline/target.rs` dispatches to the role modules), so a reference that
//! links on the page has a target here, and one that is broken on the page
//! has none: the editor cannot describe a link the build would not write.
//! A `.. toctree::` entry is answered from the toctree's own expansion
//! (`nav/expand.rs`), so it leads where the page's navigation links.

use rinx_ast::{Document, EntityId, InlineNode, TocEntry};
use rinx_entity::EntitySchema;
use rinx_index::{DefinitionKey, ExternalInventory, ExternalTarget, ProjectIndex};
use rinx_scope::DocumentScopes;

use crate::config::SiteConfig;
use crate::numbering::Numbering;
use crate::resolution::{DomainObjectResolver, EntityResolver, OptionResolver};

/// What a cross-reference resolved to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceTarget {
    /// What the target is called: a section's title, a document's title, an
    /// object's qualified name, a figure's number — what a reference written
    /// without an explicit title shows, whatever this one was written with.
    pub title: String,
    pub destination: Destination,
    /// The entity the target is, when it is one: what an entity role names,
    /// and a label or `:any:` leading to an entity — so a reader can show
    /// the entity's fields beside the link.
    pub entity: Option<EntityId>,
    /// The definition the target is, by the key the index names it under —
    /// what the document defining it recorded the site of, so an editor can
    /// open the line it was written on. `None` for a document itself, and
    /// for every target that is no document of this site.
    pub definition: Option<DefinitionKey>,
}

/// Where a [`ReferenceTarget`] is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Destination {
    /// A document of this site, by its `.rst` path, and the anchor on its
    /// page — `None` for the page itself.
    Document {
        doc_path: String,
        anchor: Option<String>,
    },
    /// A page the build writes that no document is (`genindex.html`), by its
    /// path from the site root.
    GeneratedPage { path: String },
    /// Another site, through its inventory: the address the page links, and
    /// the tooltip naming the site, `(in Python v3.12)`.
    External { url: String, site: String },
}

impl ReferenceTarget {
    /// A target in the document `doc_path`, at `anchor` on its page.
    pub(crate) fn in_document(
        title: impl Into<String>,
        doc_path: &str,
        anchor: Option<String>,
    ) -> Self {
        Self {
            title: title.into(),
            destination: Destination::Document {
                doc_path: doc_path.to_string(),
                anchor,
            },
            entity: None,
            definition: None,
        }
    }

    /// This target, as the entity `id`.
    pub(crate) fn naming_entity(self, id: EntityId) -> Self {
        Self {
            entity: Some(id),
            ..self
        }
    }

    /// This target, as the definition the index names `key`.
    pub(crate) fn defined_as(self, key: DefinitionKey) -> Self {
        Self {
            definition: Some(key),
            ..self
        }
    }

    /// A target another site's `inventory` lists, as the page at `doc_path`
    /// links it, titled `title` — or by the inventory when that is `None`.
    pub(crate) fn external(
        title: Option<&str>,
        inventory: &ExternalInventory,
        target: &ExternalTarget,
        doc_path: &str,
    ) -> Self {
        let hit = crate::resolution::ExternalHit { inventory, target };
        Self {
            title: title.unwrap_or_else(|| target.display_text()).to_string(),
            destination: Destination::External {
                url: hit.href(doc_path),
                site: hit.tooltip(),
            },
            entity: None,
            definition: None,
        }
    }
}

/// Answers where the references of one document lead, against one project
/// index — holding what the renderer holds for a page, so each answer is
/// the one rendering that page would give.
pub struct ReferenceResolver<'a> {
    pub(crate) index: &'a ProjectIndex,
    pub(crate) doc_path: &'a str,
    pub(crate) domains: DomainObjectResolver<'a>,
    pub(crate) options: OptionResolver<'a>,
    pub(crate) entities: EntityResolver<'a>,
    pub(crate) numbering: Numbering<'a>,
    pub(crate) scopes: DocumentScopes<'a>,
}

impl<'a> ReferenceResolver<'a> {
    /// A resolver for the references of `doc`, against `index`, under the
    /// site's `config` and entity `schema`.
    #[must_use]
    pub fn new(
        doc: &'a Document,
        index: &'a ProjectIndex,
        config: &'a SiteConfig,
        schema: &'a EntitySchema,
    ) -> Self {
        Self {
            index,
            doc_path: &doc.path,
            domains: DomainObjectResolver::new(index),
            options: OptionResolver::new(index),
            entities: EntityResolver::new(index, schema),
            numbering: Numbering::new(doc, index, config),
            scopes: DocumentScopes::of(&doc.nodes),
        }
    }

    /// Where `reference`, an inline node of this resolver's document, leads —
    /// or `None` when it is no cross-reference resolved against the index,
    /// was written with `!` to suppress its link, or is broken.
    #[must_use]
    pub fn resolve(&self, reference: &InlineNode) -> Option<ReferenceTarget> {
        crate::inline::reference_target(reference, self)
    }

    /// Where `reference` would lead had it named `target` instead — written
    /// the same way, with the same role, prefixes and search order, and in
    /// the scope `reference` is written in — or `None` as for
    /// [`Self::resolve`], and for an inline node that names no target.
    ///
    /// What an editor asks of each target it might complete a reference
    /// with, so it offers only those the page would link, spelled as they
    /// lead there from where the reference is written. A reference resolved
    /// in a scope — a domain role, `:option:`, `:any:` — must be a node of
    /// this resolver's document, as for [`Self::resolve`]; any other may be
    /// one built for the question.
    #[must_use]
    pub fn resolve_as(&self, reference: &InlineNode, target: &str) -> Option<ReferenceTarget> {
        let renamed = renamed(reference, target)?;
        match reference {
            InlineNode::DomainObjectReference { .. }
            | InlineNode::OptionReference { .. }
            | InlineNode::AnyReference { .. } => crate::inline::scoped_reference_target(
                &renamed,
                self.scopes.scope_at(reference),
                self,
            ),
            _ => crate::inline::reference_target(&renamed, self),
        }
    }

    /// The documents `entry`, an entry of a `.. toctree::` in this
    /// resolver's document, links — each under the title its navigation
    /// entry shows when the entry writes none: one for a document or `self`, every
    /// match for a glob, none for an external link or a document this site
    /// does not have.
    #[must_use]
    pub fn toc_entry_targets(&self, entry: &TocEntry) -> Vec<ReferenceTarget> {
        crate::nav::entry_documents(entry, self.doc_path, self.index)
            .into_iter()
            .map(|doc_path| {
                ReferenceTarget::in_document(
                    crate::nav::document_title(&doc_path, None, self.index),
                    &doc_path,
                    None,
                )
            })
            .collect()
    }
}

/// `reference` naming `target` instead, shown as `target` where its role
/// shows the name written — or `None` when it is no reference to a target.
fn renamed(reference: &InlineNode, target: &str) -> Option<InlineNode> {
    let mut renamed = reference.clone();
    match &mut renamed {
        InlineNode::Reference { target: named, .. }
        | InlineNode::DocReference { target: named, .. }
        | InlineNode::AnyReference { target: named, .. }
        | InlineNode::NumberReference { target: named, .. }
        | InlineNode::EntityReference { target: named, .. } => *named = target.to_string(),
        InlineNode::EquationReference { label, .. } => *label = target.to_string(),
        InlineNode::TermReference { display, term, .. } => {
            *display = target.to_string();
            *term = target.to_string();
        }
        InlineNode::OptionReference {
            display,
            target: named,
            ..
        } => {
            *display = target.to_string();
            *named = target.to_string();
        }
        InlineNode::DomainObjectReference { name, display, .. } => {
            *display = target.to_string();
            *name = target.to_string();
        }
        _ => return None,
    }
    Some(renamed)
}

#[cfg(test)]
mod tests {
    use rinx_ast::{
        Directive, InventorySelector, Node, ObjectType, PyObjectType, TargetName, TargetSearchOrder,
    };

    use super::*;

    fn function_reference(name: &str) -> InlineNode {
        InlineNode::DomainObjectReference {
            object_type: ObjectType::Py(PyObjectType::Function),
            name: name.to_string(),
            display: name.to_string(),
            link: true,
            search_order: TargetSearchOrder::default(),
            span: None,
            inventory: InventorySelector::Any,
        }
    }

    /// A document under `.. py:currentmodule:: pkg`, holding `reference`.
    fn in_module_pkg(reference: InlineNode) -> Document {
        Document::new(
            "guide.rst".to_string(),
            vec![
                Node::Directive(Directive::PyCurrentModule {
                    module: Some("pkg".to_string()),
                }),
                Node::Paragraph(vec![reference]),
            ],
        )
    }

    /// The one inline node of `doc`'s last paragraph.
    fn written(doc: &Document) -> &InlineNode {
        let Some(Node::Paragraph(inlines)) = doc.nodes.last() else {
            unreachable!()
        };
        &inlines[0]
    }

    #[test]
    fn test_resolve_as_resolves_another_name_in_the_reference_s_scope() {
        // Given — `run` is `pkg.run` only inside module `pkg`
        let mut index = ProjectIndex::default();
        index.insert_domain_object(ObjectType::Py(PyObjectType::Function), "pkg.run", "api.rst");
        let doc = in_module_pkg(function_reference("x"));
        let config = SiteConfig::default();
        let resolver = ReferenceResolver::new(&doc, &index, &config, EntitySchema::empty_ref());

        // When
        let short = resolver.resolve_as(written(&doc), "run");
        let full = resolver.resolve_as(written(&doc), "pkg.run");

        // Then
        assert_eq!(short.as_ref().map(|t| t.title.as_str()), Some("pkg.run"));
        assert_eq!(short, full);
        assert_eq!(resolver.resolve_as(written(&doc), "other"), None);
    }

    #[test]
    fn test_resolve_as_resolves_a_label_reference_built_for_the_question() {
        // Given — a reference no document holds
        let mut index = ProjectIndex::default();
        index
            .targets
            .insert(TargetName::new("install"), "setup.rst".to_string());
        let doc = Document::new("guide.rst".to_string(), Vec::new());
        let config = SiteConfig::default();
        let resolver = ReferenceResolver::new(&doc, &index, &config, EntitySchema::empty_ref());
        let reference = InlineNode::Reference {
            display: None,
            target: "x".to_string(),
            span: None,
            inventory: InventorySelector::Any,
        };

        // When
        let found = resolver.resolve_as(&reference, "install");

        // Then
        assert!(matches!(
            found,
            Some(ReferenceTarget {
                destination: Destination::Document { ref doc_path, .. },
                ..
            }) if doc_path == "setup.rst"
        ));
    }

    #[test]
    fn test_toc_entry_targets_lead_to_each_document_by_its_title() {
        // Given — a glob matching two titled documents
        let mut index = ProjectIndex::default();
        for (doc_path, title) in [("api/a.rst", "Alpha"), ("api/b.rst", "Beta")] {
            index
                .document_titles
                .insert(doc_path.to_string(), title.to_string());
        }
        let doc = Document::new("index.rst".to_string(), Vec::new());
        let config = SiteConfig::default();
        let resolver = ReferenceResolver::new(&doc, &index, &config, EntitySchema::empty_ref());
        let entry = TocEntry::Glob {
            pattern: "api/*".to_string(),
            span: None,
        };

        // When
        let targets = resolver.toc_entry_targets(&entry);

        // Then
        assert_eq!(
            targets,
            vec![
                ReferenceTarget::in_document("Alpha", "api/a.rst", None),
                ReferenceTarget::in_document("Beta", "api/b.rst", None),
            ]
        );
    }

    #[test]
    fn test_toc_entry_targets_are_empty_for_a_missing_document() {
        // Given
        let index = ProjectIndex::default();
        let doc = Document::new("index.rst".to_string(), Vec::new());
        let config = SiteConfig::default();
        let resolver = ReferenceResolver::new(&doc, &index, &config, EntitySchema::empty_ref());
        let entry = TocEntry::Document {
            title: None,
            docname: "gone".to_string(),
            span: None,
        };

        // When / Then
        assert!(resolver.toc_entry_targets(&entry).is_empty());
    }

    #[test]
    fn test_renamed_names_the_target_and_shows_it() {
        // When
        let renamed = renamed(&function_reference("x"), "pkg.run");

        // Then
        assert_eq!(renamed, Some(function_reference("pkg.run")));
    }

    #[test]
    fn test_renamed_is_none_for_no_reference() {
        // When / Then
        assert_eq!(renamed(&InlineNode::Text("x".to_string()), "y"), None);
    }
}

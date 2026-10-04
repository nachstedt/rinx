//! Where a cross-reference leads, asked without rendering a page — what the
//! language server's hover shows, and what its go-to-definition opens.
//!
//! The answer comes from the same resolution each role's HTML writer uses
//! (`inline/target.rs` dispatches to the role modules), so a reference that
//! links on the page has a target here, and one that is broken on the page
//! has none: the editor cannot describe a link the build would not write.

use rinx_ast::{Document, InlineNode};
use rinx_entity::EntitySchema;
use rinx_index::{ExternalInventory, ExternalTarget, ProjectIndex};
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
}

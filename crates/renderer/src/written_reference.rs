//! What a document wrote that leads somewhere in the project — its
//! cross-references and `.. toctree::` entries — owned, so a reader can keep
//! them without the document and resolve them later, against whatever the
//! project index has become.
//!
//! Where a reference leads is a question of the whole project (a label two
//! documents define leads nowhere, a domain role searches every scope), so a
//! document can only record what it *wrote*. [`UseResolver`] answers where
//! each use leads exactly as [`ReferenceResolver`] would for the page: it is
//! one, holding no page — every scope a resolution needs travels with the
//! reference instead. That is what an editor's "find all references" asks of
//! each document it holds no parse of.

use rinx_ast::{
    Child, Directive, Document, InlineNode, Node, Span, TocEntry, for_each_child, walk_nodes,
};
use rinx_entity::EntitySchema;
use rinx_index::ProjectIndex;
use rinx_scope::{DocumentScopes, Scope};

use crate::config::SiteConfig;
use crate::numbering::Numbering;
use crate::resolution::{DomainObjectResolver, EntityResolver, OptionResolver};
use crate::{ReferenceResolver, ReferenceTarget};

/// One place a document leads into the project from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WrittenUse {
    /// A cross-reference resolved against the project index.
    Reference {
        node: InlineNode,
        /// The scope it is written in, for a reference resolved in one — a
        /// domain role, `:option:` or `:any:` — and `None` for every other.
        scope: Option<Scope>,
        span: Span,
    },
    /// An entry of a `.. toctree::`.
    TocEntry { entry: TocEntry, span: Span },
}

impl WrittenUse {
    /// Where it was written — in an `.. include::`d fragment when the span
    /// names a file.
    #[must_use]
    pub const fn span(&self) -> Span {
        match self {
            Self::Reference { span, .. } | Self::TocEntry { span, .. } => *span,
        }
    }
}

/// Every cross-reference and `.. toctree::` entry written in `doc` with a
/// span, in document order — those spliced in from an `.. include::`d
/// fragment included, since the including document resolves them.
///
/// Found by the walk the language server finds the one under the cursor by,
/// so "every use" and "the use here" cover the same nodes.
#[must_use]
pub fn written_uses(doc: &Document) -> Vec<WrittenUse> {
    let scopes = DocumentScopes::of(&doc.nodes);
    let mut uses = Vec::new();
    walk_nodes(&doc.nodes, &mut |node| {
        if let Node::Directive(Directive::Toctree(toctree)) = node {
            uses.extend(toctree.entries.iter().filter_map(|entry| {
                Some(WrittenUse::TocEntry {
                    entry: entry.clone(),
                    span: entry.span()?,
                })
            }));
        }
        for_each_child(node, &mut |child| {
            if let Child::Inline(inlines) = child {
                uses.extend(
                    inlines
                        .iter()
                        .filter_map(|inline| written_reference(inline, &scopes)),
                );
            }
        });
    });
    uses
}

/// `inline` as a use, when it is a cross-reference with a span.
fn written_reference(inline: &InlineNode, scopes: &DocumentScopes<'_>) -> Option<WrittenUse> {
    let scope = match inline {
        InlineNode::DomainObjectReference { .. }
        | InlineNode::OptionReference { .. }
        | InlineNode::AnyReference { .. } => Some(scopes.scope_at(inline).clone()),
        InlineNode::Reference { .. }
        | InlineNode::DocReference { .. }
        | InlineNode::TermReference { .. }
        | InlineNode::NumberReference { .. }
        | InlineNode::EquationReference { .. }
        | InlineNode::EntityReference { .. } => None,
        _ => return None,
    };
    Some(WrittenUse::Reference {
        node: inline.clone(),
        scope,
        span: inline.span()?,
    })
}

/// Answers where the uses one document wrote lead, against one project
/// index, as rendering that document's page would.
pub struct UseResolver<'a> {
    resolver: ReferenceResolver<'a>,
}

impl<'a> UseResolver<'a> {
    /// A resolver for the uses the document `doc_path` wrote, against
    /// `index`, under the site's `config` and entity `schema`.
    #[must_use]
    pub fn new(
        doc_path: &'a str,
        index: &'a ProjectIndex,
        config: &'a SiteConfig,
        schema: &'a EntitySchema,
    ) -> Self {
        Self {
            resolver: ReferenceResolver {
                index,
                doc_path,
                domains: DomainObjectResolver::new(index),
                options: OptionResolver::new(index),
                entities: EntityResolver::new(index, schema),
                numbering: Numbering::without_captions(config),
                scopes: DocumentScopes::of(&[]),
            },
        }
    }

    /// Where `written` leads: what [`ReferenceResolver::resolve`] answers for
    /// a reference — nothing for a broken one — and what
    /// [`ReferenceResolver::toc_entry_targets`] answers for an entry.
    #[must_use]
    pub fn targets(&self, written: &WrittenUse) -> Vec<ReferenceTarget> {
        match written {
            WrittenUse::Reference {
                node,
                scope: Some(scope),
                ..
            } => crate::inline::scoped_reference_target(node, scope, &self.resolver)
                .into_iter()
                .collect(),
            WrittenUse::Reference {
                node, scope: None, ..
            } => crate::inline::reference_target(node, &self.resolver)
                .into_iter()
                .collect(),
            WrittenUse::TocEntry { entry, .. } => self.resolver.toc_entry_targets(entry),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::{FileId, ObjectType, Position, PyObjectType, TargetName};

    /// The spans' first lines of every use written in `text`.
    fn use_lines(text: &str) -> Vec<u32> {
        written_uses(&rinx_parser::parse("index.rst", text))
            .iter()
            .map(|written| written.span().start.line)
            .collect()
    }

    #[test]
    fn test_written_uses_lists_every_reference_in_document_order() {
        // Given
        let text = "See :ref:`install` and :doc:`setup`.\n\n.. note::\n\n   And :term:`API`.\n";

        // When / Then
        assert_eq!(use_lines(text), vec![1, 1, 5]);
    }

    #[test]
    fn test_written_uses_skips_what_leads_outside_the_index() {
        // Given — a hyperlink, a download and a literal
        let text = "`site <https://example.org>`_ :download:`a.zip` ``ref``\n";

        // When / Then
        assert!(use_lines(text).is_empty());
    }

    #[test]
    fn test_written_uses_lists_each_toctree_entry() {
        // Given
        let text = ".. toctree::\n   :maxdepth: 2\n\n   intro\n   Setup <setup>\n";

        // When
        let uses = written_uses(&rinx_parser::parse("index.rst", text));

        // Then
        assert!(
            uses.iter()
                .all(|written| matches!(written, WrittenUse::TocEntry { .. }))
        );
        assert_eq!(use_lines(text), vec![4, 5]);
    }

    #[test]
    fn test_written_uses_keeps_the_scope_of_a_scoped_reference_only() {
        // Given
        let text = ".. py:currentmodule:: pkg\n\n:py:func:`run` and :ref:`install`\n";

        // When
        let uses = written_uses(&rinx_parser::parse("index.rst", text));

        // Then
        let mut pkg = Scope::default();
        pkg.python.set_module("pkg");
        let scopes: Vec<Option<Scope>> = uses
            .into_iter()
            .map(|written| match written {
                WrittenUse::Reference { scope, .. } => scope,
                WrittenUse::TocEntry { .. } => None,
            })
            .collect();
        assert_eq!(scopes, vec![Some(pkg), None]);
    }

    #[test]
    fn test_written_uses_keeps_the_file_of_a_reference_spliced_in() {
        // Given — a span counting lines in an included fragment
        let mut document = rinx_parser::parse("index.rst", "See :ref:`install`.\n");
        let Node::Paragraph(inlines) = &mut document.nodes[0] else {
            unreachable!()
        };
        let span = inlines[1].span().unwrap();
        inlines[1] = inlines[1]
            .clone()
            .with_span(Some(span.with_file(Some(FileId::new(0)))));

        // When
        let uses = written_uses(&document);

        // Then
        assert_eq!(uses.len(), 1);
        assert_eq!(uses[0].span().file, Some(FileId::new(0)));
    }

    /// An index defining the function `pkg.run` in `api.rst`, the function
    /// `other.run` there too, and the label `install` in `setup.rst`.
    fn index() -> ProjectIndex {
        let mut index = ProjectIndex::default();
        index.insert_domain_object(ObjectType::Py(PyObjectType::Function), "pkg.run", "api.rst");
        index.insert_domain_object(
            ObjectType::Py(PyObjectType::Function),
            "other.run",
            "api.rst",
        );
        index
            .targets
            .insert(TargetName::new("install"), "setup.rst".to_string());
        index
            .document_titles
            .insert("setup.rst".to_string(), "Setup".to_string());
        index
    }

    #[test]
    fn test_targets_are_what_the_page_s_resolver_answers_for_every_use() {
        // Given — references in two module scopes, a label, a document, a
        // toctree entry and a broken reference
        let text = ".. py:currentmodule:: pkg\n\n:py:func:`run` :ref:`install`\n\n\
                    .. py:currentmodule:: other\n\n:py:func:`run` :doc:`setup` :ref:`ghost`\n\n\
                    .. toctree::\n\n   setup\n";
        let document = rinx_parser::parse("index.rst", text);
        let index = index();
        let config = SiteConfig::default();
        let schema = EntitySchema::empty_ref();
        let page = ReferenceResolver::new(&document, &index, &config, schema);
        let uses_resolver = UseResolver::new("index.rst", &index, &config, schema);

        // When
        let uses = written_uses(&document);

        // Then
        let mut references = Vec::new();
        walk_nodes(&document.nodes, &mut |node| {
            for_each_child(node, &mut |child| {
                if let Child::Inline(inlines) = child {
                    references.extend(inlines.iter().filter(|inline| inline.span().is_some()));
                }
            });
        });
        let (resolved, entries): (Vec<_>, Vec<_>) = uses
            .iter()
            .partition(|written| matches!(written, WrittenUse::Reference { .. }));
        assert_eq!(resolved.len(), 5);
        for (written, reference) in resolved.iter().zip(&references) {
            assert_eq!(
                uses_resolver.targets(written),
                page.resolve(reference).into_iter().collect::<Vec<_>>()
            );
        }
        let titles: Vec<String> = resolved
            .iter()
            .flat_map(|written| uses_resolver.targets(written))
            .map(|target| target.title)
            .collect();
        assert_eq!(titles, vec!["pkg.run", "install", "other.run", "Setup"]);
        assert_eq!(
            entries
                .iter()
                .flat_map(|written| uses_resolver.targets(written))
                .map(|target| target.title)
                .collect::<Vec<_>>(),
            vec!["Setup"]
        );
    }

    #[test]
    fn test_span_is_where_the_use_was_written() {
        // Given
        let uses = written_uses(&rinx_parser::parse("index.rst", "See :ref:`install`.\n"));

        // When / Then
        assert_eq!(uses[0].span().start, Position::new(1, 5));
    }
}

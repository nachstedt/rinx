use rinx_ast::{Directive, Document, IndexEntry, Node, TableRow, TargetName};
use rinx_index::{
    EntityUpdateRecord, EquationLocation, GenIndexEntry, ProjectIndex, TargetLocation,
};
use rinx_scope::Scope;

use super::domain_object_index::index_domain_object;
use super::equation_numbering::number_equations;
use super::outline::build_document_outline;

/// Analyzes a single `Document` and returns a local `ProjectIndex`.
///
/// This records the document itself and extracts targets, document titles, the document's section outline,
/// glossary terms and equation numbers. The `nav_tree` is not
/// populated here — it is built globally by [`build_project_index()`].
#[must_use]
pub fn analyze(doc: &Document) -> ProjectIndex {
    let mut index = ProjectIndex::default();
    index.documents.insert(doc.path.clone());
    let mut found_title = false;
    for node in &doc.nodes {
        if !found_title && let Node::Heading { level: 1, text } = node {
            index
                .document_titles
                .insert(doc.path.clone(), rinx_ast::inline_plain_text(text));
            found_title = true;
        }
    }
    let analysis = build_document_outline(&doc.nodes);
    if !analysis.outline.is_empty() {
        index
            .document_outlines
            .insert(doc.path.clone(), analysis.outline);
    }

    // Each toctree is paired with the section it was written inside, which the
    // outline pass computed in the same walk — the renderer needs both to put
    // a toctree's entries where the author wrote the directive.
    let toctrees: Vec<rinx_index::DocumentToctree> = doc
        .nodes
        .iter()
        .filter_map(|node| match node {
            Node::Directive(Directive::Toctree(toctree)) => Some(toctree.clone()),
            _ => None,
        })
        .zip(
            analysis
                .toctree_sections
                .into_iter()
                .chain(std::iter::repeat(None)),
        )
        .map(|(toctree, section)| rinx_index::DocumentToctree { toctree, section })
        .collect();
    if !toctrees.is_empty() {
        index.toctrees.insert(doc.path.clone(), toctrees);
    }

    index_nodes(&doc.nodes, &doc.path, &mut index, &mut Scope::default());
    for (label, number) in number_equations(doc) {
        index
            .equations
            .insert(label, EquationLocation::new(doc.path.clone(), number));
    }
    index
}

/// Records a `.. index::` directive's terms for the general index page.
fn index_genindex_entries(
    entries: &[IndexEntry],
    id: &str,
    doc_path: &str,
    index: &mut ProjectIndex,
) {
    for entry in entries {
        if let IndexEntry::Term {
            primary,
            subentry,
            main,
        } = entry
        {
            index.genindex_entries.push(GenIndexEntry {
                primary: primary.clone(),
                subentry: subentry.clone(),
                main: *main,
                doc_path: doc_path.to_string(),
                anchor: id.to_string(),
            });
        }
        // IndexEntry::See/SeeAlso redirect rather than link to content and are
        // not surfaced in genindex_entries yet (see docs/compatibility.rst).
    }
}

/// Records a `.. entity-update::`/`.. needextend::` so `apply_entity_updates`
/// can act on it once every document is merged, and indexes its body as
/// ordinary content belonging to this document — a target or entity written
/// in the justification is reached exactly as a dropdown's body is.
fn index_entity_update(
    update: &rinx_ast::EntityUpdate,
    doc_path: &str,
    index: &mut ProjectIndex,
    scope: &mut Scope,
) {
    index.entity_updates.push(EntityUpdateRecord {
        doc_path: doc_path.to_string(),
        update: update.clone(),
    });
    index_nodes(&update.body, doc_path, index, scope);
}

/// Records one entity, its link target, and everything inside its sections.
fn index_entity_and_its_sections(
    entity: &rinx_ast::EntityBody,
    doc_path: &str,
    index: &mut ProjectIndex,
    scope: &mut Scope,
) {
    super::entity_index::index_entity(entity, doc_path, index);
    // An entity is also an ordinary link target, so `:ref:` reaches one without
    // any role being declared — which is what makes a schema's roles optional
    // sugar rather than an obligation.
    let name = TargetName::new(entity.id.as_str());
    index
        .targets
        .insert(name.clone(), TargetLocation::Internal(doc_path.to_string()));
    // Its anchor is not its name, so a `:ref:` must be told where it is.
    index
        .target_anchors
        .insert(name, rinx_index::entity_anchor(entity.id.as_str()));
    // Every section is body content and may hold definitions of its own — a
    // target, a nested entity, a glossary. Recursing keeps this walk mirroring
    // `render_nodes`, as the module doc requires.
    for section in &entity.sections {
        index_nodes(&section.body, doc_path, index, scope);
    }
}

/// Recursively registers targets, glossary terms, and domain objects found
/// anywhere in `nodes`, including inside table cells, list items, and
/// directive bodies — not just at the document's top level. This mirrors
/// the recursion shape `render_nodes` (in the renderer crate) uses, since a
/// definition nested in a container still needs to be indexed for
/// cross-references to resolve, exactly like it's still rendered with a
/// working anchor.
///
/// `scope.python` carries the enclosing `py:class`/`py:exception` stack
/// (lexical, pushed/popped around a nested body — see
/// [`rinx_ast::DomainObjectBody::deduce_local_scope`], shared with
/// the renderer so index keys and anchor `id`s can't drift apart) and the
/// most recently seen `py:module` (document-order state, not lexical
/// nesting — real Sphinx docs write `py:module` and the functions/classes it
/// documents as *siblings*, not nested underneath it, so it is never popped
/// when returning from a nested body; a module stays "current" for the rest
/// of the document until another `py:module`, or `py:currentmodule`,
/// changes it). `scope.c` is the same idea for the `c` domain's
/// `c:struct`/`c:union` nesting — a wholly separate stack (see
/// [`rinx_scope::CScope`]'s doc comment for why it isn't a variant of
/// `PythonScope`); `c:function`/`c:macro` never touch it and keep qualifying
/// via `scope.python` exactly as before it existed.
pub(super) fn index_nodes(
    nodes: &[Node],
    doc_path: &str,
    index: &mut ProjectIndex,
    scope: &mut Scope,
) {
    record_target_titles(nodes, index);
    for node in nodes {
        match node {
            Node::Target { name, uri } => {
                let location = uri.as_ref().map_or_else(
                    || TargetLocation::Internal(doc_path.to_string()),
                    |url| TargetLocation::External(url.clone()),
                );
                index.targets.insert(name.clone(), location);
            }
            Node::Directive(Directive::Toctree(toctree)) => {
                // `:name:` makes the toctree itself a `:ref:` target. Recorded
                // here with every other per-document target so it merges the
                // same way — and registered even for a `:hidden:` toctree,
                // which still renders its anchor precisely so the reference
                // does not dangle.
                if let Some(name) = &toctree.options.name {
                    index
                        .targets
                        .insert(name.clone(), TargetLocation::Internal(doc_path.to_string()));
                }
            }
            Node::Directive(Directive::Entity(entity)) => {
                index_entity_and_its_sections(entity, doc_path, index, scope);
            }
            Node::Directive(Directive::EntityUpdate(update)) => {
                index_entity_update(update, doc_path, index, scope);
            }
            Node::Directive(Directive::Glossary { entries, .. }) => {
                for entry in entries {
                    for term in &entry.terms {
                        index
                            .glossary_terms
                            .insert(TargetName::new(term), doc_path.to_string());
                    }
                }
            }
            Node::Directive(Directive::Index { entries, id }) => {
                index_genindex_entries(entries, id, doc_path, index);
            }
            Node::Directive(Directive::DomainObject(obj)) => {
                index_domain_object(obj, doc_path, index, scope);
            }
            Node::Directive(
                directive @ (Directive::PyCurrentModule { .. }
                | Directive::CNamespace { .. }
                | Directive::CNamespacePush { .. }
                | Directive::CNamespacePop
                | Directive::StdProgram { .. }),
            ) => apply_scope_directive(directive, scope),
            Node::Directive(
                directive @ (Directive::Admonition { .. }
                | Directive::VersionChange { .. }
                | Directive::EntitySection { .. }
                | Directive::SeeAlso { .. }
                // A grid carries no `:name:` of its own, so unlike a
                // dropdown it reaches none of the name-bearing arms — but its
                // body is still this document's content, and a target,
                // section or entity written in a cell belongs to the
                // document exactly as if it had been written outside one.
                | Directive::Grid(_)
                | Directive::GridItem(_)),
            ) => index_body_only_directive(directive, doc_path, index, scope),
            Node::BulletList { items, .. } | Node::EnumeratedList { items, .. } => {
                for item in items {
                    index_nodes(&item.nodes, doc_path, index, scope);
                }
            }
            Node::DefinitionList { items } => {
                for item in items {
                    index_nodes(&item.definition, doc_path, index, scope);
                }
            }
            Node::Table {
                header_rows,
                body_rows,
            } => {
                index_table_rows(header_rows.iter().chain(body_rows), doc_path, index, scope);
            }
            Node::Directive(Directive::DataTable { rows, name, .. }) => {
                register_directive_name(name.as_ref(), doc_path, index);
                index_table_rows(rows, doc_path, index, scope);
            }
            Node::Directive(Directive::Table {
                header_rows,
                body_rows,
                name,
                ..
            }) => {
                register_directive_name(name.as_ref(), doc_path, index);
                index_table_rows(header_rows.iter().chain(body_rows), doc_path, index, scope);
            }
            Node::Directive(
                directive @ (Directive::CodeBlock(_)
                | Directive::Image(_)
                | Directive::Figure(_)
                | Directive::Contents(_)
                | Directive::Dropdown(_)
                | Directive::EntityTable(_)
                | Directive::EntityFlow(_)
                | Directive::EntitySequence(_)
                | Directive::EntityPie(_)
                | Directive::EntityBar(_)
                | Directive::Uml(_)),
            ) => index_name_bearing_directive(directive, doc_path, index, scope),
            Node::Directive(Directive::Sectnum(options)) => index_sectnum(options, doc_path, index),
            _ => {}
        }
    }
}

/// Records the title a `:ref:` with no explicit title shows for each target:
/// the heading an internal target is written directly above, or the caption
/// of a figure, table or code block — for the targets above it and for its
/// own `:name:` alike, as Sphinx takes them.
///
/// Follows docutils' `PropagateTargets`: consecutive targets chain onto the
/// same element, and anything else in between — including a comment, which
/// docutils counts as invisible and so will not move a target onto — ends
/// the chain. A target with a URI names a URL rather than what follows it,
/// so it ends the chain too. An element with no title leaves its targets
/// without one. Called once per node list by [`index_nodes`], which is what
/// makes an element inside a directive body count as well.
fn record_target_titles(nodes: &[Node], index: &mut ProjectIndex) {
    let mut pending: Vec<&TargetName> = Vec::new();
    for node in nodes {
        if let Node::Target { name, uri: None } = node {
            pending.push(name);
            continue;
        }
        let (own_name, title) = element_title(node);
        if let Some(title) = title {
            for name in pending.drain(..).chain(own_name) {
                index.target_titles.insert(name.clone(), title.clone());
            }
        }
        pending.clear();
    }
}

/// The title a `:ref:` to `node` shows, paired with the `:name:` the node
/// makes a target of itself — `(None, None)` for anything with neither.
fn element_title(node: &Node) -> (Option<&TargetName>, Option<String>) {
    match node {
        Node::Heading { text, .. } => (None, Some(rinx_ast::inline_plain_text(text))),
        Node::Directive(Directive::Figure(figure)) => (
            figure.image.name.as_ref(),
            figure.caption.as_deref().map(rinx_ast::inline_plain_text),
        ),
        Node::Directive(
            Directive::Table { title, name, .. } | Directive::DataTable { title, name, .. },
        ) => (name.as_ref(), title.clone()),
        Node::Directive(Directive::CodeBlock(block)) => {
            (block.name.as_ref(), block.caption.clone())
        }
        _ => (None, None),
    }
}

/// Recurses into the body of a directive that contributes nothing of its own
/// to the index, but whose content is still ordinary content belonging to
/// this document — a target, section or entity written inside one of these
/// must still be reached.
///
/// Split out of [`index_nodes`] for the same reason [`apply_scope_directive`]
/// is: these five arms were the bulk of what made that match too long. Any
/// other directive is a no-op here rather than a panic, for the same reason
/// `apply_scope_directive`'s catch-all is.
fn index_body_only_directive(
    directive: &Directive,
    doc_path: &str,
    index: &mut ProjectIndex,
    scope: &mut Scope,
) {
    let body = match directive {
        Directive::Admonition { body, .. }
        | Directive::VersionChange { body, .. }
        | Directive::EntitySection { body, .. }
        | Directive::SeeAlso { body } => body,
        Directive::Grid(grid) => &grid.body,
        Directive::GridItem(item) => &item.body,
        _ => return,
    };
    index_nodes(body, doc_path, index, scope);
}

/// Applies the directives that only move the traversal scope, indexing
/// nothing themselves.
///
/// Split out of [`index_nodes`] because they form one responsibility — the
/// same one the renderer keeps in its own `scope_directives` module — and
/// because their five arms are the bulk of what made that match unreadable.
/// Any other directive is a no-op here rather than a panic: the caller's match
/// decides which ones arrive, and duplicating that list would be a second
/// place to keep in step.
fn apply_scope_directive(directive: &Directive, scope: &mut Scope) {
    match directive {
        Directive::PyCurrentModule { module } => match module {
            Some(name) => scope.python.set_module(name),
            None => scope.python.clear_module(),
        },
        Directive::CNamespace { namespace } => scope.c.set_namespace(namespace.as_deref()),
        Directive::CNamespacePush { namespace } => scope.c.push_namespace(namespace),
        Directive::CNamespacePop => scope.c.pop_namespace(),
        Directive::StdProgram { name } => match name {
            Some(name) => scope.program.set(name),
            None => scope.program.clear(),
        },
        _ => {}
    }
}

/// Registers a directive's optional `:name:` as an internal cross-reference
/// target.
///
/// Shared by every directive with a `:name:` option — the two data-table
/// directives, `.. table::`, and the two code-block directives — since none of
/// them needs anything table- or code-specific to do it.
/// Indexes the directives whose whole contribution is the `:name:` a `:ref:`
/// can reach them by.
///
/// Split out of [`index_nodes`] for the same reason
/// [`rinx_ast::walk_nodes`] splits its directive arm out: three
/// near-identical arms make the node match harder to read than the one thing
/// they have in common. A figure and a dropdown additionally carry a body — a
/// legend and a dropdown's content are ordinary content, so anything
/// referenceable written there has to be indexed like any other body's. Any other directive is a no-op rather than a panic:
/// the caller's match decides which ones arrive, and duplicating that list
/// would be a second place to keep in step.
fn index_name_bearing_directive(
    directive: &Directive,
    doc_path: &str,
    index: &mut ProjectIndex,
    scope: &mut Scope,
) {
    match directive {
        Directive::CodeBlock(block) => {
            register_directive_name(block.name.as_ref(), doc_path, index);
        }
        Directive::Image(options) => {
            register_directive_name(options.name.as_ref(), doc_path, index);
        }
        Directive::Figure(figure) => {
            register_directive_name(figure.image.name.as_ref(), doc_path, index);
            index_nodes(&figure.legend, doc_path, index, scope);
        }
        Directive::Contents(contents) => {
            register_directive_name(contents.options.name.as_ref(), doc_path, index);
        }
        // A listing directive's `:name:` is all it contributes: its rows are
        // resolved from this very index while rendering, so there is nothing
        // here to index them from.
        Directive::EntityTable(table) => {
            register_directive_name(table.name.as_ref(), doc_path, index);
        }
        // A diagram's `:name:` is all it contributes, for the same reason a
        // listing directive's is: its picture is compiled by a build action
        // and its content, when templated, is resolved from this very index.
        Directive::Uml(uml) => {
            register_directive_name(uml.name.as_ref(), doc_path, index);
        }
        // A flowchart contributes its `:name:` and nothing else, being both of
        // the above at once: a question answered from this index, drawn as a
        // picture a build action compiles.
        Directive::EntityFlow(flow) => {
            register_directive_name(flow.name.as_ref(), doc_path, index);
        }
        // A sequence diagram contributes its `:name:` and nothing else, for the
        // flowchart's reason: its participants and messages are walked from
        // this very index while rendering.
        Directive::EntitySequence(sequence) => {
            register_directive_name(sequence.name.as_ref(), doc_path, index);
        }
        // A pie chart contributes its `:name:` and nothing else, for the
        // listing directive's reason: its wedges are counted from this very
        // index while rendering. Unlike a flowchart, no build action is
        // involved at all — the chart is drawn in the render itself.
        Directive::EntityPie(pie) => {
            register_directive_name(pie.name.as_ref(), doc_path, index);
        }
        // A bar chart likewise: its cells are counted from this index while
        // rendering, and nothing is compiled.
        Directive::EntityBar(bar) => {
            register_directive_name(bar.name.as_ref(), doc_path, index);
        }
        // A dropdown carries both: a `:name:` of its own, and a body whose
        // targets, sections and entities belong to this document exactly as
        // if they had been written outside it.
        Directive::Dropdown(dropdown) => {
            register_directive_name(dropdown.name.as_ref(), doc_path, index);
            index_nodes(&dropdown.body, doc_path, index, scope);
        }
        _ => {}
    }
}

/// Records a document's `.. sectnum::` options.
///
/// A `.. sectnum::` numbers its whole document regardless of where it's
/// written, so this is called from anywhere `index_nodes` finds one, not just
/// the document's top level — matching docutils, which treats it as a
/// document-wide switch, not a positional marker. When a document writes more
/// than one, the last visited (document order) wins, simply from
/// `BTreeMap::insert` overwriting the earlier one.
/// Records a document's `.. sectnum::` options.
///
/// A `.. sectnum::` numbers its whole document regardless of where it's
/// written, so this is called from anywhere `index_nodes` finds one, not just
/// the document's top level — matching docutils, which treats it as a
/// document-wide switch, not a positional marker. When a document writes more
/// than one, the last visited (document order) wins, simply from
/// `BTreeMap::insert` overwriting the earlier one.
fn index_sectnum(options: &rinx_ast::SectnumOptions, doc_path: &str, index: &mut ProjectIndex) {
    index.sectnum.insert(doc_path.to_string(), options.clone());
}

fn register_directive_name(name: Option<&TargetName>, doc_path: &str, index: &mut ProjectIndex) {
    if let Some(target_name) = name {
        index.targets.insert(
            target_name.clone(),
            TargetLocation::Internal(doc_path.to_string()),
        );
    }
}

/// Indexes every row's cells, recursing into their content. Shared by grid
/// tables, `.. table::`, and the data-table directives, whose rows otherwise
/// arrive in different shapes (a `Vec<TableRow>` split into header/body, or
/// one flat `Vec<TableRow>` with a separate header-row count).
fn index_table_rows<'a>(
    rows: impl IntoIterator<Item = &'a TableRow>,
    doc_path: &str,
    index: &mut ProjectIndex,
    scope: &mut Scope,
) {
    for row in rows {
        for cell in &row.cells {
            index_nodes(&cell.content, doc_path, index, scope);
        }
    }
}

/// Tests, split by topic into sibling modules rather than kept inline:
/// [`targets_and_titles_tests`] for the plain target/title/glossary walk,
/// [`domain_object_tests`] for the scope-qualified domain-object keys, and
/// [`index_entry_tests`] for the `.. index::` directive.
#[cfg(test)]
mod domain_object_tests;
#[cfg(test)]
mod index_entry_tests;
#[cfg(test)]
mod targets_and_titles_tests;

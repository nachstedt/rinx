use rusty_sphinx_ast::{Directive, Document, IndexEntry, Node, TableRow, TargetName};
use rusty_sphinx_index::{EquationLocation, GenIndexEntry, ProjectIndex, TargetLocation};
use rusty_sphinx_scope::Scope;

use super::domain_object_index::index_domain_object;
use super::equation_numbering::number_equations;
use super::outline::build_document_outline;

/// Analyzes a single `Document` and returns a local `ProjectIndex`.
///
/// This extracts targets, document titles, the document's section outline,
/// glossary terms and equation numbers. The `nav_tree` is not
/// populated here — it is built globally by [`build_project_index()`].
#[must_use]
pub fn analyze(doc: &Document) -> ProjectIndex {
    let mut index = ProjectIndex::default();
    let mut found_title = false;
    for node in &doc.nodes {
        if !found_title && let Node::Heading { level: 1, text } = node {
            index
                .document_titles
                .insert(doc.path.clone(), rusty_sphinx_ast::inline_plain_text(text));
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
    let toctrees: Vec<rusty_sphinx_index::DocumentToctree> = doc
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
        .map(|(toctree, section)| rusty_sphinx_index::DocumentToctree { toctree, section })
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
        // not surfaced in genindex_entries yet (see spec_gaps.md).
    }
}

/// Records one entity, its link target, and everything inside its sections.
fn index_entity_and_its_sections(
    entity: &rusty_sphinx_ast::EntityBody,
    doc_path: &str,
    index: &mut ProjectIndex,
    scope: &mut Scope,
) {
    super::entity_index::index_entity(entity, doc_path, index);
    // An entity is also an ordinary link target, so `:ref:` reaches one without
    // any role being declared — which is what makes a schema's roles optional
    // sugar rather than an obligation.
    index.targets.insert(
        TargetName::new(entity.id.as_str()),
        TargetLocation::Internal(doc_path.to_string()),
    );
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
/// [`rusty_sphinx_ast::DomainObjectBody::deduce_local_scope`], shared with
/// the renderer so index keys and anchor `id`s can't drift apart) and the
/// most recently seen `py:module` (document-order state, not lexical
/// nesting — real Sphinx docs write `py:module` and the functions/classes it
/// documents as *siblings*, not nested underneath it, so it is never popped
/// when returning from a nested body; a module stays "current" for the rest
/// of the document until another `py:module`, or `py:currentmodule`,
/// changes it). `scope.c` is the same idea for the `c` domain's
/// `c:struct`/`c:union` nesting — a wholly separate stack (see
/// [`rusty_sphinx_scope::CScope`]'s doc comment for why it isn't a variant of
/// `PythonScope`); `c:function`/`c:macro` never touch it and keep qualifying
/// via `scope.python` exactly as before it existed.
pub(super) fn index_nodes(
    nodes: &[Node],
    doc_path: &str,
    index: &mut ProjectIndex,
    scope: &mut Scope,
) {
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
                Directive::Admonition { body, .. }
                | Directive::VersionChange { body, .. }
                | Directive::EntitySection { body, .. }
                | Directive::SeeAlso { body },
            ) => {
                index_nodes(body, doc_path, index, scope);
            }
            // A grid carries no `:name:` of its own, so unlike a dropdown it
            // reaches none of the name-bearing arms — but its body is still
            // this document's content, and a target, section or entity
            // written in a cell belongs to the document exactly as if it had
            // been written outside one.
            Node::Directive(Directive::Grid(grid)) => {
                index_nodes(&grid.body, doc_path, index, scope);
            }
            Node::Directive(Directive::GridItem(item)) => {
                index_nodes(&item.body, doc_path, index, scope);
            }
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
                | Directive::Uml(_)),
            ) => index_name_bearing_directive(directive, doc_path, index, scope),
            Node::Directive(Directive::Sectnum(options)) => index_sectnum(options, doc_path, index),
            _ => {}
        }
    }
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
/// [`rusty_sphinx_ast::walk_nodes`] splits its directive arm out: three
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
fn index_sectnum(
    options: &rusty_sphinx_ast::SectnumOptions,
    doc_path: &str,
    index: &mut ProjectIndex,
) {
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

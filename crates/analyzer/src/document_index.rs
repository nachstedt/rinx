use rusty_sphinx_ast::{Directive, Document, IndexEntry, Node, TableRow, TargetName};
use rusty_sphinx_index::{EquationLocation, GenIndexEntry, ProjectIndex, TargetLocation};
use rusty_sphinx_scope::Scope;

use super::domain_object_index::index_domain_object;
use super::equation_numbering::number_equations;

/// Analyzes a single `Document` and returns a local `ProjectIndex`.
///
/// This extracts targets, document titles, glossary terms and equation
/// numbers. The `nav_tree` is not
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
    index_nodes(&doc.nodes, &doc.path, &mut index, &mut Scope::default());
    for (label, number) in number_equations(doc) {
        index
            .equations
            .insert(label, EquationLocation::new(doc.path.clone(), number));
    }
    index
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
                            anchor: id.clone(),
                        });
                    }
                    // IndexEntry::See/SeeAlso redirect rather than link to
                    // content and are not surfaced in genindex_entries yet
                    // (see spec_gaps.md).
                }
            }
            Node::Directive(Directive::DomainObject(obj)) => {
                index_domain_object(obj, doc_path, index, scope);
            }
            Node::Directive(Directive::PyCurrentModule { module }) => match module {
                Some(name) => scope.python.set_module(name),
                None => scope.python.clear_module(),
            },
            Node::Directive(Directive::CNamespace { namespace }) => {
                scope.c.set_namespace(namespace.as_deref());
            }
            Node::Directive(Directive::CNamespacePush { namespace }) => {
                scope.c.push_namespace(namespace);
            }
            Node::Directive(Directive::CNamespacePop) => scope.c.pop_namespace(),
            Node::Directive(Directive::StdProgram { name }) => match name {
                Some(name) => scope.program.set(name),
                None => scope.program.clear(),
            },
            Node::Directive(
                Directive::Admonition { body, .. }
                | Directive::VersionChange { body, .. }
                | Directive::SeeAlso { body },
            ) => {
                index_nodes(body, doc_path, index, scope);
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
                register_table_name(name.as_ref(), doc_path, index);
                index_table_rows(rows, doc_path, index, scope);
            }
            Node::Directive(Directive::Table {
                header_rows,
                body_rows,
                name,
                ..
            }) => {
                register_table_name(name.as_ref(), doc_path, index);
                index_table_rows(header_rows.iter().chain(body_rows), doc_path, index, scope);
            }
            _ => {}
        }
    }
}

/// Registers a table's optional `:name:` as an internal cross-reference
/// target, shared by `.. list-table::`/`.. csv-table::` and `.. table::` —
/// the two directive kinds with a `:name:` option, out of the three table
/// kinds `index_nodes` indexes.
fn register_table_name(name: Option<&TargetName>, doc_path: &str, index: &mut ProjectIndex) {
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

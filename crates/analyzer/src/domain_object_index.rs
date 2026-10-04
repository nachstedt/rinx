use rinx_ast::DomainObjectBody;
use rinx_index::{GenIndexEntry, ModuleEntry, ProjectIndex};
use rinx_scope::{DefinitionNames, DocumentScopes};

use super::document_index::index_nodes;

/// Registers a single `Directive::DomainObject` under the names `scopes`
/// qualified it to, and recurses into its body. Split out of [`index_nodes`]
/// to keep that function's line count manageable.
pub(super) fn index_domain_object(
    obj: &rinx_ast::DomainObjectBody,
    doc_path: &str,
    index: &mut ProjectIndex,
    scopes: &DocumentScopes,
) {
    // `:no-index:` drops the target and the index entry but keeps the body.
    if !obj.no_index() {
        match &*scopes.definition(obj) {
            // A multi-flag `.. option:: -c, --compress` registers several
            // independently-indexed names sharing one rendered `<dt>`.
            DefinitionNames::Options(lines) => {
                for qualified_name in lines.iter().flatten() {
                    index_name(obj, qualified_name, doc_path, index);
                }
            }
            DefinitionNames::Object(names) => {
                for qualified_name in names.as_slice() {
                    index_name(obj, qualified_name, doc_path, index);
                    record_module(obj, qualified_name, doc_path, index);
                }
            }
        }
    }
    // The body is indexed exactly once, no matter how many names the object
    // declares — the aliases share it rather than each owning a copy.
    index_nodes(obj.body(), doc_path, index, scopes);
}

/// Registers `obj` as a target under `qualified_name`, and in the general
/// index unless it asked not to be.
fn index_name(
    obj: &DomainObjectBody,
    qualified_name: &str,
    doc_path: &str,
    index: &mut ProjectIndex,
) {
    index.insert_domain_object(obj.object_type(), qualified_name, doc_path);
    if obj.no_index_entry() {
        return;
    }
    let anchor = rinx_ast::build_domain_object_key(obj.object_type(), qualified_name);
    index.genindex_entries.push(GenIndexEntry {
        primary: format!("{qualified_name} ({})", obj.object_type().as_str()),
        subentry: None,
        main: false,
        doc_path: doc_path.to_string(),
        anchor: anchor.as_str().to_string(),
    });
}

/// Records a `.. py:module::` in [`ProjectIndex::modules`], for the module
/// index and the `:mod:` tooltip — what Sphinx's `note_module` does. Any other
/// object type records nothing. Only reached for a module that is indexed at
/// all, since `:no-index:` leaves it out of Sphinx's `modules` too.
fn record_module(obj: &DomainObjectBody, name: &str, doc_path: &str, index: &mut ProjectIndex) {
    if let DomainObjectBody::PyModule { options, .. } = obj {
        index.modules.insert(
            rinx_ast::TargetName::new(name),
            ModuleEntry {
                doc_path: doc_path.to_string(),
                synopsis: options.synopsis.clone(),
                platform: options.platform.clone(),
                deprecated: options.has(rinx_ast::ModuleFlag::Deprecated),
            },
        );
    }
}

#[cfg(test)]
mod basics_tests;
#[cfg(test)]
mod c_tests;
#[cfg(test)]
mod scoping_tests;

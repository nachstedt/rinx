use rinx_ast::DomainObjectBody;
use rinx_index::{GenIndexEntry, ProjectIndex};
use rinx_scope::Scope;

use super::document_index::index_nodes;

/// Registers a single `Directive::DomainObject` (and recurses into its
/// body), handling both qualification and module-context updates. Split out
/// of [`index_nodes`] to keep that function's line count manageable.
pub(super) fn index_domain_object(
    obj: &rinx_ast::DomainObjectBody,
    doc_path: &str,
    index: &mut ProjectIndex,
    scope: &mut Scope,
) {
    // `StdCmdoption` qualifies against `scope.program`, not `scope.python`/
    // `scope.c`, and — because a multi-flag `.. option:: -c, --compress`
    // registers *several* independently-indexed names sharing *one* rendered
    // `<dt>` (see `DomainObjectBody::StdCmdoption`'s doc comment) — doesn't fit
    // the single-name-per-`<dt>` loop below at all. Handled as its own early
    // branch rather than folded into the `is_module`/`uses_c_scope` chain.
    if let DomainObjectBody::StdCmdoption { signatures, .. } = obj {
        for line in signatures.as_slice() {
            for spec in rinx_ast::split_option_line_specs(line) {
                let optname = rinx_ast::extract_option_name(&spec);
                let qualified_name = scope.program.qualify(&optname);
                index.insert_domain_object(obj.object_type(), &qualified_name, doc_path);
                let anchor = rinx_ast::build_domain_object_key(obj.object_type(), &qualified_name);
                index.genindex_entries.push(GenIndexEntry {
                    primary: format!("{qualified_name} ({})", obj.object_type().as_str()),
                    subentry: None,
                    main: false,
                    doc_path: doc_path.to_string(),
                    anchor: anchor.as_str().to_string(),
                });
            }
        }
        // Options never nest (`deduce_local_scope` is empty for `StdCmdoption`),
        // so the shared body is indexed with no scope push/pop, unlike the
        // `is_module`/`uses_c_scope`/plain-`py` branches below.
        index_nodes(obj.body(), doc_path, index, scope);
        return;
    }

    // A `py:module`'s own name is never qualified against the *previous*
    // module: real Sphinx always writes it in full and sets it verbatim as
    // the new current module, it never nests it under whatever module was
    // current before.
    let is_module = matches!(obj, rinx_ast::DomainObjectBody::PyModule { .. });
    // Every `c`-domain object nests under `CScope` instead of `PythonScope`
    // (`docs/dev/known_bugs.md` #2: `c:function`/`c:macro` used to keep qualifying via
    // `PythonScope`'s module+class stack, so nesting one inside a
    // `py:class`/`py:exception` body wrongly prefixed it — real Sphinx's C
    // domain has no concept of an enclosing Python class at all). `c:type`
    // joins this set (alongside `c:struct`/`c:union`/`c:member`) because real
    // Sphinx's C domain scopes nested declarations generically off whatever
    // declaration they're indented under, not specifically off struct/union.
    let uses_c_scope = matches!(
        obj,
        DomainObjectBody::CStruct { .. }
            | DomainObjectBody::CUnion { .. }
            | DomainObjectBody::CMember { .. }
            | DomainObjectBody::CType { .. }
            | DomainObjectBody::CFunction { .. }
            | DomainObjectBody::CMacro { .. }
    );
    // The `:module:` option: overrides `scope.python`'s current module for
    // the duration of this whole call — this object's own (and its aliases')
    // qualification below, and its nested body — restored at the very end,
    // mirroring real Sphinx's `PyObject.before_content()`/`after_content()`
    // push/pop of `ref_context['py:module']`. `py:module` and every `c`
    // object never carry this option (`module_override` is always `None` for
    // them), so this is a no-op for `is_module`/`uses_c_scope`.
    let restore_module = obj
        .module_override()
        .map(|module| scope.python.push_module_override(module));

    let own_names = obj.names();
    // Only the primary name qualifies the *scope*: it alone decides what this
    // object lends to its body and, for a module, what becomes current. The
    // remaining names are pure aliases — each independently resolvable, but
    // none of them contributes scope.
    let (qualified_primary, new_segments) = if is_module {
        (own_names.first().clone(), Vec::new())
    } else if uses_c_scope {
        let qualification = scope.c.qualify(own_names.first());
        (qualification.qualified_name, qualification.new_segments)
    } else {
        let qualification = scope.python.qualify(own_names.first());
        (qualification.qualified_name, qualification.new_segments)
    };

    for (index_in_object, own_name) in own_names.as_slice().iter().enumerate() {
        let qualified_name = if index_in_object == 0 {
            qualified_primary.clone()
        } else if uses_c_scope {
            scope.c.qualify(own_name).qualified_name
        } else {
            scope.python.qualify(own_name).qualified_name
        };
        if obj.no_index() {
            continue;
        }
        index.insert_domain_object(obj.object_type(), &qualified_name, doc_path);
        if obj.no_index_entry() {
            continue;
        }
        let anchor = rinx_ast::build_domain_object_key(obj.object_type(), &qualified_name);
        index.genindex_entries.push(GenIndexEntry {
            primary: format!("{qualified_name} ({})", obj.object_type().as_str()),
            subentry: None,
            main: false,
            doc_path: doc_path.to_string(),
            anchor: anchor.as_str().to_string(),
        });
    }

    if is_module {
        scope.python.set_module(&qualified_primary);
    }
    // The body is indexed exactly once, no matter how many names the object
    // declares — the aliases share it rather than each owning a copy.
    let lend = obj.deduce_local_scope(&new_segments);
    if uses_c_scope {
        let saved = scope.c.push_containers(&lend);
        index_nodes(obj.body(), doc_path, index, scope);
        scope.c.restore_containers(saved);
    } else {
        let depth = scope.python.push_classes(&lend);
        index_nodes(obj.body(), doc_path, index, scope);
        scope.python.truncate_classes(depth);
    }

    if let Some(previous) = restore_module {
        scope.python.restore_module(previous);
    }
}

#[cfg(test)]
mod basics_tests;
#[cfg(test)]
mod c_tests;
#[cfg(test)]
mod scoping_tests;

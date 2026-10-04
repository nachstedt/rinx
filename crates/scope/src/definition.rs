//! What a domain-object definition is qualified to, and how it moves the
//! scope for its body and for everything written after it.
//!
//! These are the rules the analyzer's index key and the renderer's anchor `id`
//! must agree on, so they exist exactly once: [`crate::DocumentScopes`] applies
//! them in one walk over a document and both phases read its result. Before
//! that, each phase replayed them in its own walk, kept in step by comments.

use rinx_ast::{Directive, DomainObjectBody, NonEmptyVector};

use crate::Scope;

/// The names a domain-object definition is qualified to — its index keys and
/// anchor `id`s, before the object type is prefixed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DefinitionNames {
    /// Every object type but `std:cmdoption`: one qualified name per entry of
    /// [`DomainObjectBody::names`], in that order, the primary name first.
    Object(NonEmptyVector<String>),
    /// A `.. option::`/`.. cmdoption::`: per signature line, the name of each
    /// option the line lists, qualified against the ambient `.. program::`.
    ///
    /// Kept per line because a multi-flag `.. option:: -c, --compress`
    /// registers *several* independently-indexed names sharing *one* rendered
    /// `<dt>` (see `DomainObjectBody::StdCmdoption`'s doc comment), which the
    /// one-name-per-`<dt>` shape of [`Self::Object`] cannot express.
    Options(Vec<Vec<String>>),
}

/// What [`Scope::enter_definition`] changed and [`Scope::leave`] must undo.
#[must_use = "a definition's scope must be left again, or it leaks into its siblings"]
#[derive(Debug)]
pub(crate) struct Entered {
    /// The module a `:module:` option replaced, when it carried one.
    replaced_module: Option<ReplacedModule>,
    body: BodyScope,
}

/// The current module a `:module:` option replaced — which may have been
/// none at all.
#[derive(Debug)]
struct ReplacedModule(Option<String>);

/// What a definition pushed for its body.
#[derive(Debug)]
enum BodyScope {
    /// The C container stack as it was before.
    C(Vec<Vec<String>>),
    /// The depth of the Python class stack before.
    Python(usize),
    /// Nothing: options never nest.
    Nothing,
}

impl Scope {
    /// Applies a directive that only moves the scope — `.. py:currentmodule::`,
    /// the `.. c:namespace::` family and `.. program::` — and does nothing
    /// for any other directive.
    pub fn apply_scope_directive(&mut self, directive: &Directive) {
        match directive {
            Directive::PyCurrentModule { module } => match module {
                Some(name) => self.python.set_module(name),
                None => self.python.clear_module(),
            },
            Directive::CNamespace { namespace } => self.c.set_namespace(namespace.as_deref()),
            Directive::CNamespacePush { namespace } => self.c.push_namespace(namespace),
            Directive::CNamespacePop => self.c.pop_namespace(),
            Directive::StdProgram { name } => match name {
                Some(name) => self.program.set(name),
                None => self.program.clear(),
            },
            _ => {}
        }
    }

    /// Qualifies the definition `obj` against this scope and enters the scope
    /// its body is read in, returning its names and what [`Self::leave`] must
    /// undo once the body has been walked.
    pub(crate) fn enter_definition(
        &mut self,
        obj: &DomainObjectBody,
    ) -> (DefinitionNames, Entered) {
        // `StdCmdoption` qualifies against `program`, not `python`/`c`, and —
        // because a multi-flag `.. option:: -c, --compress` registers several
        // names sharing one `<dt>` — doesn't fit the one-name-per-`<dt>` shape
        // below at all.
        if let DomainObjectBody::StdCmdoption { signatures, .. } = obj {
            let names = signatures
                .as_slice()
                .iter()
                .map(|line| {
                    rinx_ast::split_option_line_specs(line)
                        .iter()
                        .map(|spec| self.program.qualify(&rinx_ast::extract_option_name(spec)))
                        .collect()
                })
                .collect();
            // Options never nest (`deduce_local_scope` is empty for
            // `StdCmdoption`), so the body is read with nothing pushed.
            let entered = Entered {
                replaced_module: None,
                body: BodyScope::Nothing,
            };
            return (DefinitionNames::Options(names), entered);
        }

        // A `py:module`'s own name is never qualified against the *previous*
        // module: real Sphinx always writes it in full and sets it verbatim as
        // the new current module, it never nests it under whatever module was
        // current before.
        let is_module = matches!(obj, DomainObjectBody::PyModule { .. });
        // Every `c`-domain object nests under `CScope` instead of
        // `PythonScope` (`docs/dev/known_bugs.md` #2: `c:function`/`c:macro`
        // used to keep qualifying via `PythonScope`'s module+class stack, so
        // nesting one inside a `py:class`/`py:exception` body wrongly prefixed
        // it — real Sphinx's C domain has no concept of an enclosing Python
        // class at all). `c:type` joins this set (alongside
        // `c:struct`/`c:union`/`c:member`) because real Sphinx's C domain
        // scopes nested declarations generically off whatever declaration
        // they're indented under, not specifically off struct/union.
        let uses_c_scope = matches!(
            obj,
            DomainObjectBody::CStruct { .. }
                | DomainObjectBody::CUnion { .. }
                | DomainObjectBody::CMember { .. }
                | DomainObjectBody::CType { .. }
                | DomainObjectBody::CFunction { .. }
                | DomainObjectBody::CMacro { .. }
        );
        // The `:module:` option: overrides the current module for the whole
        // object — its own (and its aliases') qualification below, and its
        // nested body — restored by `leave`, mirroring real Sphinx's
        // `PyObject.before_content()`/`after_content()` push/pop of
        // `ref_context['py:module']`. `py:module` and every `c` object never
        // carry this option, so this is a no-op for them.
        let replaced_module = obj
            .module_override()
            .map(|module| ReplacedModule(self.python.push_module_override(module)));

        let own_names = obj.names();
        // Only the primary name qualifies the *scope*: it alone decides what
        // this object lends to its body and, for a module, what becomes
        // current. The remaining names are pure aliases — each independently
        // resolvable, but none of them contributes scope.
        let (qualified_primary, new_segments) = if is_module {
            (own_names.first().clone(), Vec::new())
        } else if uses_c_scope {
            let qualification = self.c.qualify(own_names.first());
            (qualification.qualified_name, qualification.new_segments)
        } else {
            let qualification = self.python.qualify(own_names.first());
            (qualification.qualified_name, qualification.new_segments)
        };
        let aliases = own_names.as_slice()[1..]
            .iter()
            .map(|own_name| {
                if uses_c_scope {
                    self.c.qualify(own_name).qualified_name
                } else {
                    self.python.qualify(own_name).qualified_name
                }
            })
            .collect();
        let names = NonEmptyVector::new(qualified_primary, aliases);

        if is_module {
            self.python.set_module(names.first());
        }
        // The body is read exactly once, no matter how many names the object
        // declares — the aliases share it rather than each owning a copy.
        let lend = obj.deduce_local_scope(&new_segments);
        let body = if uses_c_scope {
            BodyScope::C(self.c.push_containers(&lend))
        } else {
            BodyScope::Python(self.python.push_classes(&lend))
        };
        (
            DefinitionNames::Object(names),
            Entered {
                replaced_module,
                body,
            },
        )
    }

    /// Leaves a definition entered with [`Self::enter_definition`], once its
    /// body has been walked: the class or container stack is restored, and so
    /// is the module a `:module:` option replaced. A `py:module` stays
    /// current — that is document-order state, not lexical nesting.
    pub(crate) fn leave(&mut self, entered: Entered) {
        match entered.body {
            BodyScope::C(saved) => self.c.restore_containers(saved),
            BodyScope::Python(depth) => self.python.truncate_classes(depth),
            BodyScope::Nothing => {}
        }
        if let Some(ReplacedModule(previous)) = entered.replaced_module {
            self.python.restore_module(previous);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The first domain object `source` parses to.
    fn object(source: &str) -> DomainObjectBody {
        let document = rinx_parser::parse("test.rst", source);
        document
            .nodes
            .into_iter()
            .find_map(|node| match node {
                rinx_ast::Node::Directive(Directive::DomainObject(obj)) => Some(obj),
                _ => None,
            })
            .expect("a domain object")
    }

    /// The names of an object definition, as plain strings.
    fn object_names(names: &DefinitionNames) -> Vec<&str> {
        match names {
            DefinitionNames::Object(names) => names.as_slice().iter().map(String::as_str).collect(),
            DefinitionNames::Options(_) => panic!("an object, not options"),
        }
    }

    #[test]
    fn test_apply_scope_directive_sets_and_clears_the_python_module() {
        // Given
        let mut scope = Scope::default();

        // When — set, then clear
        scope.apply_scope_directive(&Directive::PyCurrentModule {
            module: Some("pkg.mod".to_string()),
        });
        // Asserted through `qualify`, the scope's observable effect, rather than
        // through a getter added just for this test.
        let after_set = scope.python.qualify("f").qualified_name;
        scope.apply_scope_directive(&Directive::PyCurrentModule { module: None });

        // Then
        assert_eq!(after_set, "pkg.mod.f");
        assert_eq!(scope.python.qualify("f").qualified_name, "f");
    }

    #[test]
    fn test_apply_scope_directive_pushes_and_pops_a_c_namespace() {
        // Given
        let mut scope = Scope::default();

        // When
        scope.apply_scope_directive(&Directive::CNamespacePush {
            namespace: "inner".to_string(),
        });
        let after_push = scope.c.qualify("f").qualified_name;
        scope.apply_scope_directive(&Directive::CNamespacePop);

        // Then
        assert_eq!(after_push, "inner.f");
        assert_eq!(scope.c.qualify("f").qualified_name, "f");
    }

    #[test]
    fn test_apply_scope_directive_sets_and_clears_the_program() {
        // Given
        let mut scope = Scope::default();

        // When
        scope.apply_scope_directive(&Directive::StdProgram {
            name: Some("tool".to_string()),
        });
        let after_set = scope.program.qualify("-v");
        scope.apply_scope_directive(&Directive::StdProgram { name: None });

        // Then
        assert_eq!(after_set, "tool.-v");
        assert_eq!(scope.program.qualify("-v"), "-v");
    }

    #[test]
    fn test_apply_scope_directive_ignores_a_directive_that_moves_no_scope() {
        // Given — a walk passes every directive here, so an unrelated one
        // must be a no-op rather than a panic
        let mut scope = Scope::default();
        scope.python.set_module("pkg");

        // When
        scope.apply_scope_directive(&Directive::SeeAlso { body: vec![] });

        // Then — the unrelated directive changed nothing
        assert_eq!(scope.python.qualify("f").qualified_name, "pkg.f");
    }

    #[test]
    fn test_enter_definition_qualifies_a_class_and_lends_it_to_the_body() {
        // Given
        let mut scope = Scope::default();
        scope.python.set_module("pkg");
        let class = object(".. py:class:: Widget\n");

        // When
        let (names, entered) = scope.enter_definition(&class);
        let inside = scope.python.qualify("run").qualified_name;
        scope.leave(entered);

        // Then — the body reads the class, and leaving takes it away again
        assert_eq!(object_names(&names), ["pkg.Widget"]);
        assert_eq!(inside, "pkg.Widget.run");
        assert_eq!(scope.python.qualify("run").qualified_name, "pkg.run");
    }

    #[test]
    fn test_enter_definition_qualifies_every_alias_against_the_same_scope() {
        // Given
        let mut scope = Scope::default();
        scope.python.set_module("pkg");
        let function = object(".. py:function:: load(path)\n                 loads(text)\n");

        // When
        let (names, entered) = scope.enter_definition(&function);
        scope.leave(entered);

        // Then — primary first
        assert_eq!(object_names(&names), ["pkg.load", "pkg.loads"]);
    }

    #[test]
    fn test_enter_definition_sets_a_module_verbatim_and_keeps_it_after_leaving() {
        // Given — a module is never nested under the one current before it
        let mut scope = Scope::default();
        scope.python.set_module("old");
        let module = object(".. py:module:: pkg.sub\n");

        // When
        let (names, entered) = scope.enter_definition(&module);
        scope.leave(entered);

        // Then — document-order state, not lexical nesting
        assert_eq!(object_names(&names), ["pkg.sub"]);
        assert_eq!(scope.python.qualify("f").qualified_name, "pkg.sub.f");
    }

    #[test]
    fn test_leave_restores_the_module_a_module_option_replaced() {
        // Given
        let mut scope = Scope::default();
        scope.python.set_module("pkg");
        let function = object(".. py:function:: run()\n   :module: other\n");

        // When
        let (names, entered) = scope.enter_definition(&function);
        let inside = scope.python.qualify("helper").qualified_name;
        scope.leave(entered);

        // Then
        assert_eq!(object_names(&names), ["other.run"]);
        assert_eq!(inside, "other.helper");
        assert_eq!(scope.python.qualify("helper").qualified_name, "pkg.helper");
    }

    #[test]
    fn test_enter_definition_nests_a_c_struct_and_leave_restores_the_containers() {
        // Given
        let mut scope = Scope::default();
        let structure = object(".. c:struct:: point\n");

        // When
        let (names, entered) = scope.enter_definition(&structure);
        let inside = scope.c.qualify("x").qualified_name;
        scope.leave(entered);

        // Then
        assert_eq!(object_names(&names), ["point"]);
        assert_eq!(inside, "point.x");
        assert_eq!(scope.c.qualify("x").qualified_name, "x");
    }

    #[test]
    fn test_enter_definition_qualifies_a_c_function_against_the_c_scope_only() {
        // Given — an enclosing Python module means nothing to the C domain
        let mut scope = Scope::default();
        scope.python.set_module("pkg");
        let function = object(".. c:function:: int area(void)\n");

        // When
        let (names, entered) = scope.enter_definition(&function);
        scope.leave(entered);

        // Then
        assert_eq!(object_names(&names), ["area"]);
    }

    #[test]
    fn test_enter_definition_qualifies_each_option_of_each_line_against_the_program() {
        // Given
        let mut scope = Scope::default();
        scope.program.set("tool");
        let option = object(".. option:: -c, --compress\n            -v\n");

        // When
        let (names, entered) = scope.enter_definition(&option);
        scope.leave(entered);

        // Then — per line, per option
        assert_eq!(
            names,
            DefinitionNames::Options(vec![
                vec!["tool.-c".to_string(), "tool.--compress".to_string()],
                vec!["tool.-v".to_string()],
            ])
        );
    }
}

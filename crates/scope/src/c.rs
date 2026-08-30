use rusty_sphinx_ast::TargetSearchOrder;

/// The `c`-domain's current scope while indexing/rendering, modeled as a
/// stack of scopes whose **last entry is the one currently in effect**.
///
/// Two independent mechanisms push onto that stack, exactly as in real
/// Sphinx, where both manipulate the single `ref_context['c:parent_key']`
/// value (`CObject.before_content`/`after_content` for the first,
/// `CNamespaceObject`/`CNamespacePushObject`/`CNamespacePopObject.run()` for
/// the second, all reading and writing that same key):
///
/// 1. **Lexical nesting** — entering a `c:struct`/`c:union`/`c:type` body
///    pushes that container's name ([`Self::push_containers`]), and leaving
///    it restores the whole stack as it was ([`Self::restore_containers`]).
/// 2. **The `c:namespace` directive family** —
///    [`Self::set_namespace`] (absolute, resets the stack),
///    [`Self::push_namespace`] (relative), [`Self::pop_namespace`].
///
/// Because both share one stack, a `.. c:namespace:: NULL` written *inside*
/// a `c:type` body resets the qualifier that body established — the real
/// `CPython` `c-api/memory.rst` shape, where enum-style `.. c:macro::`
/// constants nested in `.. c:type:: PyMemAllocatorDomain` are documented
/// bare.
///
/// Restoring the lexical scope replaces the stack *by value* rather than
/// truncating it to a saved depth: `c:namespace` can shrink the stack
/// arbitrarily mid-body, which would leave a saved depth pointing past the
/// stack's new end, making a depth-based `Vec::truncate` restore silently
/// no-op.
///
/// One deliberate deviation from real Sphinx follows from that: an unmatched
/// `c:namespace-push` inside a `c:struct`/`c:union`/`c:type` body is
/// contained by the body-close restore, so it cannot leak out and be
/// consumed by a later, unrelated `c:namespace-pop`. Sphinx's
/// `before_content`/`after_content` save and restore only the current-scope
/// pointer, not the separate namespace stack its namespace directives
/// maintain, so it appears to leak there. Containing is simpler and more
/// defensible for what is malformed input either way.
///
/// Deliberately not a variant of [`crate::PythonScope`] (see that type's own
/// doc comment) — the `c` domain has no class nesting and no module concept,
/// so this is `PythonScope`'s `classes` stack and its `qualify`/`absorb`
/// algorithm on their own, with nothing module-shaped grafted on. Every
/// `c`-domain object type qualifies against this type, including
/// `c:function`/`c:macro` (`known_bugs.md` #2: they used to keep qualifying
/// via `PythonScope`, so a `c:function`/`c:macro` nested inside a
/// `py:class`/`py:exception` body was wrongly prefixed with the enclosing
/// Python module+class — real Sphinx's C domain has no concept of an
/// enclosing Python class at all).
///
/// Transient: built fresh by each of the analyzer's and renderer's document
/// walks, never stored in the AST.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CScope {
    /// Invariant: never empty — the last entry is the current scope. Every
    /// mutator below preserves this, which is why the field is private and
    /// [`Default`] is hand-written rather than derived (the derive would
    /// produce an empty vector).
    scopes: Vec<Vec<String>>,
}

impl Default for CScope {
    fn default() -> Self {
        Self {
            scopes: vec![Vec::new()],
        }
    }
}

/// The result of [`CScope::qualify`]: the object's fully qualified name, plus
/// the segments its own (possibly dotted) name contributes beyond the scope
/// it was qualified against — the raw material
/// [`rusty_sphinx_ast::DomainObjectBody::deduce_local_scope`] slices to
/// decide what a nested body should push onto the container stack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CQualification {
    pub qualified_name: String,
    pub new_segments: Vec<String>,
}

impl CScope {
    /// The scope currently in effect — the last entry on the stack.
    fn current(&self) -> &[String] {
        self.scopes.last().map_or(&[], Vec::as_slice)
    }

    /// Pushes lexically nested container segments (innermost last) onto the
    /// scope, returning a snapshot the caller restores with
    /// [`Self::restore_containers`] once the nested body has been processed.
    ///
    /// The snapshot is the whole stack, not a depth: see the type's doc
    /// comment for why a depth cannot be restored correctly once
    /// `c:namespace` is in play.
    pub fn push_containers(&mut self, segments: &[String]) -> Vec<Vec<String>> {
        let saved = self.scopes.clone();
        let mut nested = self.current().to_vec();
        nested.extend_from_slice(segments);
        self.scopes.push(nested);
        saved
    }

    /// Restores the scope stack to a snapshot previously returned by
    /// [`Self::push_containers`], discarding whatever the nested body did to
    /// it — including any unmatched `c:namespace` changes.
    pub fn restore_containers(&mut self, saved: Vec<Vec<String>>) {
        self.scopes = saved;
    }

    /// Sets the current scope absolutely and resets the namespace stack —
    /// the `.. c:namespace::` directive. `None` is the `NULL`/`0` form,
    /// resetting to global scope.
    ///
    /// `Some(ns)` is implemented as a reset followed by
    /// [`Self::push_namespace`], which is precisely how real Sphinx
    /// documents it: `.. c:namespace:: A.B` is equivalent to
    /// `.. c:namespace:: NULL` followed by `.. c:namespace-push:: A.B`, so a
    /// later [`Self::pop_namespace`] returns to global scope.
    pub fn set_namespace(&mut self, namespace: Option<&str>) {
        self.scopes = vec![Vec::new()];
        if let Some(namespace) = namespace {
            self.push_namespace(namespace);
        }
    }

    /// Extends the current scope relatively — the `.. c:namespace-push::`
    /// directive.
    pub fn push_namespace(&mut self, namespace: &str) {
        let mut nested = self.current().to_vec();
        nested.extend(
            namespace
                .split('.')
                .filter(|segment| !segment.is_empty())
                .map(str::to_string),
        );
        self.scopes.push(nested);
    }

    /// Undoes the most recent [`Self::push_namespace`] in its entirety (not
    /// one dotted segment of it) — the `.. c:namespace-pop::` directive.
    /// With no push to undo, the scope becomes global, matching the spec.
    pub fn pop_namespace(&mut self) {
        self.scopes.pop();
        if self.scopes.is_empty() {
            self.scopes.push(Vec::new());
        }
    }

    /// Qualifies `own_name` (a domain object's own, possibly dotted, name)
    /// against this scope.
    ///
    /// A dotted `own_name` that repeats (all or just the innermost segment
    /// of) the current container stack has that repeat absorbed, exactly
    /// once, before the scope is prefixed — mirroring
    /// [`crate::PythonScope::qualify`]'s class-repeat absorption, e.g. a
    /// member written `Data.count` inside `.. c:struct:: Data` must not
    /// double to `Data.Data.count`.
    #[must_use]
    pub fn qualify(&self, own_name: &str) -> CQualification {
        let sig_segments: Vec<&str> = own_name.split('.').collect();
        let new_segments = self.absorb(&sig_segments);

        let mut parts: Vec<&str> = self.current().iter().map(String::as_str).collect();
        parts.extend(new_segments.iter().copied());

        CQualification {
            qualified_name: parts.join("."),
            new_segments: new_segments.into_iter().map(str::to_string).collect(),
        }
    }

    /// The qualified names a *reference* to `name` may denote, in the order
    /// they should be tried, most-to-least qualified for
    /// [`TargetSearchOrder::MostQualifiedFirst`] and reversed for
    /// [`TargetSearchOrder::LeastQualifiedFirst`] — the reference-side
    /// counterpart to [`Self::qualify`], mirroring
    /// [`crate::PythonScope::reference_candidates`] (see that method's doc
    /// comment for why nothing is absorbed here the way `qualify` absorbs a
    /// repeated container prefix on the definition side).
    ///
    /// Walks every suffix of the container stack from the full path down to
    /// the bare name, dropping the *innermost* container at each step (e.g.
    /// `Outer.Inner.name`, then `Outer.name`, then `name`) — mirroring real
    /// Sphinx's `Symbol.find_declaration`, which starts at the reference's
    /// enclosing declaration and walks up through each ancestor scope in
    /// turn, not just straight to the root.
    #[must_use]
    pub fn reference_candidates(&self, name: &str, order: TargetSearchOrder) -> Vec<String> {
        let current = self.current();
        let tiers: Vec<String> = (0..=current.len())
            .rev()
            .map(|depth| {
                if depth == 0 {
                    name.to_string()
                } else {
                    format!("{}.{name}", current[..depth].join("."))
                }
            })
            .collect();

        match order {
            TargetSearchOrder::MostQualifiedFirst => tiers,
            TargetSearchOrder::LeastQualifiedFirst => tiers.into_iter().rev().collect(),
        }
    }

    /// Drops a leading repeat of the container stack from `sig_segments`:
    /// the whole stack if `sig_segments` starts with it, else just its
    /// innermost (last) entry if `sig_segments` starts with that alone, else
    /// nothing. Empty when there is no container scope to compare against.
    fn absorb<'a>(&self, sig_segments: &[&'a str]) -> Vec<&'a str> {
        let current = self.current();
        if current.is_empty() {
            return sig_segments.to_vec();
        }
        if sig_segments.len() >= current.len()
            && sig_segments[..current.len()]
                .iter()
                .zip(current)
                .all(|(sig, container)| *sig == container)
        {
            return sig_segments[current.len()..].to_vec();
        }
        if sig_segments.first() == current.last().map(String::as_str).as_ref() {
            return sig_segments[1..].to_vec();
        }
        sig_segments.to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segments(strs: &[&str]) -> Vec<String> {
        strs.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn test_qualify_returns_bare_name_with_empty_scope() {
        // Given
        let scope = CScope::default();

        // When
        let qualification = scope.qualify("count");

        // Then
        assert_eq!(qualification.qualified_name, "count");
        assert_eq!(qualification.new_segments, vec!["count".to_string()]);
    }

    #[test]
    fn test_qualify_prefixes_with_enclosing_struct() {
        // Given — `.. c:member:: int count` nested inside `.. c:struct:: Data`.
        let mut scope = CScope::default();
        scope.push_containers(&segments(&["Data"]));

        // When
        let qualification = scope.qualify("count");

        // Then
        assert_eq!(qualification.qualified_name, "Data.count");
        assert_eq!(qualification.new_segments, vec!["count".to_string()]);
    }

    #[test]
    fn test_qualify_absorbs_whole_container_repeat() {
        // Given — a member written with its enclosing struct's name repeated.
        let mut scope = CScope::default();
        scope.push_containers(&segments(&["Data"]));

        // When
        let qualification = scope.qualify("Data.count");

        // Then — not doubled to "Data.Data.count".
        assert_eq!(qualification.qualified_name, "Data.count");
        assert_eq!(qualification.new_segments, vec!["count".to_string()]);
    }

    #[test]
    fn test_qualify_absorbs_innermost_container_repeat_in_two_level_nesting() {
        // Given — a union nested inside a struct, itself nested arbitrarily.
        let mut scope = CScope::default();
        scope.push_containers(&segments(&["Outer"]));
        scope.push_containers(&segments(&["Inner"]));

        // When
        let qualification = scope.qualify("Inner.field");

        // Then
        assert_eq!(qualification.qualified_name, "Outer.Inner.field");
    }

    #[test]
    fn test_qualify_absorbs_whole_two_level_container_repeat() {
        // Given
        let mut scope = CScope::default();
        scope.push_containers(&segments(&["Outer"]));
        scope.push_containers(&segments(&["Inner"]));

        // When
        let qualification = scope.qualify("Outer.Inner.field");

        // Then — not doubled to "Outer.Inner.Outer.Inner.field".
        assert_eq!(qualification.qualified_name, "Outer.Inner.field");
    }

    #[test]
    fn test_qualify_does_not_absorb_unrelated_dotted_prefix() {
        // Given — the flat CPython-docs shape: no enclosing `.. c:struct::`
        // at all, so nothing is ever absorbed.
        let scope = CScope::default();

        // When
        let qualification = scope.qualify("PyTypeObject.tp_bases");

        // Then
        assert_eq!(qualification.qualified_name, "PyTypeObject.tp_bases");
        assert_eq!(
            qualification.new_segments,
            vec!["PyTypeObject".to_string(), "tp_bases".to_string()]
        );
    }

    #[test]
    fn test_restore_containers_restores_prior_scope() {
        // Given
        let mut scope = CScope::default();
        scope.push_containers(&segments(&["Outer"]));
        let saved = scope.push_containers(&segments(&["Inner"]));

        // When — restore once "Inner"'s body is done.
        scope.restore_containers(saved);
        let qualification = scope.qualify("field");

        // Then — only "Outer" remains in scope.
        assert_eq!(qualification.qualified_name, "Outer.field");
    }

    #[test]
    fn test_push_containers_nests_under_the_current_scope() {
        // Given
        let mut scope = CScope::default();
        scope.push_containers(&segments(&["Outer"]));

        // When
        scope.push_containers(&segments(&["Inner"]));

        // Then — the pushed segments extend, rather than replace, what was
        // already current.
        assert_eq!(scope.qualify("field").qualified_name, "Outer.Inner.field");
    }

    #[test]
    fn test_reference_candidates_yields_only_the_bare_name_with_empty_scope() {
        // Given
        let scope = CScope::default();

        // When
        let candidates = scope.reference_candidates("count", TargetSearchOrder::MostQualifiedFirst);

        // Then
        assert_eq!(candidates, vec!["count".to_string()]);
    }

    #[test]
    fn test_reference_candidates_tries_the_enclosing_container_then_the_bare_name() {
        // Given — the bug's own reproducer: `` :c:member:`digits` `` written
        // inside `.. c:type:: PyLongExport`'s own body.
        let mut scope = CScope::default();
        scope.push_containers(&segments(&["PyLongExport"]));

        // When
        let candidates =
            scope.reference_candidates("digits", TargetSearchOrder::MostQualifiedFirst);

        // Then
        assert_eq!(
            candidates,
            vec!["PyLongExport.digits".to_string(), "digits".to_string()]
        );
    }

    #[test]
    fn test_reference_candidates_walks_every_ancestor_for_multi_level_nesting() {
        // Given — a union nested inside a struct: `Outer.Inner.field` must be
        // tried, then `Outer.field` (dropping only the innermost container,
        // mirroring Sphinx's `Symbol.find_declaration` walking up one scope
        // at a time), then the bare name.
        let mut scope = CScope::default();
        scope.push_containers(&segments(&["Outer"]));
        scope.push_containers(&segments(&["Inner"]));

        // When
        let candidates = scope.reference_candidates("field", TargetSearchOrder::MostQualifiedFirst);

        // Then
        assert_eq!(
            candidates,
            vec![
                "Outer.Inner.field".to_string(),
                "Outer.field".to_string(),
                "field".to_string(),
            ]
        );
    }

    #[test]
    fn test_reference_candidates_least_qualified_first_reverses_the_tier_order() {
        // Given
        let mut scope = CScope::default();
        scope.push_containers(&segments(&["PyLongExport"]));

        // When
        let candidates =
            scope.reference_candidates("digits", TargetSearchOrder::LeastQualifiedFirst);

        // Then
        assert_eq!(
            candidates,
            vec!["digits".to_string(), "PyLongExport.digits".to_string()]
        );
    }

    #[test]
    fn test_set_namespace_qualifies_subsequent_declarations() {
        // Given
        let mut scope = CScope::default();

        // When — `.. c:namespace:: A.B`.
        scope.set_namespace(Some("A.B"));

        // Then
        assert_eq!(scope.qualify("name").qualified_name, "A.B.name");
    }

    #[test]
    fn test_set_namespace_none_resets_to_global_scope() {
        // Given — the `.. c:namespace:: NULL` form, after some scope exists.
        let mut scope = CScope::default();
        scope.set_namespace(Some("A.B"));

        // When
        scope.set_namespace(None);

        // Then
        assert_eq!(scope.qualify("name").qualified_name, "name");
    }

    #[test]
    fn test_set_namespace_replaces_rather_than_extends_the_current_scope() {
        // Given — `c:namespace` is absolute, unlike `c:namespace-push`.
        let mut scope = CScope::default();
        scope.set_namespace(Some("A"));

        // When
        scope.set_namespace(Some("B"));

        // Then — not "A.B".
        assert_eq!(scope.qualify("name").qualified_name, "B.name");
    }

    #[test]
    fn test_push_namespace_extends_the_current_scope_relatively() {
        // Given — real Sphinx's own example: after `.. c:namespace:: A.B`
        // then `.. c:namespace-push:: C.D`, the scope is `A.B.C.D`.
        let mut scope = CScope::default();
        scope.set_namespace(Some("A.B"));

        // When
        scope.push_namespace("C.D");

        // Then
        assert_eq!(scope.qualify("name").qualified_name, "A.B.C.D.name");
    }

    #[test]
    fn test_pop_namespace_undoes_a_whole_multi_segment_push() {
        // Given — real Sphinx documents pop as undoing the previous *push*,
        // not just one scope segment: after pushing "C.D" onto "A.B", a pop
        // returns to "A.B", not "A.B.C".
        let mut scope = CScope::default();
        scope.set_namespace(Some("A.B"));
        scope.push_namespace("C.D");

        // When
        scope.pop_namespace();

        // Then
        assert_eq!(scope.qualify("name").qualified_name, "A.B.name");
    }

    #[test]
    fn test_pop_namespace_without_a_prior_push_yields_global_scope() {
        // Given — no push at all, just the default scope.
        let mut scope = CScope::default();

        // When
        scope.pop_namespace();

        // Then — global scope, and the never-empty invariant survives.
        assert_eq!(scope.qualify("name").qualified_name, "name");
    }

    #[test]
    fn test_pop_namespace_after_set_namespace_yields_global_scope() {
        // Given — the spec's stated equivalence: `.. c:namespace:: A.B` is
        // `.. c:namespace:: NULL` plus `.. c:namespace-push:: A.B`, so the
        // pop that undoes it lands at global scope.
        let mut scope = CScope::default();
        scope.set_namespace(Some("A.B"));

        // When
        scope.pop_namespace();

        // Then
        assert_eq!(scope.qualify("name").qualified_name, "name");
    }

    #[test]
    fn test_namespace_reset_inside_a_nested_body_does_not_break_its_restore() {
        // Given — the case a depth-based restore gets wrong: a namespace is
        // pushed, a container body is entered, and a `c:namespace` reset
        // inside that body shrinks the scope stack below the depth captured
        // on entry.
        let mut scope = CScope::default();
        scope.push_namespace("Foo");
        let saved = scope.push_containers(&segments(&["Bar"]));
        scope.set_namespace(Some("Baz"));
        assert_eq!(scope.qualify("inner").qualified_name, "Baz.inner");

        // When — the container body closes.
        scope.restore_containers(saved);

        // Then — back to the pushed namespace, not left stranded at "Baz".
        assert_eq!(scope.qualify("name").qualified_name, "Foo.name");
    }

    #[test]
    fn test_namespace_null_inside_a_nested_body_unqualifies_declarations() {
        // Given — the real CPython `c-api/memory.rst` shape: enum-style
        // macros nested in a `c:type` body, preceded by `.. c:namespace::
        // NULL` so they document bare.
        let mut scope = CScope::default();
        let saved = scope.push_containers(&segments(&["PyMemAllocatorDomain"]));

        // When
        scope.set_namespace(None);

        // Then — the nested declaration is unqualified despite the body.
        assert_eq!(
            scope.qualify("PYMEM_DOMAIN_RAW").qualified_name,
            "PYMEM_DOMAIN_RAW"
        );

        // And — the body's own close still restores global scope.
        scope.restore_containers(saved);
        assert_eq!(scope.qualify("after").qualified_name, "after");
    }

    #[test]
    fn test_unmatched_namespace_push_does_not_escape_a_nested_body() {
        // Given — a deliberate deviation from real Sphinx, whose
        // `before_content`/`after_content` restore only the current-scope
        // pointer and not the namespace stack: here the body-close restore
        // replaces the whole stack, so an unmatched push is contained.
        let mut scope = CScope::default();
        let saved = scope.push_containers(&segments(&["Bar"]));
        scope.push_namespace("Leaked");

        // When
        scope.restore_containers(saved);

        // Then — the push is gone, and a later unrelated pop cannot consume
        // it to drag the scope somewhere unexpected.
        assert_eq!(scope.qualify("name").qualified_name, "name");
        scope.pop_namespace();
        assert_eq!(scope.qualify("name").qualified_name, "name");
    }

    #[test]
    fn test_reference_candidates_follow_the_current_namespace() {
        // Given — namespace scope feeds reference resolution too, not just
        // definition qualification.
        let mut scope = CScope::default();
        scope.set_namespace(Some("A.B"));

        // When
        let candidates = scope.reference_candidates("name", TargetSearchOrder::MostQualifiedFirst);

        // Then
        assert_eq!(
            candidates,
            vec![
                "A.B.name".to_string(),
                "A.name".to_string(),
                "name".to_string(),
            ]
        );
    }
}

use rusty_sphinx_ast::{Domain, TargetSearchOrder};

/// The `py`-domain's enclosing lexical scope while indexing/rendering: the
/// most recently seen `py:module` (document-order, persists across siblings)
/// and the stack of enclosing `py:class`/`py:exception` names (lexical,
/// pushed/popped around a nested body).
///
/// Kept as two separate fields rather than one pre-flattened qualifier
/// string, because they behave differently when a domain object's own
/// (possibly dotted) signature repeats part of the scope: real Sphinx dedups
/// a repeated *class* name (`.. method:: Random.seed` inside
/// `.. class:: Random` must not double to `Random.Random.seed`) but always
/// raw-concatenates the *module* (`.. classmethod:: datetime.strptime` at
/// module level, with no enclosing class, must yield
/// `datetime.datetime.strptime` — the `datetime.` in the signature is the
/// *class* name, not a repeat of the module, and there is nothing to dedup
/// against). A single flattened string can't tell those apart once the class
/// name and module name coincide, which is exactly what happens in
/// `CPython`'s own `datetime` docs.
///
/// Named for the domain it models — the `c` domain has no class nesting and
/// a different namespacing concept (`c:namespace`), so it needs a different
/// structure, not a variant of this one.
///
/// Transient: built fresh by each of the analyzer's and renderer's document
/// walks, never stored in the AST.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PythonScope {
    module: Option<String>,
    classes: Vec<String>,
}

/// The result of [`PythonScope::qualify`]: the object's fully qualified name,
/// plus the segments its own (possibly dotted) name contributes beyond the
/// scope it was qualified against — the raw material
/// [`rusty_sphinx_ast::DomainObjectBody::deduce_local_scope`] slices to
/// decide what a nested body should push onto the class stack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Qualification {
    pub qualified_name: String,
    pub new_segments: Vec<String>,
}

impl PythonScope {
    /// Sets the current `py:module`. Document-order state — never popped by
    /// [`Self::truncate_classes`], matching real Sphinx: a module stays
    /// current for the rest of the document until another `py:module`
    /// changes it.
    pub fn set_module(&mut self, name: &str) {
        self.module = Some(name.to_string());
    }

    /// Clears the current `py:module`, so later references qualify as if
    /// none had ever been set — the `.. currentmodule:: None` reset. The
    /// class stack is untouched, matching [`Self::set_module`]'s
    /// document-order semantics.
    pub fn clear_module(&mut self) {
        self.module = None;
    }

    /// Temporarily overrides the current `py:module` for the duration of one
    /// domain object's own qualification and nested body — the `:module:`
    /// option's effect, mirroring real Sphinx's `PyObject.before_content()`
    /// pushing `ref_context['py:module']` (and `after_content()` popping it,
    /// via [`Self::restore_module`]). Unlike [`Self::set_module`], this is
    /// lexically scoped, not document-order: the caller must restore the
    /// prior value once the object (including its nested body) is fully
    /// processed, exactly like [`Self::push_classes`]/[`Self::truncate_classes`].
    ///
    /// An empty `module` clears the module instead of setting it, matching
    /// real Sphinx's `add_target_and_index`, which only prefixes when
    /// `options.get('module', ...)` is truthy — so `:module:` written with no
    /// value deliberately un-qualifies the object even with an ambient
    /// module in scope.
    ///
    /// Returns the previous module so the caller can restore it via
    /// [`Self::restore_module`].
    pub fn push_module_override(&mut self, module: &str) -> Option<String> {
        let previous = self.module.take();
        if !module.is_empty() {
            self.module = Some(module.to_string());
        }
        previous
    }

    /// Restores `py:module` to a value previously returned by
    /// [`Self::push_module_override`] — real Sphinx's `after_content()` pop.
    pub fn restore_module(&mut self, previous: Option<String>) {
        self.module = previous;
    }

    /// Pushes lexically nested class segments (innermost last) onto the
    /// scope, returning the prior depth so a caller can restore it with
    /// [`Self::truncate_classes`] once the nested body has been processed.
    pub fn push_classes(&mut self, segments: &[String]) -> usize {
        let depth = self.classes.len();
        self.classes.extend_from_slice(segments);
        depth
    }

    /// Restores the class stack to a depth previously returned by
    /// [`Self::push_classes`], popping whatever was pushed since.
    pub fn truncate_classes(&mut self, depth: usize) {
        self.classes.truncate(depth);
    }

    /// The qualified names a *reference* to `name` may denote, in the order
    /// they should be tried, deduplicated.
    ///
    /// This is the reference-side counterpart to [`Self::qualify`], and the
    /// two deliberately differ. `qualify` serves *definitions*: it absorbs a
    /// class prefix a signature repeats, because a definition names itself
    /// once and the index key must not double it. Here nothing is absorbed —
    /// a reference's text is the author's own words and is never rewritten
    /// before lookup; a repeat is covered by simply having a tier for it.
    ///
    /// Sphinx's two orders (see [`TargetSearchOrder`]) are not mirror images:
    /// the default order has a class-only tier (`ZipFile.read` resolving
    /// without the module in scope), which the dot-prefixed order omits.
    ///
    /// Module and class context are `py`-domain concepts, so every other
    /// domain yields just the literal name — the `c` domain namespaces with
    /// `c:namespace`, which this type does not model.
    #[must_use]
    pub fn reference_candidates(
        &self,
        domain: Domain,
        name: &str,
        order: TargetSearchOrder,
    ) -> Vec<String> {
        if domain != Domain::Py {
            return vec![name.to_string()];
        }

        let module = self.module.as_deref();
        let class = (!self.classes.is_empty()).then(|| self.classes.join("."));
        let prefixed = |prefix: &str| format!("{prefix}.{name}");
        let module_and_class = || match (module, class.as_deref()) {
            (Some(module), Some(class)) => Some(prefixed(&format!("{module}.{class}"))),
            _ => None,
        };

        let tiers = match order {
            TargetSearchOrder::LeastQualifiedFirst => [
                Some(name.to_string()),
                class.as_deref().map(prefixed),
                module.map(prefixed),
                module_and_class(),
            ],
            TargetSearchOrder::MostQualifiedFirst => [
                module_and_class(),
                module.map(prefixed),
                Some(name.to_string()),
                None,
            ],
        };

        let mut candidates: Vec<String> = Vec::with_capacity(tiers.len());
        for candidate in tiers.into_iter().flatten() {
            if !candidates.contains(&candidate) {
                candidates.push(candidate);
            }
        }
        candidates
    }

    /// Qualifies `own_name` (a `py`-domain object's own, possibly dotted,
    /// name) against this scope.
    ///
    /// This is the *definition* side — see [`Self::reference_candidates`] for
    /// how a reference to a name is resolved, and why the two differ. Only
    /// ever called for `py`-domain objects: `c`-domain objects qualify
    /// against [`crate::CScope`] instead (see that type's doc comment).
    ///
    /// A dotted `own_name` that repeats (all or just the innermost segment
    /// of) the current class stack has that repeat absorbed, exactly once,
    /// before the scope is prefixed — the class stack is never compared
    /// against the module, so a class name that happens to equal the module
    /// name (`datetime`/`datetime`) is never mistaken for a module repeat.
    /// The module is always included when set, unlike the retired
    /// `effective_qualifier` this superseded, which gated it on domain.
    #[must_use]
    pub fn qualify(&self, own_name: &str) -> Qualification {
        let sig_segments: Vec<&str> = own_name.split('.').collect();
        let new_segments = self.absorb(&sig_segments);

        let mut parts: Vec<&str> = Vec::new();
        if let Some(module) = &self.module {
            parts.push(module.as_str());
        }
        parts.extend(self.classes.iter().map(String::as_str));
        parts.extend(new_segments.iter().copied());

        Qualification {
            qualified_name: parts.join("."),
            new_segments: new_segments.into_iter().map(str::to_string).collect(),
        }
    }

    /// Drops a leading repeat of the class stack from `sig_segments`: the
    /// whole stack if `sig_segments` starts with it, else just its innermost
    /// (last) entry if `sig_segments` starts with that alone, else nothing.
    /// Empty when there is no class scope to compare against — a signature
    /// is never absorbed against the module.
    fn absorb<'a>(&self, sig_segments: &[&'a str]) -> Vec<&'a str> {
        if self.classes.is_empty() {
            return sig_segments.to_vec();
        }
        if sig_segments.len() >= self.classes.len()
            && sig_segments[..self.classes.len()]
                .iter()
                .zip(&self.classes)
                .all(|(sig, class)| *sig == class)
        {
            return sig_segments[self.classes.len()..].to_vec();
        }
        if sig_segments.first() == self.classes.last().map(String::as_str).as_ref() {
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
        let scope = PythonScope::default();

        // When
        let qualification = scope.qualify("greet");

        // Then
        assert_eq!(qualification.qualified_name, "greet");
        assert_eq!(qualification.new_segments, vec!["greet".to_string()]);
    }

    #[test]
    fn test_qualify_prefixes_with_module_for_py_domain() {
        // Given
        let mut scope = PythonScope::default();
        scope.set_module("types");

        // When
        let qualification = scope.qualify("coroutine");

        // Then
        assert_eq!(qualification.qualified_name, "types.coroutine");
    }

    #[test]
    fn test_qualify_absorbs_whole_class_repeat() {
        // Given — CPython's `random.rst`: `.. method:: Random.seed` inside
        // `.. class:: Random`, itself under `.. module:: random`.
        let mut scope = PythonScope::default();
        scope.set_module("random");
        scope.push_classes(&segments(&["Random"]));

        // When
        let qualification = scope.qualify("Random.seed");

        // Then — not doubled to "random.Random.Random.seed".
        assert_eq!(qualification.qualified_name, "random.Random.seed");
        assert_eq!(qualification.new_segments, vec!["seed".to_string()]);
    }

    #[test]
    fn test_qualify_absorbs_innermost_class_repeat_in_two_level_nesting() {
        // Given — a signature repeating only the innermost of two nested
        // classes.
        let mut scope = PythonScope::default();
        scope.push_classes(&segments(&["Outer"]));
        scope.push_classes(&segments(&["Inner"]));

        // When
        let qualification = scope.qualify("Inner.method");

        // Then
        assert_eq!(qualification.qualified_name, "Outer.Inner.method");
    }

    #[test]
    fn test_qualify_absorbs_whole_two_level_class_repeat() {
        // Given — a signature repeating the entire two-level class stack.
        let mut scope = PythonScope::default();
        scope.push_classes(&segments(&["Outer"]));
        scope.push_classes(&segments(&["Inner"]));

        // When
        let qualification = scope.qualify("Outer.Inner.method");

        // Then — not doubled to "Outer.Inner.Outer.Inner.method".
        assert_eq!(qualification.qualified_name, "Outer.Inner.method");
    }

    #[test]
    fn test_qualify_does_not_absorb_module_shaped_prefix_with_no_class_scope() {
        // Given — the regression case for the class/module conflation bug:
        // CPython's `datetime.rst` documents `.. classmethod::
        // datetime.strptime` at column 0, with no enclosing `.. class::` —
        // the `datetime.` in the signature is the *class* name, which merely
        // coincides with the module name, and must not be dedup'd away.
        let mut scope = PythonScope::default();
        scope.set_module("datetime");

        // When
        let qualification = scope.qualify("datetime.strptime");

        // Then
        assert_eq!(qualification.qualified_name, "datetime.datetime.strptime");
        assert_eq!(
            qualification.new_segments,
            vec!["datetime".to_string(), "strptime".to_string()]
        );
    }

    #[test]
    fn test_qualify_matches_class_repeat_case_sensitively() {
        // Given — CPython's `zipfile.rst`: `.. method:: ZipFile.read` under
        // `.. module:: zipfile`. The class name differs from the module name
        // only by case, and `ZipFile.` must not be mistaken for a repeat of
        // `zipfile.` — a hazard that cannot arise here because absorption
        // only ever compares against `classes`, never `module`.
        let mut scope = PythonScope::default();
        scope.set_module("zipfile");
        scope.push_classes(&segments(&["ZipFile"]));

        // When
        let qualification = scope.qualify("ZipFile.read");

        // Then
        assert_eq!(qualification.qualified_name, "zipfile.ZipFile.read");
    }

    #[test]
    fn test_truncate_classes_restores_prior_depth() {
        // Given — push "Outer", then push "Inner" for its nested body,
        // capturing the depth from before the "Inner" push.
        let mut scope = PythonScope::default();
        scope.push_classes(&segments(&["Outer"]));
        let depth = scope.push_classes(&segments(&["Inner"]));

        // When — truncate back to that depth once "Inner"'s body is done.
        scope.truncate_classes(depth);
        let qualification = scope.qualify("method");

        // Then — only "Outer" remains on the class stack.
        assert_eq!(qualification.qualified_name, "Outer.method");
    }

    #[test]
    fn test_push_classes_returns_prior_depth() {
        // Given
        let mut scope = PythonScope::default();
        scope.push_classes(&segments(&["Outer"]));

        // When
        let depth = scope.push_classes(&segments(&["Inner"]));

        // Then
        assert_eq!(depth, 1);
    }

    #[test]
    fn test_set_module_persists_across_truncate_classes() {
        // Given — module state must survive class-scope pop/push, matching
        // real Sphinx's sequential (not lexically nested) module context.
        let mut scope = PythonScope::default();
        scope.set_module("email.mime");
        let depth = scope.push_classes(&segments(&["MIMEText"]));
        scope.truncate_classes(depth);

        // When
        let qualification = scope.qualify("MIMEText");

        // Then
        assert_eq!(qualification.qualified_name, "email.mime.MIMEText");
    }

    #[test]
    fn test_qualify_returns_name_unchanged_when_already_fully_qualified_under_bare_class() {
        // Given — CPython's `exceptions.rst`: `.. attribute::
        // StopIteration.value` nested inside `.. exception::
        // StopIteration`, with no enclosing module.
        let mut scope = PythonScope::default();
        scope.push_classes(&segments(&["StopIteration"]));

        // When
        let qualification = scope.qualify("StopIteration.value");

        // Then
        assert_eq!(qualification.qualified_name, "StopIteration.value");
    }

    #[test]
    fn test_reference_candidates_least_qualified_first_yields_all_four_tiers() {
        // Given — a reference written inside `.. class:: zipimporter` under
        // `.. module:: zipimport`.
        let mut scope = PythonScope::default();
        scope.set_module("zipimport");
        scope.push_classes(&segments(&["zipimporter"]));

        // When
        let candidates = scope.reference_candidates(
            Domain::Py,
            "find_spec",
            TargetSearchOrder::LeastQualifiedFirst,
        );

        // Then — Sphinx's documented default: unqualified first, then
        // progressively more scope.
        assert_eq!(
            candidates,
            segments(&[
                "find_spec",
                "zipimporter.find_spec",
                "zipimport.find_spec",
                "zipimport.zipimporter.find_spec",
            ])
        );
    }

    #[test]
    fn test_reference_candidates_most_qualified_first_omits_the_class_only_tier() {
        // Given — the same scope, but the target was written with a leading
        // dot.
        let mut scope = PythonScope::default();
        scope.set_module("zipimport");
        scope.push_classes(&segments(&["zipimporter"]));

        // When
        let candidates = scope.reference_candidates(
            Domain::Py,
            "find_spec",
            TargetSearchOrder::MostQualifiedFirst,
        );

        // Then — not the reverse of the default order: real Sphinx's
        // dot-prefixed branch never tries a class without its module.
        assert_eq!(
            candidates,
            segments(&[
                "zipimport.zipimporter.find_spec",
                "zipimport.find_spec",
                "find_spec",
            ])
        );
    }

    #[test]
    fn test_reference_candidates_joins_nested_classes_into_one_qualifier() {
        // Given — a class nested inside another class.
        let mut scope = PythonScope::default();
        scope.set_module("pkg");
        scope.push_classes(&segments(&["Outer", "Inner"]));

        // When
        let candidates =
            scope.reference_candidates(Domain::Py, "run", TargetSearchOrder::MostQualifiedFirst);

        // Then
        assert_eq!(
            candidates,
            segments(&["pkg.Outer.Inner.run", "pkg.run", "run"])
        );
    }

    #[test]
    fn test_reference_candidates_skips_tiers_with_no_module_in_scope() {
        // Given — a class scope but no `.. module::` yet.
        let mut scope = PythonScope::default();
        scope.push_classes(&segments(&["ZipFile"]));

        // When
        let candidates =
            scope.reference_candidates(Domain::Py, "read", TargetSearchOrder::LeastQualifiedFirst);

        // Then — the two module-bearing tiers simply do not exist.
        assert_eq!(candidates, segments(&["read", "ZipFile.read"]));
    }

    #[test]
    fn test_reference_candidates_collapses_to_the_bare_name_without_any_scope() {
        // Given / When
        let candidates = PythonScope::default().reference_candidates(
            Domain::Py,
            "greet",
            TargetSearchOrder::LeastQualifiedFirst,
        );

        // Then — every tier coincides, and duplicates are dropped rather
        // than looked up repeatedly.
        assert_eq!(candidates, segments(&["greet"]));
    }

    #[test]
    fn test_clear_module_drops_module_from_qualify() {
        // Given
        let mut scope = PythonScope::default();
        scope.set_module("enum");

        // When
        scope.clear_module();
        let qualification = scope.qualify("Enum");

        // Then
        assert_eq!(qualification.qualified_name, "Enum");
    }

    #[test]
    fn test_clear_module_drops_module_bearing_tiers_from_reference_candidates() {
        // Given
        let mut scope = PythonScope::default();
        scope.set_module("enum");

        // When
        scope.clear_module();
        let candidates =
            scope.reference_candidates(Domain::Py, "Enum", TargetSearchOrder::LeastQualifiedFirst);

        // Then
        assert_eq!(candidates, segments(&["Enum"]));
    }

    #[test]
    fn test_clear_module_leaves_class_stack_intact() {
        // Given — mirrors `test_set_module_persists_across_truncate_classes`:
        // clearing the module is likewise independent of the class stack.
        let mut scope = PythonScope::default();
        scope.set_module("enum");
        scope.push_classes(&segments(&["Flag"]));

        // When
        scope.clear_module();
        let qualification = scope.qualify("name");

        // Then
        assert_eq!(qualification.qualified_name, "Flag.name");
    }

    #[test]
    fn test_reference_candidates_ignores_scope_outside_the_py_domain() {
        // Given — module context is a `py` concept; the `c` domain
        // namespaces differently and must not inherit it.
        let mut scope = PythonScope::default();
        scope.set_module("zipimport");
        scope.push_classes(&segments(&["zipimporter"]));

        // When
        let candidates = scope.reference_candidates(
            Domain::C,
            "PyList_Append",
            TargetSearchOrder::MostQualifiedFirst,
        );

        // Then
        assert_eq!(candidates, segments(&["PyList_Append"]));
    }

    #[test]
    fn test_reference_candidates_does_not_absorb_a_repeated_class_prefix() {
        // Given — the CPython `random` shape: a reference written
        // `Random.seed` from inside `.. class:: Random`. `qualify` would
        // absorb the repeat; a reference must not be rewritten, and does not
        // need to be — the module+class tier resolves it.
        let mut scope = PythonScope::default();
        scope.set_module("random");
        scope.push_classes(&segments(&["Random"]));

        // When
        let candidates = scope.reference_candidates(
            Domain::Py,
            "Random.seed",
            TargetSearchOrder::LeastQualifiedFirst,
        );

        // Then
        assert_eq!(
            candidates,
            segments(&[
                "Random.seed",
                "Random.Random.seed",
                "random.Random.seed",
                "random.Random.Random.seed",
            ])
        );
    }

    #[test]
    fn test_push_module_override_replaces_current_module_and_returns_previous() {
        // Given
        let mut scope = PythonScope::default();
        scope.set_module("multiprocessing.shared_memory");

        // When
        let previous = scope.push_module_override("multiprocessing.managers");

        // Then
        assert_eq!(previous.as_deref(), Some("multiprocessing.shared_memory"));
        assert_eq!(
            scope.qualify("SharedMemoryManager").qualified_name,
            "multiprocessing.managers.SharedMemoryManager"
        );
    }

    #[test]
    fn test_push_module_override_returns_none_with_no_prior_module() {
        // Given
        let mut scope = PythonScope::default();

        // When
        let previous = scope.push_module_override("ctypes.util");

        // Then
        assert_eq!(previous, None);
        assert_eq!(
            scope.qualify("find_library").qualified_name,
            "ctypes.util.find_library"
        );
    }

    #[test]
    fn test_restore_module_puts_back_previous_value() {
        // Given
        let mut scope = PythonScope::default();
        scope.set_module("multiprocessing.shared_memory");
        let previous = scope.push_module_override("multiprocessing.managers");

        // When
        scope.restore_module(previous);

        // Then
        assert_eq!(
            scope.qualify("SharedMemoryManager").qualified_name,
            "multiprocessing.shared_memory.SharedMemoryManager"
        );
    }

    #[test]
    fn test_restore_module_puts_back_none_when_there_was_no_prior_module() {
        // Given
        let mut scope = PythonScope::default();
        let previous = scope.push_module_override("ctypes.util");

        // When
        scope.restore_module(previous);

        // Then
        assert_eq!(scope.qualify("find_library").qualified_name, "find_library");
    }

    #[test]
    fn test_push_module_override_with_empty_value_clears_the_module() {
        // Given — real Sphinx's `add_target_and_index` only prefixes when
        // `options.get('module', ...)` is truthy, so a bare `:module:` (an
        // empty option value) un-qualifies the object even with an ambient
        // module in scope.
        let mut scope = PythonScope::default();
        scope.set_module("ctypes");

        // When
        scope.push_module_override("");

        // Then
        assert_eq!(scope.qualify("greet").qualified_name, "greet");
    }
}

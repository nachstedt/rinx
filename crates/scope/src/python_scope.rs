use rusty_sphinx_ast::Domain;

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

    /// Qualifies `own_name` (a domain object's own, possibly dotted, name)
    /// against this scope for an object of the given `domain`.
    ///
    /// A dotted `own_name` that repeats (all or just the innermost segment
    /// of) the current class stack has that repeat absorbed, exactly once,
    /// before the scope is prefixed — the class stack is never compared
    /// against the module, so a class name that happens to equal the module
    /// name (`datetime`/`datetime`) is never mistaken for a module repeat.
    /// The module is included only when `domain` is [`Domain::Py`], or when
    /// a class scope applies at all (an enclosing class's own qualified name
    /// already had the module folded in when the class itself was
    /// qualified) — mirroring the domain gating the retired
    /// `effective_qualifier` used to apply.
    #[must_use]
    pub fn qualify(&self, domain: Domain, own_name: &str) -> Qualification {
        let sig_segments: Vec<&str> = own_name.split('.').collect();
        let new_segments = self.absorb(&sig_segments);

        let include_module = domain == Domain::Py || !self.classes.is_empty();
        let mut parts: Vec<&str> = Vec::new();
        if include_module && let Some(module) = &self.module {
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
        let qualification = scope.qualify(Domain::Py, "greet");

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
        let qualification = scope.qualify(Domain::Py, "coroutine");

        // Then
        assert_eq!(qualification.qualified_name, "types.coroutine");
    }

    #[test]
    fn test_qualify_never_applies_module_to_c_domain_with_no_class_scope() {
        // Given — a `c:function` written as a later sibling after
        // `py:module:: types` must not get "types." prepended.
        let mut scope = PythonScope::default();
        scope.set_module("types");

        // When
        let qualification = scope.qualify(Domain::C, "PyList_Append");

        // Then
        assert_eq!(qualification.qualified_name, "PyList_Append");
    }

    #[test]
    fn test_qualify_absorbs_whole_class_repeat() {
        // Given — CPython's `random.rst`: `.. method:: Random.seed` inside
        // `.. class:: Random`, itself under `.. module:: random`.
        let mut scope = PythonScope::default();
        scope.set_module("random");
        scope.push_classes(&segments(&["Random"]));

        // When
        let qualification = scope.qualify(Domain::Py, "Random.seed");

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
        let qualification = scope.qualify(Domain::Py, "Inner.method");

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
        let qualification = scope.qualify(Domain::Py, "Outer.Inner.method");

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
        let qualification = scope.qualify(Domain::Py, "datetime.strptime");

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
        let qualification = scope.qualify(Domain::Py, "ZipFile.read");

        // Then
        assert_eq!(qualification.qualified_name, "zipfile.ZipFile.read");
    }

    #[test]
    fn test_qualify_includes_module_for_c_domain_when_class_scope_present() {
        // Given — a `c:function` (structurally) nested inside a `py:class`
        // body: the class's own qualified name already had the module
        // folded in, so it applies here too, matching the retired
        // `effective_qualifier`'s `class_qualifier.or(...)` precedence.
        let mut scope = PythonScope::default();
        scope.set_module("random");
        scope.push_classes(&segments(&["Random"]));

        // When
        let qualification = scope.qualify(Domain::C, "seed");

        // Then
        assert_eq!(qualification.qualified_name, "random.Random.seed");
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
        let qualification = scope.qualify(Domain::Py, "method");

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
        let qualification = scope.qualify(Domain::Py, "MIMEText");

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
        let qualification = scope.qualify(Domain::Py, "StopIteration.value");

        // Then
        assert_eq!(qualification.qualified_name, "StopIteration.value");
    }
}

/// The `c`-domain's enclosing lexical scope while indexing/rendering: the
/// stack of enclosing `c:struct`/`c:union` names (lexical, pushed/popped
/// around a nested body).
///
/// Deliberately not a variant of [`crate::PythonScope`] (see that type's own
/// doc comment) — the `c` domain has no class nesting and no module concept,
/// just a container stack, so this is `PythonScope`'s `classes` stack and its
/// `qualify`/`absorb` algorithm on their own, with nothing module-shaped
/// grafted on. `c:function`/`c:macro` do not use this type at all: they keep
/// qualifying via `PythonScope`, exactly as before this type existed
/// (including the edge case of a `c:function` nested inside a `py:class`
/// body), since nothing about their behavior needed to change.
///
/// Transient: built fresh by each of the analyzer's and renderer's document
/// walks, never stored in the AST.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CScope {
    containers: Vec<String>,
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
    /// Pushes lexically nested container segments (innermost last) onto the
    /// scope, returning the prior depth so a caller can restore it with
    /// [`Self::truncate_containers`] once the nested body has been processed.
    pub fn push_containers(&mut self, segments: &[String]) -> usize {
        let depth = self.containers.len();
        self.containers.extend_from_slice(segments);
        depth
    }

    /// Restores the container stack to a depth previously returned by
    /// [`Self::push_containers`], popping whatever was pushed since.
    pub fn truncate_containers(&mut self, depth: usize) {
        self.containers.truncate(depth);
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

        let mut parts: Vec<&str> = self.containers.iter().map(String::as_str).collect();
        parts.extend(new_segments.iter().copied());

        CQualification {
            qualified_name: parts.join("."),
            new_segments: new_segments.into_iter().map(str::to_string).collect(),
        }
    }

    /// Drops a leading repeat of the container stack from `sig_segments`:
    /// the whole stack if `sig_segments` starts with it, else just its
    /// innermost (last) entry if `sig_segments` starts with that alone, else
    /// nothing. Empty when there is no container scope to compare against.
    fn absorb<'a>(&self, sig_segments: &[&'a str]) -> Vec<&'a str> {
        if self.containers.is_empty() {
            return sig_segments.to_vec();
        }
        if sig_segments.len() >= self.containers.len()
            && sig_segments[..self.containers.len()]
                .iter()
                .zip(&self.containers)
                .all(|(sig, container)| *sig == container)
        {
            return sig_segments[self.containers.len()..].to_vec();
        }
        if sig_segments.first() == self.containers.last().map(String::as_str).as_ref() {
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
    fn test_truncate_containers_restores_prior_depth() {
        // Given
        let mut scope = CScope::default();
        scope.push_containers(&segments(&["Outer"]));
        let depth = scope.push_containers(&segments(&["Inner"]));

        // When — truncate back to that depth once "Inner"'s body is done.
        scope.truncate_containers(depth);
        let qualification = scope.qualify("field");

        // Then — only "Outer" remains on the container stack.
        assert_eq!(qualification.qualified_name, "Outer.field");
    }

    #[test]
    fn test_push_containers_returns_prior_depth() {
        // Given
        let mut scope = CScope::default();
        scope.push_containers(&segments(&["Outer"]));

        // When
        let depth = scope.push_containers(&segments(&["Inner"]));

        // Then
        assert_eq!(depth, 1);
    }
}

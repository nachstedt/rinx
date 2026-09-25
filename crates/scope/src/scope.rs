use crate::{CScope, ProgramScope, PythonScope};
use rinx_ast::{Domain, TargetSearchOrder};

/// The scope for every domain, owned as one unit and threaded through both
/// the analyzer's indexing traversal (`index_nodes`/`index_domain_object`)
/// and the renderer's matching render traversal (`render_domain_object`),
/// as well as through reference resolution
/// (`rinx_renderer::resolution::domain_object::DomainObjectResolver::resolve`)
/// — one scope argument/field regardless of how many domains exist, instead
/// of growing one parameter per domain everywhere a scope is needed.
///
/// `.python`/`.c` stay as two separate fields rather than being hidden
/// behind a single dispatch method: on the *definition* side, which of the
/// two a given object qualifies against is decided per `DomainObjectBody`
/// variant, not per domain — today that split happens to coincide with the
/// domain boundary (every `c`-domain object, including `c:function`/
/// `c:macro`, qualifies against `.c`; see [`CScope`]'s doc comment), but
/// callers still branch on the object variant (the `uses_c_scope` checks in
/// `index_domain_object` and `render_domain_object`) rather than the domain,
/// matching `deduce_local_scope`'s exhaustive-match philosophy of deciding
/// per object type. Only reference *resolution* has a clean per-domain
/// split, which is what [`Self::reference_candidates`] provides.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Scope {
    pub python: PythonScope,
    pub c: CScope,
    /// The `std`-domain "current program" context (`.. program::`),
    /// consulted only by `StdCmdoption` definitions/`:option:` references — see
    /// [`ProgramScope`]. Reference resolution for `:option:` does not go
    /// through [`Self::reference_candidates`] (its algorithm has no scope
    /// tiers), so `Domain::Std` there is inert; see that method's doc
    /// comment.
    pub program: ProgramScope,
}

impl Scope {
    /// Dispatches to the scope matching `domain`'s
    /// `reference_candidates` — see [`PythonScope::reference_candidates`]
    /// and [`CScope::reference_candidates`] for what each domain's tiers
    /// look like.
    ///
    /// `Domain::Std` has no tiered scope search of its own — `:option:`
    /// resolution is a distinct ambient-program/global-fallback/embedded-
    /// program search over `ProjectIndex::domain_objects` directly (see
    /// `rinx_renderer::resolution::option::OptionResolver`), which
    /// never calls this method. The arm below exists purely to keep this
    /// match exhaustive as `Domain` gains variants; it is not exercised by
    /// `:option:` reference resolution in practice.
    #[must_use]
    pub fn reference_candidates(
        &self,
        domain: Domain,
        name: &str,
        order: TargetSearchOrder,
    ) -> Vec<String> {
        match domain {
            Domain::Py => self.python.reference_candidates(domain, name, order),
            Domain::C => self.c.reference_candidates(name, order),
            Domain::Std => vec![name.to_string()],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reference_candidates_dispatches_to_python_scope_for_py_domain() {
        // Given
        let mut scope = Scope::default();
        scope.python.set_module("datetime");

        // When
        let candidates = scope.reference_candidates(
            Domain::Py,
            "datetime",
            TargetSearchOrder::MostQualifiedFirst,
        );

        // Then
        assert_eq!(
            candidates,
            vec!["datetime.datetime".to_string(), "datetime".to_string()]
        );
    }

    #[test]
    fn test_reference_candidates_is_inert_passthrough_for_std_domain() {
        // Given — `:option:` never calls this method (see the doc comment),
        // so this just pins that the arm exists and is harmless.
        let scope = Scope::default();

        // When
        let candidates =
            scope.reference_candidates(Domain::Std, "-X", TargetSearchOrder::LeastQualifiedFirst);

        // Then
        assert_eq!(candidates, vec!["-X".to_string()]);
    }

    #[test]
    fn test_reference_candidates_dispatches_to_c_scope_for_c_domain() {
        // Given
        let mut scope = Scope::default();
        scope.c.push_containers(&["PyLongExport".to_string()]);

        // When
        let candidates =
            scope.reference_candidates(Domain::C, "digits", TargetSearchOrder::LeastQualifiedFirst);

        // Then
        assert_eq!(
            candidates,
            vec!["digits".to_string(), "PyLongExport.digits".to_string()]
        );
    }
}

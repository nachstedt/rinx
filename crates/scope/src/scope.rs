use crate::{CScope, PythonScope};
use rusty_sphinx_ast::{Domain, TargetSearchOrder};

/// A borrowing view over every domain's scope, so reference-resolution code
/// (see `rusty_sphinx_renderer::domain_resolution::DomainObjectResolver::resolve`)
/// takes one scope argument regardless of how many domains exist, instead of
/// growing one parameter per domain.
///
/// Deliberately borrows rather than owning [`PythonScope`]/[`CScope`]: the
/// definition-side traversal (`RenderCtx`'s `python_scope`/`c_scope` fields,
/// and the analyzer's equivalent `&mut` parameters) keeps qualifying against
/// each domain's scope independently, exactly as before this type existed —
/// only reference *resolution* needs both scopes at once, so a `Scope` is
/// built transiently at that call site rather than replacing the fields it
/// borrows from.
#[derive(Debug, Clone, Copy)]
pub struct Scope<'a> {
    pub python: &'a PythonScope,
    pub c: &'a CScope,
}

impl Scope<'_> {
    /// Dispatches to the scope matching `domain`'s
    /// `reference_candidates` — see [`PythonScope::reference_candidates`]
    /// and [`CScope::reference_candidates`] for what each domain's tiers
    /// look like.
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reference_candidates_dispatches_to_python_scope_for_py_domain() {
        // Given
        let mut python = PythonScope::default();
        python.set_module("datetime");
        let c = CScope::default();
        let scope = Scope {
            python: &python,
            c: &c,
        };

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
    fn test_reference_candidates_dispatches_to_c_scope_for_c_domain() {
        // Given
        let python = PythonScope::default();
        let mut c = CScope::default();
        c.push_containers(&["PyLongExport".to_string()]);
        let scope = Scope {
            python: &python,
            c: &c,
        };

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

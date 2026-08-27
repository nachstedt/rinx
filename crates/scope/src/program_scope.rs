/// The `std`-domain "current program" context set by `.. program::` —
/// analogous to [`crate::PythonScope`]'s module concept, but with none of its
/// nesting/absorb-dedup machinery: options never nest, so there is nothing
/// to push/pop, only to set/clear (document-order state, exactly like
/// `PythonScope::set_module`/`clear_module`).
///
/// Transient: built fresh by each of the analyzer's and renderer's document
/// walks, never stored in the AST.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProgramScope {
    current: Option<String>,
}

impl ProgramScope {
    /// Sets the current program. `name` must already be normalized (real
    /// Sphinx's whitespace-to-hyphen substitution) — that normalization is
    /// the parser's job (building `Directive::StdProgram`), matching the
    /// "parse, don't validate" precedent `TargetSearchOrder` and
    /// `Directive::PyCurrentModule`'s `None`-sentinel resolution already
    /// follow.
    pub fn set(&mut self, name: &str) {
        self.current = Some(name.to_string());
    }

    /// Clears the current program — the `.. program:: None` reset form.
    pub fn clear(&mut self) {
        self.current = None;
    }

    /// The current program, if any is set.
    #[must_use]
    pub fn current(&self) -> Option<&str> {
        self.current.as_deref()
    }

    /// Qualifies `optname` against the current program:
    /// `"{program}.{optname}"` when one is set, else `optname` verbatim. No
    /// absorb/dedup logic, unlike `PythonScope::qualify`/`CScope::qualify` —
    /// options never nest, so a definition's own name never repeats the
    /// ambient program.
    #[must_use]
    pub fn qualify(&self, optname: &str) -> String {
        match &self.current {
            Some(program) => format!("{program}.{optname}"),
            None => optname.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_program_scope_qualify_prefixes_with_current_program() {
        // Given
        let mut scope = ProgramScope::default();
        scope.set("dis");

        // When
        let qualified = scope.qualify("-O");

        // Then
        assert_eq!(qualified, "dis.-O");
    }

    #[test]
    fn test_program_scope_qualify_is_bare_when_no_program_set() {
        // Given
        let scope = ProgramScope::default();

        // When
        let qualified = scope.qualify("-X");

        // Then
        assert_eq!(qualified, "-X");
    }

    #[test]
    fn test_program_scope_clear_resets_to_bare_qualification() {
        // Given
        let mut scope = ProgramScope::default();
        scope.set("dis");
        scope.clear();

        // When
        let qualified = scope.qualify("-X");

        // Then
        assert_eq!(qualified, "-X");
    }

    #[test]
    fn test_program_scope_current_reflects_set_and_clear() {
        // Given
        let mut scope = ProgramScope::default();
        assert_eq!(scope.current(), None);

        // When
        scope.set("dis");

        // Then
        assert_eq!(scope.current(), Some("dis"));

        // When
        scope.clear();

        // Then
        assert_eq!(scope.current(), None);
    }
}

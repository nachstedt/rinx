//! The roles a document defines for itself with `.. role::`.
//!
//! The one piece of *state* a parse carries, rather than configuration: a
//! `.. role::` applies from its definition onwards, as in docutils, and Sphinx
//! forgets it at the end of the document. The block parse is already
//! sequential — each paragraph's inline scan runs when the paragraph is
//! reached — so a table the directive fills in and the scan reads gives exactly
//! those semantics, with no whole-document pass and no AST node for the
//! definition itself.
//!
//! The interior mutability lives in this one type so that [`crate::ParseCtx`]
//! can stay a borrowed, copyable bundle: every nested context shares the same
//! table, which is also what makes a role defined in an `.. include::`d
//! fragment apply to the rest of the including document.

use std::cell::RefCell;
use std::collections::BTreeMap;

use rinx_ast::ResolvedLanguage;

/// What a role derived from `code` was defined with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CodeRole {
    /// The language its text is highlighted as; [`ResolvedLanguage::None`]
    /// when the definition named none.
    pub(crate) language: ResolvedLanguage,
    /// The classes the rendered `<code>` carries, already normalized.
    pub(crate) classes: Vec<String>,
}

/// The custom roles defined so far in one document, by lowercased name.
#[derive(Debug, Default)]
pub(crate) struct CustomRoles {
    roles: RefCell<BTreeMap<String, CodeRole>>,
}

impl CustomRoles {
    /// Defines `name`, replacing an earlier definition of it — docutils lets a
    /// document redefine its own role, and the later one wins from there on.
    pub(crate) fn define(&self, name: &str, role: CodeRole) {
        self.roles.borrow_mut().insert(name.to_lowercase(), role);
    }

    /// The role `name` was defined as, if it has been by now. Role names are
    /// case-insensitive, as docutils normalizes them.
    pub(crate) fn lookup(&self, name: &str) -> Option<CodeRole> {
        self.roles.borrow().get(&name.to_lowercase()).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain() -> CodeRole {
        CodeRole {
            language: ResolvedLanguage::None,
            classes: vec!["plain".to_string()],
        }
    }

    fn python() -> CodeRole {
        CodeRole {
            language: ResolvedLanguage::parse("python").unwrap(),
            classes: Vec::new(),
        }
    }

    #[test]
    fn test_lookup_misses_a_role_never_defined() {
        // Given
        let roles = CustomRoles::default();

        // When / Then
        assert_eq!(roles.lookup("python"), None);
    }

    #[test]
    fn test_lookup_finds_a_defined_role() {
        // Given
        let roles = CustomRoles::default();

        // When
        roles.define("python", python());

        // Then
        assert_eq!(roles.lookup("python"), Some(python()));
    }

    #[test]
    fn test_lookup_ignores_case() {
        // Given a role defined with capitals
        let roles = CustomRoles::default();
        roles.define("Py", python());

        // When / Then — docutils normalizes role names
        assert_eq!(roles.lookup("PY"), Some(python()));
        assert_eq!(roles.lookup("py"), Some(python()));
    }

    #[test]
    fn test_define_replaces_an_earlier_definition() {
        // Given
        let roles = CustomRoles::default();
        roles.define("x", python());

        // When
        roles.define("x", plain());

        // Then
        assert_eq!(roles.lookup("x"), Some(plain()));
    }
}

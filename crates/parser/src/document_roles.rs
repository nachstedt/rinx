//! The roles a document defines for itself with `.. role::`, and the default
//! role a `.. default-role::` picks.
//!
//! The one piece of *state* a parse carries, rather than configuration: both
//! directives apply from where they are written onwards, as in docutils, and
//! Sphinx forgets both at the end of the document. The block parse is already
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

use rinx_ast::{ResolvedLanguage, ScriptPosition};

use crate::default_role::DefaultRole;

/// What a `.. role::` defined a role as: one variant per base role a custom
/// role may derive from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CustomRole {
    /// Derived from `code`.
    Code(CodeRole),
    /// Derived from `sub`/`subscript` or `sup`/`superscript`, carrying the
    /// classes the rendered `<sub>`/`<sup>` carries, already normalized.
    Script {
        position: ScriptPosition,
        classes: Vec<String>,
    },
}

/// What a role derived from `code` was defined with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CodeRole {
    /// The language its text is highlighted as; [`ResolvedLanguage::None`]
    /// when the definition named none.
    pub(crate) language: ResolvedLanguage,
    /// The classes the rendered `<code>` carries, already normalized.
    pub(crate) classes: Vec<String>,
}

/// The custom roles defined so far in one document, by lowercased name, and
/// the default role a `.. default-role::` last chose, if any has.
#[derive(Debug, Default)]
pub(crate) struct DocumentRoles {
    roles: RefCell<BTreeMap<String, CustomRole>>,
    /// `None` until a `.. default-role::` is written, so the library's own
    /// default applies until then.
    default: RefCell<Option<DefaultRole>>,
}

impl DocumentRoles {
    /// Defines `name`, replacing an earlier definition of it — docutils lets a
    /// document redefine its own role, and the later one wins from there on.
    pub(crate) fn define(&self, name: &str, role: CustomRole) {
        self.roles.borrow_mut().insert(name.to_lowercase(), role);
    }

    /// The role `name` was defined as, if it has been by now. Role names are
    /// case-insensitive, as docutils normalizes them.
    pub(crate) fn lookup(&self, name: &str) -> Option<CustomRole> {
        self.roles.borrow().get(&name.to_lowercase()).cloned()
    }

    /// Makes `role` the default from here on, replacing the library's and any
    /// earlier `.. default-role::`'s.
    pub(crate) fn set_default(&self, role: DefaultRole) {
        *self.default.borrow_mut() = Some(role);
    }

    /// The default a `.. default-role::` chose, or `None` while none has.
    pub(crate) fn default_role(&self) -> Option<DefaultRole> {
        self.default.borrow().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain() -> CustomRole {
        CustomRole::Code(CodeRole {
            language: ResolvedLanguage::None,
            classes: vec!["plain".to_string()],
        })
    }

    fn python() -> CustomRole {
        CustomRole::Code(CodeRole {
            language: ResolvedLanguage::parse("python").unwrap(),
            classes: Vec::new(),
        })
    }

    fn chem() -> CustomRole {
        CustomRole::Script {
            position: ScriptPosition::Subscript,
            classes: vec!["chem".to_string()],
        }
    }

    #[test]
    fn test_default_role_is_unset_until_a_directive_sets_it() {
        // Given / When / Then
        assert_eq!(DocumentRoles::default().default_role(), None);
    }

    #[test]
    fn test_set_default_replaces_an_earlier_default() {
        // Given
        let roles = DocumentRoles::default();
        let ctx = crate::ParseCtx::with_domain(rinx_ast::Domain::Py);
        roles.set_default(DefaultRole::parse("any", &ctx).unwrap());

        // When
        roles.set_default(DefaultRole::TITLE_REFERENCE);

        // Then
        assert_eq!(roles.default_role(), Some(DefaultRole::TITLE_REFERENCE));
    }

    #[test]
    fn test_lookup_misses_a_role_never_defined() {
        // Given
        let roles = DocumentRoles::default();

        // When / Then
        assert_eq!(roles.lookup("python"), None);
    }

    #[test]
    fn test_lookup_finds_a_defined_role() {
        // Given
        let roles = DocumentRoles::default();

        // When
        roles.define("python", python());

        // Then
        assert_eq!(roles.lookup("python"), Some(python()));
    }

    #[test]
    fn test_lookup_ignores_case() {
        // Given a role defined with capitals
        let roles = DocumentRoles::default();
        roles.define("Py", python());

        // When / Then — docutils normalizes role names
        assert_eq!(roles.lookup("PY"), Some(python()));
        assert_eq!(roles.lookup("py"), Some(python()));
    }

    #[test]
    fn test_define_replaces_an_earlier_definition() {
        // Given
        let roles = DocumentRoles::default();
        roles.define("x", python());

        // When
        roles.define("x", plain());

        // Then
        assert_eq!(roles.lookup("x"), Some(plain()));
    }

    #[test]
    fn test_define_replaces_a_code_role_with_a_script_role() {
        // Given
        let roles = DocumentRoles::default();
        roles.define("x", python());

        // When — the later definition derives from another base
        roles.define("x", chem());

        // Then
        assert_eq!(roles.lookup("x"), Some(chem()));
    }
}

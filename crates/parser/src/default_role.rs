//! The role interpreted text gets when it is written without one: the
//! `` `text` `` docutils calls *interpreted text* and Sphinx's `default_role`
//! and `.. default-role::` give a meaning.
//!
//! Flat beside [`crate::context`] because three trees reach it: the library's
//! configuration arrives on [`ParseCtx`], a `.. default-role::` directive
//! replaces it for the rest of the document, and the inline scan applies it.

use std::fmt;

use crate::context::ParseCtx;
use crate::inline::find_role_kind;

/// A role a bare `` `text` `` is read as, already known to this build.
///
/// Opaque, so a name can only get here through [`Self::parse`], which asks the
/// inline scan whether it would match the role: a default role is a role, and
/// the one table of roles decides what one is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefaultRole(Kind);

#[derive(Debug, Clone, PartialEq, Eq)]
enum Kind {
    /// docutils' `title-reference`, which a bare `` `text` `` means unless a
    /// project or document says otherwise.
    TitleReference,
    /// Any other role, by the name it was written as.
    Role(String),
}

/// A name [`DefaultRole::parse`] found no role for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownRole {
    name: String,
}

impl fmt::Display for UnknownRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "':{}:' is not a role this build knows", self.name)
    }
}

impl std::error::Error for UnknownRole {}

impl DefaultRole {
    /// docutils' own default, `title-reference`: a bare `` `text` `` is the
    /// title of a work.
    pub const TITLE_REFERENCE: Self = Self(Kind::TitleReference);

    /// The role `name` is, if `ctx` knows one by that name: a built-in role
    /// (with its domain written out or not), an entity role the schema
    /// declares, or a role the document has defined by now.
    ///
    /// Any spelling of `title-reference` is that role itself, so a bare
    /// `` `text` `` under it is built exactly as one with no default would be.
    ///
    /// # Errors
    ///
    /// [`UnknownRole`] when no role answers to `name`.
    pub fn parse(name: &str, ctx: &ParseCtx<'_>) -> Result<Self, UnknownRole> {
        match find_role_kind(name, ctx) {
            None => Err(UnknownRole {
                name: name.to_string(),
            }),
            Some("title-reference") => Ok(Self::TITLE_REFERENCE),
            Some(_) => Ok(Self(Kind::Role(name.to_string()))),
        }
    }

    /// The name of the role a bare `` `text` `` is dispatched as, or `None`
    /// for `title-reference`, which is built directly.
    pub(crate) fn role_name(&self) -> Option<&str> {
        match &self.0 {
            Kind::TitleReference => None,
            Kind::Role(name) => Some(name),
        }
    }
}

impl Default for DefaultRole {
    fn default() -> Self {
        Self::TITLE_REFERENCE
    }
}

#[cfg(test)]
mod tests {
    use rinx_ast::Domain;

    use super::*;
    use crate::document_roles::{CodeRole, CustomRole, DocumentRoles};

    fn ctx() -> ParseCtx<'static> {
        ParseCtx::with_domain(Domain::Py)
    }

    #[test]
    fn test_parse_folds_every_title_reference_spelling() {
        // Given / When / Then
        for name in ["title-reference", "title", "t"] {
            assert_eq!(
                DefaultRole::parse(name, &ctx()),
                Ok(DefaultRole::TITLE_REFERENCE),
                "{name}"
            );
        }
    }

    #[test]
    fn test_parse_keeps_the_name_of_another_built_in_role() {
        // Given / When
        let role = DefaultRole::parse("py:func", &ctx()).unwrap();

        // Then
        assert_eq!(role.role_name(), Some("py:func"));
    }

    #[test]
    fn test_parse_refuses_an_unknown_role() {
        // Given / When
        let error = DefaultRole::parse("nonsense", &ctx()).unwrap_err();

        // Then
        assert_eq!(
            error.to_string(),
            "':nonsense:' is not a role this build knows"
        );
    }

    #[test]
    fn test_parse_refuses_what_is_not_a_role_name() {
        // Given / When / Then
        assert!(DefaultRole::parse("", &ctx()).is_err());
        assert!(DefaultRole::parse("two words", &ctx()).is_err());
        assert!(DefaultRole::parse("a`b", &ctx()).is_err());
    }

    #[test]
    fn test_parse_accepts_a_role_the_document_defined() {
        // Given
        let roles = DocumentRoles::default();
        roles.define(
            "plain",
            CustomRole::Code(CodeRole {
                language: rinx_ast::ResolvedLanguage::None,
                classes: Vec::new(),
            }),
        );
        let base = ctx();
        let ctx = base.with_document_roles(&roles);

        // When
        let role = DefaultRole::parse("plain", &ctx).unwrap();

        // Then
        assert_eq!(role.role_name(), Some("plain"));
    }

    #[test]
    fn test_role_name_is_none_for_title_reference() {
        // Given / When / Then
        assert_eq!(DefaultRole::default().role_name(), None);
    }
}

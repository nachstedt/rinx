use serde::{Deserialize, Serialize};

/// The role name every schema understands, accepting an entity of any type.
///
/// It exists so a schema that declares no roles at all can still be linked
/// from prose, which keeps roles optional sugar rather than a per-type
/// obligation.
pub const BUILTIN_ROLE: &str = "entity";

/// An inline role that references an entity from prose.
///
/// Roles are declared at the schema's top level rather than inside an entity
/// type, because the relation is many-to-many: sphinx-needs' `:need:` refers
/// to four types at once, and listing it inside each would force a reader to
/// union four lists to learn what it accepts.
///
/// A role is not required for an entity to be linkable — every entity
/// registers a target name, so `:ref:` reaches one regardless. A declared role
/// adds a type check on the link, and lets a schema spell the role name an
/// existing project already writes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleSpec {
    /// The role spelling, e.g. `req` for ``:req:`REQ_001` ``.
    pub name: String,
    /// Entity types the role may resolve to. `None` accepts any type.
    pub types: Option<Vec<String>>,
}

impl RoleSpec {
    /// The always-available role, accepting any entity type.
    #[must_use]
    pub fn builtin() -> Self {
        Self {
            name: BUILTIN_ROLE.to_string(),
            types: None,
        }
    }

    /// Reports whether this role may resolve to an entity of `type_name`.
    #[must_use]
    pub fn accepts(&self, type_name: &str) -> bool {
        match &self.types {
            None => true,
            Some(types) => types.iter().any(|t| t == type_name),
        }
    }

    /// The types this role accepts, for a diagnostic that must name them.
    #[must_use]
    pub fn accepted_types(&self) -> Option<&[String]> {
        self.types.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_role_accepts_every_type() {
        // Given
        let role = RoleSpec::builtin();

        // When / Then
        assert_eq!(role.name, "entity");
        assert!(role.accepts("req"));
        assert!(role.accepts("audit-event"));
    }

    #[test]
    fn test_role_accepts_only_its_declared_types() {
        // Given
        let role = RoleSpec {
            name: "need".to_string(),
            types: Some(vec!["req".to_string(), "spec".to_string()]),
        };

        // When / Then
        assert!(role.accepts("req"));
        assert!(role.accepts("spec"));
        assert!(!role.accepts("impl"));
    }

    #[test]
    fn test_role_reports_its_accepted_types_for_a_diagnostic() {
        // Given
        let constrained = RoleSpec {
            name: "req".to_string(),
            types: Some(vec!["req".to_string()]),
        };
        let unconstrained = RoleSpec::builtin();

        // When
        let some = constrained.accepted_types();
        let none = unconstrained.accepted_types();

        // Then
        assert_eq!(some, Some(&["req".to_string()][..]));
        assert_eq!(none, None);
    }
}

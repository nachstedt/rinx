use serde::{Deserialize, Serialize};

/// A typed edge an entity type may carry to other entities.
///
/// Relations are declared on the type that *carries* them, beside its
/// attributes, so a type's whole option vocabulary reads from one block and
/// the declared source can never drift from where the option is actually
/// written.
///
/// The consequence is that the *target* type's own block does not mention the
/// back-link it will display: back-links are derived, not declared. See
/// [`crate::backlinks`] for the derivation and the collision rules it enforces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationSpec {
    /// The option spelling on the source entity, e.g. `links` for `:links:`.
    pub name: String,
    /// Label shown by the default rendering; falls back to `name`.
    pub label: Option<String>,
    /// Entity types a target may have. `None` accepts any type.
    pub to: Option<Vec<String>>,
    /// Whether at least one target id must be named.
    pub required: bool,
    /// Whether more than one target id may be named. With `multiple` false
    /// the option takes exactly one.
    pub multiple: bool,
    /// The back-link name derived on each target. `None` derives no back-link,
    /// making the relation one-directional.
    pub incoming: Option<String>,
    /// Heading for that back-link; falls back to `incoming`.
    pub incoming_label: Option<String>,
}

impl RelationSpec {
    /// The label to display on the source entity.
    #[must_use]
    pub fn display_label(&self) -> &str {
        self.label.as_deref().unwrap_or(&self.name)
    }

    /// The heading to display for the derived back-link, if there is one.
    #[must_use]
    pub fn incoming_display_label(&self) -> Option<&str> {
        let incoming = self.incoming.as_deref()?;
        Some(self.incoming_label.as_deref().unwrap_or(incoming))
    }

    /// Reports whether an entity of type `type_name` may be a target.
    ///
    /// An undeclared `to` accepts any type, so a schema need not enumerate
    /// every type just to allow them all.
    #[must_use]
    pub fn accepts_target(&self, type_name: &str) -> bool {
        match &self.to {
            None => true,
            Some(types) => types.iter().any(|t| t == type_name),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn links_to(to: Option<Vec<&str>>) -> RelationSpec {
        RelationSpec {
            name: "links".to_string(),
            label: None,
            to: to.map(|types| types.into_iter().map(ToString::to_string).collect()),
            required: false,
            multiple: true,
            incoming: Some("linked_by".to_string()),
            incoming_label: None,
        }
    }

    #[test]
    fn test_relation_accepts_any_target_when_no_types_are_declared() {
        // Given
        let relation = links_to(None);

        // When
        let accepted = relation.accepts_target("anything");

        // Then
        assert!(accepted);
    }

    #[test]
    fn test_relation_accepts_only_the_declared_target_types() {
        // Given
        let relation = links_to(Some(vec!["spec", "impl"]));

        // When / Then
        assert!(relation.accepts_target("spec"));
        assert!(relation.accepts_target("impl"));
        assert!(!relation.accepts_target("req"));
    }

    #[test]
    fn test_relation_falls_back_to_its_name_as_the_outgoing_label() {
        // Given
        let unlabelled = links_to(None);

        // When
        let shown = unlabelled.display_label();

        // Then
        assert_eq!(shown, "links");
    }

    #[test]
    fn test_relation_falls_back_to_the_incoming_name_as_the_incoming_label() {
        // Given
        let relation = links_to(None);

        // When
        let shown = relation.incoming_display_label();

        // Then
        assert_eq!(shown, Some("linked_by"));
    }

    #[test]
    fn test_relation_prefers_a_declared_incoming_label() {
        // Given
        let mut relation = links_to(None);
        relation.incoming_label = Some("Linked by".to_string());

        // When
        let shown = relation.incoming_display_label();

        // Then
        assert_eq!(shown, Some("Linked by"));
    }

    #[test]
    fn test_relation_without_an_incoming_name_has_no_incoming_label() {
        // Given — a deliberately one-directional relation
        let mut relation = links_to(None);
        relation.incoming = None;
        relation.incoming_label = Some("ignored".to_string());

        // When
        let shown = relation.incoming_display_label();

        // Then
        assert_eq!(shown, None);
    }
}

use serde::{Deserialize, Serialize};

/// A named prose section an entity type may contain.
///
/// A section's content is fully-parsed RST — nested directives,
/// cross-references, code blocks, even further entities. That is the whole
/// distinction from an attribute: **attributes are values, sections are
/// documents**. An attribute is typed, validated, stored in the project index
/// and filterable; a section is a node tree that is none of those things.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SectionSpec {
    /// The sub-directive name, e.g. `verification-criteria` for
    /// `.. verification-criteria::` written inside the entity's body.
    pub name: String,
    /// Heading shown by the default rendering; falls back to `name`.
    pub label: Option<String>,
    /// Whether the sub-directive must appear at least once.
    pub required: bool,
    /// Whether it may appear more than once. Each occurrence is kept, in
    /// document order.
    pub multiple: bool,
}

impl SectionSpec {
    /// The heading to display, falling back to the directive name.
    #[must_use]
    pub fn display_label(&self) -> &str {
        self.label.as_deref().unwrap_or(&self.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(label: Option<&str>) -> SectionSpec {
        SectionSpec {
            name: "verification-criteria".to_string(),
            label: label.map(ToString::to_string),
            required: false,
            multiple: false,
        }
    }

    #[test]
    fn test_section_spec_falls_back_to_the_directive_name_as_its_label() {
        // Given
        let unlabelled = spec(None);

        // When
        let shown = unlabelled.display_label();

        // Then
        assert_eq!(shown, "verification-criteria");
    }

    #[test]
    fn test_section_spec_prefers_a_declared_label() {
        // Given
        let labelled = spec(Some("Verification criteria"));

        // When
        let shown = labelled.display_label();

        // Then
        assert_eq!(shown, "Verification criteria");
    }
}

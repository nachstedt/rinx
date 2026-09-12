use serde::{Deserialize, Serialize};

/// Which spelling of a diagram directive an author wrote.
///
/// Three constructs, two spellings each, in one enum because they produce one
/// node — the same consolidation [`EntityTableSource`](crate::EntityTableSource)
/// makes for `entity-table`/`needtable`, and for the same reasons: a migrating
/// project keeps its documents, a new one need not adopt another tool's
/// vocabulary, and a diagnostic should quote the name the author actually
/// wrote.
///
/// The three constructs really are different, though, and two questions
/// separate them. [`Self::is_templated`] says whether the body is expanded
/// against the entity graph before it reaches `PlantUML` — a plain
/// `.. plantuml::` is not, which is the whole reason it can keep the hash it
/// has today. [`Self::binds_enclosing_entity`] says whether the directive is
/// only legal inside an entity, with that entity bound in its template
/// context.
///
/// The spelling deliberately changes nothing about the rendered output, and no
/// diagnostic code names one: a code names the construct.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UmlSource {
    /// `.. plantuml::` — sphinxcontrib-plantuml's own name.
    PlantUml,
    /// `.. uml::` — sphinxcontrib-plantuml's shorter name for the same thing.
    Uml,
    /// `.. entity-diagram::` — this build's own name.
    EntityDiagram,
    /// `.. needuml::` — sphinx-needs' name for the same thing.
    NeedUml,
    /// `.. entity-arch::` — this build's own name for the form written inside
    /// an entity.
    EntityArch,
    /// `.. needarch::` — sphinx-needs' name for the same thing.
    NeedArch,
}

impl UmlSource {
    /// Every spelling, grouped by the construct it names.
    pub const ALL: &'static [Self] = &[
        Self::PlantUml,
        Self::Uml,
        Self::EntityDiagram,
        Self::NeedUml,
        Self::EntityArch,
        Self::NeedArch,
    ];

    /// The directive's own name, as written and as quoted in diagnostics.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PlantUml => "plantuml",
            Self::Uml => "uml",
            Self::EntityDiagram => "entity-diagram",
            Self::NeedUml => "needuml",
            Self::EntityArch => "entity-arch",
            Self::NeedArch => "needarch",
        }
    }

    /// Whether the body is a template expanded against the entity graph.
    ///
    /// False for the two plain `PlantUML` spellings, whose body reaches the
    /// compiler exactly as written. That is what lets one pipeline serve both:
    /// an unexpanded template is its own expansion, so the hash a
    /// `.. plantuml::` has always had does not change.
    #[must_use]
    pub const fn is_templated(self) -> bool {
        match self {
            Self::PlantUml | Self::Uml => false,
            Self::EntityDiagram | Self::NeedUml | Self::EntityArch | Self::NeedArch => true,
        }
    }

    /// Whether the directive is only legal inside an entity, with that entity
    /// bound in its template context.
    ///
    /// True for the `arch` pair alone. Every other spelling may be written
    /// anywhere, and an `arch` written outside an entity is diagnosed rather
    /// than expanded against an empty context.
    #[must_use]
    pub const fn binds_enclosing_entity(self) -> bool {
        matches!(self, Self::EntityArch | Self::NeedArch)
    }
}

impl std::str::FromStr for UmlSource {
    type Err = ();

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .iter()
            .copied()
            .find(|source| source.as_str() == name)
            .ok_or(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_each_spelling_names_itself() {
        // Given / When
        let names: Vec<&str> = UmlSource::ALL
            .iter()
            .map(|source| source.as_str())
            .collect();

        // Then
        assert_eq!(
            names,
            [
                "plantuml",
                "uml",
                "entity-diagram",
                "needuml",
                "entity-arch",
                "needarch"
            ]
        );
    }

    #[test]
    fn test_parsing_a_name_returns_the_spelling_it_belongs_to() {
        // Given
        let names = ["plantuml", "needuml", "entity-arch"];

        // When
        let parsed: Vec<UmlSource> = names
            .iter()
            .map(|name| UmlSource::from_str(name).unwrap())
            .collect();

        // Then
        assert_eq!(
            parsed,
            [
                UmlSource::PlantUml,
                UmlSource::NeedUml,
                UmlSource::EntityArch
            ]
        );
    }

    #[test]
    fn test_an_unrelated_directive_name_is_refused() {
        // Given
        let name = "entity-table";

        // When
        let parsed = UmlSource::from_str(name);

        // Then
        assert_eq!(parsed, Err(()));
    }

    #[test]
    fn test_the_two_directions_are_inverses() {
        // Given — the name a diagnostic quotes must round-trip to the spelling
        // that produced it
        let sources = UmlSource::ALL;

        // When
        let round_tripped: Vec<UmlSource> = sources
            .iter()
            .map(|source| UmlSource::from_str(source.as_str()).unwrap())
            .collect();

        // Then
        assert_eq!(round_tripped, sources);
    }

    #[test]
    fn test_only_the_plain_plantuml_spellings_are_untemplated() {
        // Given / When
        let untemplated: Vec<UmlSource> = UmlSource::ALL
            .iter()
            .copied()
            .filter(|source| !source.is_templated())
            .collect();

        // Then
        assert_eq!(untemplated, [UmlSource::PlantUml, UmlSource::Uml]);
    }

    #[test]
    fn test_only_the_arch_spellings_bind_an_enclosing_entity() {
        // Given / When
        let binding: Vec<UmlSource> = UmlSource::ALL
            .iter()
            .copied()
            .filter(|source| source.binds_enclosing_entity())
            .collect();

        // Then
        assert_eq!(binding, [UmlSource::EntityArch, UmlSource::NeedArch]);
    }

    #[test]
    fn test_every_spelling_that_binds_an_entity_is_also_templated() {
        // Given — an `arch` exists to draw the need it sits in, so a
        // non-templated one could not use what it binds
        for source in UmlSource::ALL.iter().copied() {
            // When / Then
            assert!(!source.binds_enclosing_entity() || source.is_templated());
        }
    }

    #[test]
    fn test_round_trips_through_json() {
        // Given
        let source = UmlSource::NeedArch;

        // When
        let json = serde_json::to_string(&source).expect("serializing cannot fail");
        let restored: UmlSource = serde_json::from_str(&json).expect("a written spelling reloads");

        // Then
        assert_eq!(restored, source);
    }
}

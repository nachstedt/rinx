use std::collections::BTreeMap;
use std::fmt;

use rinx_ast::{AttributeValue, EntityId, EntityIdError};
use serde::{Deserialize, Serialize};

use crate::pattern::ValuePattern;

/// How an entity type's ids are determined.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct IdSpec {
    /// Prepended to every id this type produces, explicit ones excepted.
    pub prefix: Option<String>,
    /// Whether the author must write an explicit `:id:`.
    pub required: bool,
    /// Attributes whose values compose the id, joined with `_`.
    pub from: Option<Vec<String>>,
    /// A pattern every id of this type should match, however it was
    /// determined — explicit, composed or generated, prefix included.
    pub pattern: Option<ValuePattern>,
}

/// Everything [`IdSpec::derive`] needs to determine one entity's id.
pub struct IdContext<'a> {
    /// The `:id:` the author wrote, if any.
    pub explicit: Option<&'a str>,
    /// The entity's already-parsed attribute values, for `from`.
    pub attributes: &'a BTreeMap<String, AttributeValue>,
    /// The entity type's name, used by the generated form.
    pub type_name: &'a str,
    /// The document the entity was written in.
    pub doc_path: &'a str,
    /// Whatever distinguishes this entity from others of its type in the same
    /// document. The parser passes the source line the directive was written
    /// on, which needs no counter threaded through every block parser and is
    /// just as deterministic.
    ///
    /// The trade-off is deliberate: inserting a line above an entity changes
    /// its generated id. That only ever affects entities with *generated*
    /// ids — and nobody can have written a link to an id they were never
    /// shown, so nothing breaks that a real reference depended on. An entity
    /// meant to be linked to is given an explicit `:id:`.
    pub discriminator: u32,
}

/// Why an id could not be determined.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdDerivationError {
    /// The type demands an explicit `:id:` and none was written.
    Required,
    /// An `id.from` attribute had no value to build the id out of.
    MissingSource { attribute: String },
    /// The composed id was not a legal [`EntityId`].
    Illegal(EntityIdError),
}

impl fmt::Display for IdDerivationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Required => write!(f, "this entity type requires an explicit `:id:`"),
            Self::MissingSource { attribute } => write!(
                f,
                "the id is derived from `{attribute}`, which has no value"
            ),
            Self::Illegal(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for IdDerivationError {}

/// An id that does not match its type's declared pattern.
///
/// Separate from [`IdDerivationError`] because the id it describes is still a
/// legal one: links to it resolve, and the entity keeps it. What is wrong is
/// the project's naming convention, not the entity's identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdPatternMismatch {
    /// The id as determined.
    pub id: String,
    /// The pattern it failed, as the schema wrote it.
    pub pattern: String,
}

impl fmt::Display for IdPatternMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "id {:?} does not match the pattern `{}`",
            self.id, self.pattern
        )
    }
}

impl std::error::Error for IdPatternMismatch {}

impl IdSpec {
    /// Determines an entity's id.
    ///
    /// Three ways, in order: an explicit `:id:`; a value composed from the
    /// attributes named by `from`; otherwise a generated one. The generated
    /// form hashes the document path, type and source line rather than the
    /// entity's content, so it is stable under an edit to the entity's own
    /// prose — and it is deterministic, which is not negotiable, because a
    /// `.ast` file is a Bazel action output cached on its inputs.
    ///
    /// # Errors
    ///
    /// Returns [`IdDerivationError`] when an explicit id is required but
    /// absent, when a `from` attribute has no value, or when the result is not
    /// a legal id.
    pub fn derive(&self, ctx: &IdContext<'_>) -> Result<EntityId, IdDerivationError> {
        if let Some(explicit) = ctx.explicit.map(str::trim).filter(|id| !id.is_empty()) {
            return EntityId::new(explicit).map_err(IdDerivationError::Illegal);
        }
        if self.required {
            return Err(IdDerivationError::Required);
        }

        let body = match &self.from {
            Some(sources) => Self::compose_from_attributes(sources, ctx.attributes)?,
            None => Self::generate(ctx),
        };
        let prefixed = match &self.prefix {
            Some(prefix) => format!("{prefix}{body}"),
            None => body,
        };
        EntityId::new(&prefixed).map_err(IdDerivationError::Illegal)
    }

    /// Checks a determined id against the type's pattern, if it declares one.
    ///
    /// Kept apart from [`IdSpec::derive`] so a caller holding an id that did
    /// not come from `derive` — one imported from a `needs.json` — checks it
    /// the same way.
    ///
    /// # Errors
    ///
    /// Returns [`IdPatternMismatch`] when a pattern is declared and the id
    /// does not match it.
    pub fn check_pattern(&self, id: &EntityId) -> Result<(), IdPatternMismatch> {
        match &self.pattern {
            Some(pattern) if !pattern.is_match(id.as_str()) => Err(IdPatternMismatch {
                id: id.as_str().to_string(),
                pattern: pattern.as_str().to_string(),
            }),
            _ => Ok(()),
        }
    }

    /// Joins the values of the `from` attributes into an id body.
    fn compose_from_attributes(
        sources: &[String],
        attributes: &BTreeMap<String, AttributeValue>,
    ) -> Result<String, IdDerivationError> {
        let mut parts = Vec::with_capacity(sources.len());
        for source in sources {
            let value = attributes
                .get(source)
                .map(ToString::to_string)
                .filter(|text| !text.trim().is_empty())
                .ok_or_else(|| IdDerivationError::MissingSource {
                    attribute: source.clone(),
                })?;
            parts.push(EntityId::sanitize_fragment(&value));
        }
        Ok(parts.join("_"))
    }

    /// Builds the fallback id from the entity's position in the project.
    fn generate(ctx: &IdContext<'_>) -> String {
        use sha2::{Digest, Sha256};
        use std::fmt::Write as _;

        let mut hasher = Sha256::new();
        hasher.update(ctx.doc_path.as_bytes());
        hasher.update([0]);
        hasher.update(ctx.type_name.as_bytes());
        hasher.update([0]);
        hasher.update(ctx.discriminator.to_string().as_bytes());
        let digest = hasher.finalize();
        let short = digest.iter().take(4).fold(String::new(), |mut acc, b| {
            let _ = write!(acc, "{b:02x}");
            acc
        });
        format!("{}-{short}", EntityId::sanitize_fragment(ctx.type_name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attributes(pairs: &[(&str, &str)]) -> BTreeMap<String, AttributeValue> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), AttributeValue::String((*v).to_string())))
            .collect()
    }

    fn context<'a>(
        explicit: Option<&'a str>,
        attrs: &'a BTreeMap<String, AttributeValue>,
    ) -> IdContext<'a> {
        IdContext {
            explicit,
            attributes: attrs,
            type_name: "req",
            doc_path: "specs/boot",
            discriminator: 0,
        }
    }

    #[test]
    fn test_derive_prefers_an_explicit_id_and_leaves_it_untouched() {
        // Given — a prefix that must not be applied on top of what was written
        let spec = IdSpec {
            prefix: Some("REQ_".to_string()),
            required: false,
            from: None,
            pattern: None,
        };
        let attrs = attributes(&[]);

        // When
        let id = spec.derive(&context(Some("CUSTOM-7"), &attrs)).unwrap();

        // Then
        assert_eq!(id.as_str(), "CUSTOM-7");
    }

    #[test]
    fn test_derive_reports_an_illegal_explicit_id_rather_than_repairing_it() {
        // Given — rewriting this would desync the entity from its `:links:`
        let spec = IdSpec::default();
        let attrs = attributes(&[]);

        // When
        let result = spec.derive(&context(Some("REQ 001"), &attrs));

        // Then
        assert_eq!(
            result,
            Err(IdDerivationError::Illegal(EntityIdError::IllegalCharacter(
                ' '
            )))
        );
    }

    #[test]
    fn test_derive_demands_an_explicit_id_when_the_type_requires_one() {
        // Given
        let spec = IdSpec {
            prefix: None,
            required: true,
            from: None,
            pattern: None,
        };
        let attrs = attributes(&[]);

        // When
        let result = spec.derive(&context(None, &attrs));

        // Then
        assert_eq!(result, Err(IdDerivationError::Required));
    }

    #[test]
    fn test_derive_composes_an_id_from_the_named_attributes() {
        // Given — the audit-event shape
        let spec = IdSpec {
            prefix: None,
            required: false,
            from: Some(vec!["name".to_string()]),
            pattern: None,
        };
        let attrs = attributes(&[("name", "os.system")]);

        // When
        let id = spec.derive(&context(None, &attrs)).unwrap();

        // Then
        assert_eq!(id.as_str(), "os.system");
    }

    #[test]
    fn test_derive_joins_several_source_attributes_with_underscores() {
        // Given
        let spec = IdSpec {
            prefix: None,
            required: false,
            from: Some(vec!["name".to_string(), "version".to_string()]),
            pattern: None,
        };
        let attrs = attributes(&[("name", "os.system"), ("version", "3.8")]);

        // When
        let id = spec.derive(&context(None, &attrs)).unwrap();

        // Then
        assert_eq!(id.as_str(), "os.system_3.8");
    }

    #[test]
    fn test_derive_sanitizes_a_composed_id_rather_than_failing() {
        // Given — a title-derived id will routinely hold spaces
        let spec = IdSpec {
            prefix: None,
            required: false,
            from: Some(vec!["title".to_string()]),
            pattern: None,
        };
        let attrs = attributes(&[("title", "The system shall boot")]);

        // When
        let id = spec.derive(&context(None, &attrs)).unwrap();

        // Then
        assert_eq!(id.as_str(), "The_system_shall_boot");
    }

    #[test]
    fn test_derive_reports_a_source_attribute_with_no_value() {
        // Given
        let spec = IdSpec {
            prefix: None,
            required: false,
            from: Some(vec!["name".to_string()]),
            pattern: None,
        };
        let attrs = attributes(&[]);

        // When
        let result = spec.derive(&context(None, &attrs));

        // Then
        assert_eq!(
            result,
            Err(IdDerivationError::MissingSource {
                attribute: "name".to_string()
            })
        );
    }

    #[test]
    fn test_derive_applies_the_prefix_to_a_generated_id() {
        // Given
        let spec = IdSpec {
            prefix: Some("REQ_".to_string()),
            required: false,
            from: None,
            pattern: None,
        };
        let attrs = attributes(&[]);

        // When
        let id = spec.derive(&context(None, &attrs)).unwrap();

        // Then
        assert!(
            id.as_str().starts_with("REQ_req-"),
            "unexpected generated id {id}"
        );
    }

    #[test]
    fn test_generated_ids_are_deterministic() {
        // Given — a `.ast` file is a Bazel output cached on its inputs
        let spec = IdSpec::default();
        let attrs = attributes(&[]);

        // When
        let first = spec.derive(&context(None, &attrs)).unwrap();
        let second = spec.derive(&context(None, &attrs)).unwrap();

        // Then
        assert_eq!(first, second);
    }

    #[test]
    fn test_generated_ids_differ_by_document_and_discriminator() {
        // Given
        let spec = IdSpec::default();
        let attrs = attributes(&[]);
        let base = context(None, &attrs);

        // When
        let here = spec.derive(&base).unwrap();
        let later = spec
            .derive(&IdContext {
                discriminator: 1,
                ..context(None, &attrs)
            })
            .unwrap();
        let elsewhere = spec
            .derive(&IdContext {
                doc_path: "specs/shutdown",
                ..context(None, &attrs)
            })
            .unwrap();

        // Then
        assert_ne!(here, later);
        assert_ne!(here, elsewhere);
    }

    #[test]
    fn test_id_derivation_error_messages_name_the_problem() {
        // Given
        let required = IdDerivationError::Required;
        let missing = IdDerivationError::MissingSource {
            attribute: "name".to_string(),
        };
        let illegal = IdDerivationError::Illegal(EntityIdError::Empty);

        // When / Then
        assert!(required.to_string().contains("`:id:`"));
        assert!(missing.to_string().contains("name"));
        assert_eq!(illegal.to_string(), "entity id is empty");
    }

    fn patterned(source: &str) -> IdSpec {
        IdSpec {
            pattern: Some(ValuePattern::new(source).unwrap()),
            ..IdSpec::default()
        }
    }

    #[test]
    fn test_check_pattern_accepts_an_id_matching_the_pattern() {
        // Given
        let spec = patterned("^REQ_[0-9]+$");
        let id = EntityId::new("REQ_001").unwrap();

        // When
        let result = spec.check_pattern(&id);

        // Then
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn test_check_pattern_reports_an_id_outside_the_pattern() {
        // Given
        let spec = patterned("^REQ_[0-9]+$");
        let id = EntityId::new("SPEC_001").unwrap();

        // When
        let result = spec.check_pattern(&id);

        // Then
        assert_eq!(
            result,
            Err(IdPatternMismatch {
                id: "SPEC_001".to_string(),
                pattern: "^REQ_[0-9]+$".to_string(),
            })
        );
    }

    #[test]
    fn test_check_pattern_accepts_any_id_when_no_pattern_is_declared() {
        // Given
        let spec = IdSpec::default();
        let id = EntityId::new("anything-at-all").unwrap();

        // When
        let result = spec.check_pattern(&id);

        // Then
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn test_id_pattern_mismatch_message_names_the_id_and_the_pattern() {
        // Given
        let mismatch = IdPatternMismatch {
            id: "SPEC_001".to_string(),
            pattern: "^REQ_".to_string(),
        };

        // When
        let message = mismatch.to_string();

        // Then
        assert_eq!(
            message,
            "id \"SPEC_001\" does not match the pattern `^REQ_`"
        );
    }
}

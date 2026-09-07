use std::fmt;

/// Which kind of declaration a name already belongs to.
///
/// Used by the clash errors so the message can say *what* the name collides
/// with, rather than only that it collides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeclarationKind {
    Attribute,
    Section,
    Relation,
}

impl fmt::Display for DeclarationKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Attribute => "an attribute",
            Self::Section => "a section",
            Self::Relation => "a relation",
        })
    }
}

/// A fault in a schema file.
///
/// Every one of these is a hard error that refuses the schema, never a
/// warning: the schema is the vocabulary the parser works from, so a build
/// that continued past one would report a cascade of confusing
/// unknown-directive and unknown-option diagnostics instead of the single real
/// problem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaError {
    /// Two entity types share a directive name.
    DuplicateType { name: String },
    /// Two roles share a spelling.
    DuplicateRole { name: String },
    /// A type declares the same attribute, section or relation twice.
    DuplicateDeclaration {
        type_name: String,
        kind: DeclarationKind,
        name: String,
    },
    /// An attribute and a relation on one type share an option spelling.
    OptionNameClash { type_name: String, name: String },
    /// A section name would shadow a directive the parser already knows.
    SectionShadowsDirective { type_name: String, name: String },
    /// A relation's `to`, a role's `types` or similar names a type that no
    /// `[[entity_type]]` declares.
    UnknownType { referenced_by: String, name: String },
    /// An `argument.fields` entry names an attribute the type does not declare.
    UnknownArgumentField { type_name: String, field: String },
    /// A whole-text argument was given more than one field to fill.
    WholeArgumentNeedsOneField { type_name: String, found: usize },
    /// An `id.from` entry names an attribute the type does not declare.
    UnknownIdSource {
        type_name: String,
        attribute: String,
    },
    /// `values` was given for a non-enum attribute, or omitted for an enum one.
    EnumValuesMismatch {
        type_name: String,
        attribute: String,
        value_type: String,
    },
    /// An attribute is both required and defaulted, which cannot both apply.
    RequiredAttributeHasDefault {
        type_name: String,
        attribute: String,
    },
    /// Two relations feed one back-link on a shared target but label it
    /// differently.
    BacklinkLabelConflict {
        target_type: String,
        incoming: String,
        first: String,
        second: String,
    },
    /// A derived back-link name collides with something the target type
    /// already declares.
    BacklinkNameClash {
        target_type: String,
        incoming: String,
        clashes_with: DeclarationKind,
    },
    /// The schema file was not valid TOML, or did not match the expected shape.
    Malformed { message: String },
}

impl fmt::Display for SchemaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateType { name } => {
                write!(f, "entity type `{name}` is declared more than once")
            }
            Self::DuplicateRole { name } => {
                write!(f, "role `{name}` is declared more than once")
            }
            Self::DuplicateDeclaration {
                type_name,
                kind,
                name,
            } => write!(
                f,
                "entity type `{type_name}` declares {kind} named `{name}` more than once"
            ),
            Self::OptionNameClash { type_name, name } => write!(
                f,
                "entity type `{type_name}` uses the option `{name}` for both an attribute and a relation"
            ),
            Self::SectionShadowsDirective { type_name, name } => write!(
                f,
                "entity type `{type_name}` declares a section named `{name}`, which would shadow the built-in `.. {name}::` directive"
            ),
            Self::UnknownType {
                referenced_by,
                name,
            } => write!(
                f,
                "{referenced_by} names the undeclared entity type `{name}`"
            ),
            Self::UnknownArgumentField { type_name, field } => write!(
                f,
                "entity type `{type_name}` maps its argument onto `{field}`, which it does not declare as an attribute"
            ),
            Self::WholeArgumentNeedsOneField { type_name, found } => write!(
                f,
                "entity type `{type_name}` maps its whole argument onto {found} fields; use `split = \"comma\"` to divide it, or declare a single field"
            ),
            Self::UnknownIdSource {
                type_name,
                attribute,
            } => write!(
                f,
                "entity type `{type_name}` derives its id from `{attribute}`, which it does not declare as an attribute"
            ),
            Self::EnumValuesMismatch {
                type_name,
                attribute,
                value_type,
            } => write!(
                f,
                "attribute `{attribute}` of entity type `{type_name}` has type `{value_type}`, which does not match the presence of `values`"
            ),
            Self::RequiredAttributeHasDefault {
                type_name,
                attribute,
            } => write!(
                f,
                "attribute `{attribute}` of entity type `{type_name}` is required and also has a default, so it can never be missing"
            ),
            Self::BacklinkLabelConflict {
                target_type,
                incoming,
                first,
                second,
            } => write!(
                f,
                "two relations feed the back-link `{incoming}` on entity type `{target_type}` but label it differently: {first:?} and {second:?}"
            ),
            Self::BacklinkNameClash {
                target_type,
                incoming,
                clashes_with,
            } => write!(
                f,
                "the back-link `{incoming}` derived on entity type `{target_type}` collides with {clashes_with} of the same name"
            ),
            Self::Malformed { message } => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for SchemaError {}

/// Every fault found while loading one schema.
///
/// Loading reports all of them rather than stopping at the first, because a
/// schema author fixing one typo at a time through repeated builds is the slow
/// path this whole feature is meant to avoid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaErrors(pub Vec<SchemaError>);

impl fmt::Display for SchemaErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, error) in self.0.iter().enumerate() {
            if index > 0 {
                writeln!(f)?;
            }
            write!(f, "{error}")?;
        }
        Ok(())
    }
}

impl std::error::Error for SchemaErrors {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_declaration_kind_names_itself_readably() {
        // Given / When / Then
        assert_eq!(DeclarationKind::Attribute.to_string(), "an attribute");
        assert_eq!(DeclarationKind::Section.to_string(), "a section");
        assert_eq!(DeclarationKind::Relation.to_string(), "a relation");
    }

    #[test]
    fn test_schema_error_messages_name_the_offending_declaration() {
        // Given
        let error = SchemaError::DuplicateDeclaration {
            type_name: "req".to_string(),
            kind: DeclarationKind::Section,
            name: "safety".to_string(),
        };

        // When
        let message = error.to_string();

        // Then
        assert_eq!(
            message,
            "entity type `req` declares a section named `safety` more than once"
        );
    }

    #[test]
    fn test_backlink_label_conflict_quotes_both_labels() {
        // Given
        let error = SchemaError::BacklinkLabelConflict {
            target_type: "spec".to_string(),
            incoming: "linked_by".to_string(),
            first: "Linked by".to_string(),
            second: "Verified by".to_string(),
        };

        // When
        let message = error.to_string();

        // Then
        assert!(message.contains("\"Linked by\""));
        assert!(message.contains("\"Verified by\""));
    }

    #[test]
    fn test_schema_errors_prints_one_fault_per_line() {
        // Given
        let errors = SchemaErrors(vec![
            SchemaError::DuplicateType {
                name: "req".to_string(),
            },
            SchemaError::DuplicateRole {
                name: "need".to_string(),
            },
        ]);

        // When
        let message = errors.to_string();

        // Then
        assert_eq!(message.lines().count(), 2);
    }

    #[test]
    fn test_schema_errors_prints_nothing_when_empty() {
        // Given
        let errors = SchemaErrors(Vec::new());

        // When
        let message = errors.to_string();

        // Then
        assert!(message.is_empty());
    }
}

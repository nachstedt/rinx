use schemars::JsonSchema;
use serde::Deserialize;

use crate::argument::{ArgumentSpec, ArgumentSplit};
use crate::attribute::{AttributeSchema, AttributeType};
use crate::entity_type::EntityType;
use crate::error::{DeclarationKind, SchemaError, SchemaErrors};
use crate::id::IdSpec;
use crate::relation::RelationSpec;
use crate::role::RoleSpec;
use crate::schema::EntitySchema;
use crate::section::SectionSpec;

/// Decides whether a name is already a directive the parser understands.
///
/// Injected rather than duplicated here: the parser owns the directive
/// vocabulary, and a copy of it in this crate would drift the moment a new
/// built-in directive is added — the exact silent degradation the check
/// exists to prevent. This is the same seam `ParseFileLoader` uses for
/// filesystem access.
pub trait ReservedDirectiveNames {
    /// Reports whether `name` is a directive the parser already dispatches.
    fn is_reserved(&self, name: &str) -> bool;
}

/// A [`ReservedDirectiveNames`] that reserves nothing.
///
/// For tests and for callers that genuinely have no parser to ask; a schema
/// loaded through it can declare a section that shadows `.. note::`.
pub struct NoReservedNames;

impl ReservedDirectiveNames for NoReservedNames {
    fn is_reserved(&self, _name: &str) -> bool {
        false
    }
}

impl<F: Fn(&str) -> bool> ReservedDirectiveNames for F {
    fn is_reserved(&self, name: &str) -> bool {
        self(name)
    }
}

/// Loads a schema from a TOML document.
///
/// Every fault is collected rather than stopping at the first, so a schema
/// author sees the whole list in one build.
///
/// # Errors
///
/// Returns [`SchemaErrors`] if the text is not valid TOML, does not match the
/// expected shape, or violates any declaration or back-link rule.
pub fn load_schema(
    text: &str,
    reserved: &dyn ReservedDirectiveNames,
) -> Result<EntitySchema, SchemaErrors> {
    let raw: RawSchema = toml::from_str(text).map_err(|error| {
        SchemaErrors(vec![SchemaError::Malformed {
            message: error.to_string(),
        }])
    })?;

    let mut errors = Vec::new();
    let declared_types: Vec<String> = raw.entity_type.iter().map(|t| t.name.clone()).collect();

    let types: Vec<EntityType> = raw
        .entity_type
        .into_iter()
        .map(|raw_type| convert_type(raw_type, &declared_types, reserved, &mut errors))
        .collect();
    let roles: Vec<RoleSpec> = raw
        .role
        .into_iter()
        .map(|raw_role| convert_role(raw_role, &declared_types, &mut errors))
        .collect();

    collect_duplicates(
        types.iter().map(|t| t.name.as_str()),
        |name| SchemaError::DuplicateType { name },
        &mut errors,
    );
    collect_duplicates(
        roles.iter().map(|r| r.name.as_str()),
        |name| SchemaError::DuplicateRole { name },
        &mut errors,
    );

    if !errors.is_empty() {
        return Err(SchemaErrors(errors));
    }
    EntitySchema::new(types, roles).map_err(SchemaErrors)
}

/// Reports each name that appears more than once in `names`.
fn collect_duplicates<'a>(
    names: impl Iterator<Item = &'a str>,
    make_error: impl Fn(String) -> SchemaError,
    errors: &mut Vec<SchemaError>,
) {
    let mut seen: Vec<&str> = Vec::new();
    let mut reported: Vec<&str> = Vec::new();
    for name in names {
        if seen.contains(&name) {
            if !reported.contains(&name) {
                errors.push(make_error(name.to_string()));
                reported.push(name);
            }
        } else {
            seen.push(name);
        }
    }
}

/// Converts one raw entity type, recording every fault it carries.
fn convert_type(
    raw: RawEntityType,
    declared_types: &[String],
    reserved: &dyn ReservedDirectiveNames,
    errors: &mut Vec<SchemaError>,
) -> EntityType {
    let type_name = raw.name.clone();
    let attributes: Vec<AttributeSchema> = raw
        .attribute
        .into_iter()
        .map(|attr| convert_attribute(attr, &type_name, errors))
        .collect();
    let sections: Vec<SectionSpec> = raw.section.into_iter().map(RawSection::into_spec).collect();
    let relations: Vec<RelationSpec> = raw
        .relation
        .into_iter()
        .map(RawRelation::into_spec)
        .collect();

    collect_duplicates(
        attributes.iter().map(|a| a.name.as_str()),
        |name| SchemaError::DuplicateDeclaration {
            type_name: type_name.clone(),
            kind: DeclarationKind::Attribute,
            name,
        },
        errors,
    );
    collect_duplicates(
        sections.iter().map(|s| s.name.as_str()),
        |name| SchemaError::DuplicateDeclaration {
            type_name: type_name.clone(),
            kind: DeclarationKind::Section,
            name,
        },
        errors,
    );
    collect_duplicates(
        relations.iter().map(|r| r.name.as_str()),
        |name| SchemaError::DuplicateDeclaration {
            type_name: type_name.clone(),
            kind: DeclarationKind::Relation,
            name,
        },
        errors,
    );

    for relation in &relations {
        if attributes.iter().any(|a| a.name == relation.name) {
            errors.push(SchemaError::OptionNameClash {
                type_name: type_name.clone(),
                name: relation.name.clone(),
            });
        }
        check_declared_types(
            relation.to.as_deref(),
            &format!("relation `{}` of entity type `{type_name}`", relation.name),
            declared_types,
            errors,
        );
    }

    for section in &sections {
        if reserved.is_reserved(&section.name) {
            errors.push(SchemaError::SectionShadowsDirective {
                type_name: type_name.clone(),
                name: section.name.clone(),
            });
        }
    }

    let argument = raw.argument.unwrap_or_default().into_spec();
    check_argument(&argument, &type_name, &attributes, errors);

    let id = raw.id.unwrap_or_default().into_spec();
    if let Some(sources) = &id.from {
        for source in sources {
            if !attributes.iter().any(|a| &a.name == source) {
                errors.push(SchemaError::UnknownIdSource {
                    type_name: type_name.clone(),
                    attribute: source.clone(),
                });
            }
        }
    }

    EntityType {
        name: raw.name,
        label: raw.label,
        argument,
        id,
        template: raw.template,
        attributes,
        sections,
        relations,
    }
}

/// Checks an argument mapping against the attributes it fills.
fn check_argument(
    argument: &ArgumentSpec,
    type_name: &str,
    attributes: &[AttributeSchema],
    errors: &mut Vec<SchemaError>,
) {
    if argument.split == ArgumentSplit::Whole && argument.fields.len() > 1 {
        errors.push(SchemaError::WholeArgumentNeedsOneField {
            type_name: type_name.to_string(),
            found: argument.fields.len(),
        });
    }
    for field in &argument.fields {
        if !attributes.iter().any(|a| &a.name == field) {
            errors.push(SchemaError::UnknownArgumentField {
                type_name: type_name.to_string(),
                field: field.clone(),
            });
        }
    }
}

/// Converts one raw attribute, resolving its declared type spelling.
fn convert_attribute(
    raw: RawAttribute,
    type_name: &str,
    errors: &mut Vec<SchemaError>,
) -> AttributeSchema {
    // A faulty declaration still yields an attribute, so the rest of the type
    // is checked in the same pass and the author sees every fault at once.
    let value_type = resolve_attribute_type(raw.value_type, raw.values).unwrap_or_else(|()| {
        errors.push(SchemaError::EnumValuesMismatch {
            type_name: type_name.to_string(),
            attribute: raw.name.clone(),
            value_type: raw.value_type.as_str().to_string(),
        });
        AttributeType::String
    });
    if raw.required && raw.default.is_some() {
        errors.push(SchemaError::RequiredAttributeHasDefault {
            type_name: type_name.to_string(),
            attribute: raw.name.clone(),
        });
    }
    AttributeSchema {
        name: raw.name,
        label: raw.label,
        value_type,
        required: raw.required,
        default: raw.default,
    }
}

/// Pairs a `type = "..."` spelling with its `values`, rejecting a mismatch.
///
/// `values` on a non-enum attribute, or an enum without them, is a mistake
/// worth reporting rather than quietly ignoring: both spellings look like they
/// constrain the value, and only one of them does.
fn resolve_attribute_type(
    spelling: RawAttributeType,
    values: Option<Vec<String>>,
) -> Result<AttributeType, ()> {
    match (spelling, values) {
        (RawAttributeType::String, None) => Ok(AttributeType::String),
        (RawAttributeType::Text, None) => Ok(AttributeType::Text),
        (RawAttributeType::Int, None) => Ok(AttributeType::Int),
        (RawAttributeType::Bool, None) => Ok(AttributeType::Bool),
        (RawAttributeType::StringList, None) => Ok(AttributeType::StringList),
        (RawAttributeType::Enum, Some(values)) => Ok(AttributeType::Enum { values }),
        (RawAttributeType::EnumList, Some(values)) => Ok(AttributeType::EnumList { values }),
        _ => Err(()),
    }
}

/// Converts one raw role and checks the types it names.
fn convert_role(
    raw: RawRole,
    declared_types: &[String],
    errors: &mut Vec<SchemaError>,
) -> RoleSpec {
    check_declared_types(
        raw.types.as_deref(),
        &format!("role `{}`", raw.name),
        declared_types,
        errors,
    );
    RoleSpec {
        name: raw.name,
        types: raw.types,
    }
}

/// Reports every named type that no `[[entity_type]]` declares.
fn check_declared_types(
    candidates: Option<&[String]>,
    referenced_by: &str,
    declared_types: &[String],
    errors: &mut Vec<SchemaError>,
) {
    let Some(names) = candidates else {
        return;
    };
    for name in names {
        if !declared_types.contains(name) {
            errors.push(SchemaError::UnknownType {
                referenced_by: referenced_by.to_string(),
                name: name.clone(),
            });
        }
    }
}

// The raw shapes below mirror the schema file exactly. `deny_unknown_fields`
// is deliberate throughout: a mistyped key in a schema is the kind of silent
// degradation that later shows up as a baffling unknown-option diagnostic on
// every entity, so it is refused at the source.

/// A project's entity meta-model, as the schema file spells it.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(title = "rusty-sphinx entity schema")]
pub(crate) struct RawSchema {
    /// The entity types this project declares, one `[[entity_type]]` each.
    #[serde(default)]
    entity_type: Vec<RawEntityType>,
    /// Inline roles that reference an entity from prose. Optional: every entity
    /// is reachable by `:ref:` and by the built-in `:entity:` role regardless.
    #[serde(default)]
    role: Vec<RawRole>,
}

/// One entity type: a directive name plus everything an instance may carry.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct RawEntityType {
    /// The directive name, e.g. `req` for `.. req::`.
    name: String,
    /// Human-readable label for the built-in rendering; defaults to `name`.
    label: Option<String>,
    /// A `MiniJinja` template's file name, supplied by the site's
    /// `entity_templates` attribute. Omit for the built-in rendering.
    template: Option<String>,
    /// How the text after `::` maps onto attributes. Omit for a type that
    /// takes no argument.
    argument: Option<RawArgument>,
    /// How instances of this type are identified.
    id: Option<RawId>,
    /// Typed values, never parsed as RST.
    #[serde(default)]
    attribute: Vec<RawAttribute>,
    /// Named prose sections, whose content *is* fully-parsed RST.
    #[serde(default)]
    section: Vec<RawSection>,
    /// Typed edges to other entities, declared on the type that carries them.
    #[serde(default)]
    relation: Vec<RawRelation>,
}

/// How `.. <type>:: <argument>` maps onto the type's attributes.
#[derive(Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
struct RawArgument {
    /// Attribute names, in the order the argument's parts fill them. Each must
    /// be an attribute the type declares.
    #[serde(default)]
    fields: Vec<String>,
    /// How the argument text is divided before being mapped.
    #[serde(default)]
    split: RawSplit,
}

impl RawArgument {
    fn into_spec(self) -> ArgumentSpec {
        ArgumentSpec {
            fields: self.fields,
            split: match self.split {
                RawSplit::Whole => ArgumentSplit::Whole,
                RawSplit::Comma => ArgumentSplit::Comma,
            },
        }
    }
}

/// How a directive's argument text is divided.
#[derive(Deserialize, JsonSchema, Default)]
#[serde(rename_all = "lowercase")]
enum RawSplit {
    /// The whole argument is one value — the sphinx-needs shape.
    #[default]
    Whole,
    /// A comma-separated signature — `CPython`'s `.. audit-event::` shape.
    Comma,
}

/// How an entity type's ids are determined.
#[derive(Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
struct RawId {
    /// Prepended to derived and generated ids, never to an explicit `:id:`.
    prefix: Option<String>,
    /// Whether the author must write an explicit `:id:`.
    #[serde(default)]
    required: bool,
    /// Attributes whose values compose the id, joined with `_`. Each must be an
    /// attribute the type declares.
    from: Option<Vec<String>>,
}

impl RawId {
    fn into_spec(self) -> IdSpec {
        IdSpec {
            prefix: self.prefix,
            required: self.required,
            from: self.from,
        }
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct RawAttribute {
    /// The option spelling, e.g. `status` for `:status:`.
    name: String,
    /// Human-readable label for the rendered field table.
    label: Option<String>,
    #[serde(rename = "type")]
    value_type: RawAttributeType,
    /// The permitted values. Required for the two enum types, and refused for
    /// every other — both spellings look like they constrain the value, and
    /// only one does.
    values: Option<Vec<String>>,
    /// Whether the option must be given.
    #[serde(default)]
    required: bool,
    /// The value used when the option is absent.
    default: Option<String>,
}

/// The `type = "..."` spellings an attribute may declare.
///
/// A typed enum rather than a free string, so an unknown spelling is refused
/// by deserialization — which both shortens the loader and lets the generated
/// JSON Schema offer the seven valid values as completions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
enum RawAttributeType {
    #[serde(rename = "string")]
    String,
    #[serde(rename = "text")]
    Text,
    #[serde(rename = "int")]
    Int,
    #[serde(rename = "bool")]
    Bool,
    #[serde(rename = "enum")]
    Enum,
    #[serde(rename = "list<string>")]
    StringList,
    #[serde(rename = "list<enum>")]
    EnumList,
}

impl RawAttributeType {
    /// The spelling as written in a schema file, for a diagnostic that quotes it.
    const fn as_str(self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Text => "text",
            Self::Int => "int",
            Self::Bool => "bool",
            Self::Enum => "enum",
            Self::StringList => "list<string>",
            Self::EnumList => "list<enum>",
        }
    }
}

/// A named prose section, written as a sub-directive inside an entity's body.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct RawSection {
    /// The sub-directive name. May not shadow a directive the build already has.
    name: String,
    /// Heading shown by the built-in rendering; defaults to `name`.
    label: Option<String>,
    /// Whether the sub-directive must appear at least once.
    #[serde(default)]
    required: bool,
    /// Whether it may appear more than once, kept in document order.
    #[serde(default)]
    multiple: bool,
}

impl RawSection {
    fn into_spec(self) -> SectionSpec {
        SectionSpec {
            name: self.name,
            label: self.label,
            required: self.required,
            multiple: self.multiple,
        }
    }
}

/// A typed edge to other entities, declared on the type that carries it.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct RawRelation {
    /// The option spelling on the source entity, e.g. `links` for `:links:`.
    name: String,
    /// Label shown by the built-in rendering; defaults to `name`.
    label: Option<String>,
    /// Entity types a target may have. Omit to accept any type.
    to: Option<Vec<String>>,
    /// Whether at least one target id must be named.
    #[serde(default)]
    required: bool,
    /// Whether more than one target id may be named.
    #[serde(default)]
    multiple: bool,
    /// The back-link name derived on each target. Omit for a one-directional
    /// relation.
    incoming: Option<String>,
    /// Heading for that back-link; defaults to `incoming`.
    incoming_label: Option<String>,
}

impl RawRelation {
    fn into_spec(self) -> RelationSpec {
        RelationSpec {
            name: self.name,
            label: self.label,
            to: self.to,
            required: self.required,
            multiple: self.multiple,
            incoming: self.incoming,
            incoming_label: self.incoming_label,
        }
    }
}

/// An inline role that references an entity from prose.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct RawRole {
    /// The role spelling, e.g. `req` for ``:req:`REQ_001` ``.
    name: String,
    /// Entity types the role may resolve to. Omit to accept any type.
    types: Option<Vec<String>>,
}

#[cfg(test)]
mod declaration_tests;
#[cfg(test)]
mod shape_tests;

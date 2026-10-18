use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::argument::{ArgumentSpec, ArgumentSplit};
use crate::attribute::{AttributeSchema, AttributeType};
use crate::entity_type::EntityType;
use crate::error::{DeclarationKind, SchemaError, SchemaErrors};
use crate::id::IdSpec;
use crate::pattern::ValuePattern;
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
    load_raw_schema(raw, reserved)
}

/// Loads a schema from its declarations as the file spells them.
///
/// The half of [`load_schema`] after the TOML is read, for a caller that
/// built the declarations itself — the sphinx-needs conversion does — so the
/// declarations it produces are checked by exactly the rules a written
/// `entities.toml` is.
///
/// # Errors
///
/// Returns [`SchemaErrors`] if the declarations violate any declaration or
/// back-link rule.
pub fn load_raw_schema(
    raw: RawSchema,
    reserved: &dyn ReservedDirectiveNames,
) -> Result<EntitySchema, SchemaErrors> {
    let mut errors = Vec::new();
    let import_keys = raw.import_keys;
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
    EntitySchema::new(types, roles)
        .map(|schema| schema.with_import_keys(import_keys))
        .map_err(SchemaErrors)
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

    let id = raw.id.unwrap_or_default().into_spec(&type_name, errors);
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
        AttributeType::string()
    });
    let value_type = match raw.pattern {
        Some(source) => attach_pattern(value_type, &source, type_name, &raw.name, errors),
        None => value_type,
    };
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

/// Compiles an attribute's `pattern` and attaches it to its type.
///
/// Refused on any type that is not text: a pattern on an `int` or an `enum`
/// looks like it constrains the value, and nothing would ever check it. A
/// faulty pattern still yields the type without one, so the rest of the
/// schema is checked in the same pass.
fn attach_pattern(
    value_type: AttributeType,
    source: &str,
    type_name: &str,
    attribute: &str,
    errors: &mut Vec<SchemaError>,
) -> AttributeType {
    let Some(pattern) = compile_pattern(
        source,
        type_name,
        &format!("attribute `{attribute}`"),
        errors,
    ) else {
        return value_type;
    };
    match value_type {
        AttributeType::String { .. } => AttributeType::String {
            pattern: Some(pattern),
        },
        AttributeType::Text { .. } => AttributeType::Text {
            pattern: Some(pattern),
        },
        AttributeType::StringList { .. } => AttributeType::StringList {
            pattern: Some(pattern),
        },
        other => {
            errors.push(SchemaError::PatternOnNonTextType {
                type_name: type_name.to_string(),
                attribute: attribute.to_string(),
                value_type: other.as_str().to_string(),
            });
            other
        }
    }
}

/// Compiles a schema-written pattern, recording why it could not be.
fn compile_pattern(
    source: &str,
    type_name: &str,
    declared_on: &str,
    errors: &mut Vec<SchemaError>,
) -> Option<ValuePattern> {
    ValuePattern::new(source)
        .map_err(|error| {
            errors.push(SchemaError::InvalidPattern {
                type_name: type_name.to_string(),
                declared_on: declared_on.to_string(),
                message: error.to_string(),
            });
        })
        .ok()
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
        (RawAttributeType::String, None) => Ok(AttributeType::string()),
        (RawAttributeType::Text, None) => Ok(AttributeType::text()),
        (RawAttributeType::Int, None) => Ok(AttributeType::Int),
        (RawAttributeType::Bool, None) => Ok(AttributeType::Bool),
        (RawAttributeType::StringList, None) => Ok(AttributeType::string_list()),
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
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(title = "rinx entity schema")]
pub struct RawSchema {
    /// Names a `.. needimport::` may write instead of a path, each mapped to
    /// the file it stands for — sphinx-needs' `needs_import_keys`.
    ///
    /// A path, unlike the `template` an entity type names: the alias is
    /// written in a *document*, so its meaning belongs with the project's
    /// vocabulary rather than in the build file. The file it names must still
    /// be declared in the library's `parse_data`, since only the build system
    /// can put it in the parse action's sandbox.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    #[schemars(extend("default" = {}))]
    pub(crate) import_keys: std::collections::BTreeMap<String, String>,
    /// The entity types this project declares, one `[[entity_type]]` each.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schemars(extend("default" = []))]
    pub(crate) entity_type: Vec<RawEntityType>,
    /// Inline roles that reference an entity from prose. Optional: every entity
    /// is reachable by `:ref:` and by the built-in `:entity:` role regardless.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schemars(extend("default" = []))]
    pub(crate) role: Vec<RawRole>,
}

/// One entity type: a directive name plus everything an instance may carry.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawEntityType {
    /// The directive name, e.g. `req` for `.. req::`.
    pub(crate) name: String,
    /// Human-readable label for the built-in rendering; defaults to `name`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) label: Option<String>,
    /// A `MiniJinja` template's file name, supplied by the site's
    /// `entity_templates` attribute. Omit for the built-in rendering.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) template: Option<String>,
    /// How the text after `::` maps onto attributes. Omit for a type that
    /// takes no argument.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) argument: Option<RawArgument>,
    /// How instances of this type are identified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) id: Option<RawId>,
    /// Typed values, never parsed as RST.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schemars(extend("default" = []))]
    pub(crate) attribute: Vec<RawAttribute>,
    /// Named prose sections, whose content *is* fully-parsed RST.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schemars(extend("default" = []))]
    pub(crate) section: Vec<RawSection>,
    /// Typed edges to other entities, declared on the type that carries them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schemars(extend("default" = []))]
    pub(crate) relation: Vec<RawRelation>,
}

/// How `.. <type>:: <argument>` maps onto the type's attributes.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawArgument {
    /// Attribute names, in the order the argument's parts fill them. Each must
    /// be an attribute the type declares.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schemars(extend("default" = []))]
    pub(crate) fields: Vec<String>,
    /// How the argument text is divided before being mapped.
    #[serde(default, skip_serializing_if = "RawSplit::is_whole")]
    #[schemars(extend("default" = "whole"))]
    pub(crate) split: RawSplit,
}

impl RawSplit {
    /// Whether this is the default split, which a written schema leaves out.
    fn is_whole(&self) -> bool {
        matches!(self, Self::Whole)
    }
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
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema, Default)]
#[serde(rename_all = "lowercase")]
pub(crate) enum RawSplit {
    /// The whole argument is one value — the sphinx-needs shape.
    #[default]
    Whole,
    /// A comma-separated signature — `CPython`'s `.. audit-event::` shape.
    Comma,
}

/// How an entity type's ids are determined.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawId {
    /// Prepended to derived and generated ids, never to an explicit `:id:`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) prefix: Option<String>,
    /// Whether the author must write an explicit `:id:`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[schemars(extend("default" = false))]
    pub(crate) required: bool,
    /// Attributes whose values compose the id, joined with `_`. Each must be an
    /// attribute the type declares.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) from: Option<Vec<String>>,
    /// A regular expression every id of this type should match — explicit,
    /// composed or generated, prefix included. Searched for, as JSON Schema's
    /// `pattern` is: write `^…$` to match the whole id. A mismatch is reported
    /// as `entity.id-pattern-mismatch`, and the entity keeps its id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) pattern: Option<String>,
}

impl RawId {
    fn into_spec(self, type_name: &str, errors: &mut Vec<SchemaError>) -> IdSpec {
        let pattern = self
            .pattern
            .and_then(|source| compile_pattern(&source, type_name, "the id", errors));
        IdSpec {
            prefix: self.prefix,
            required: self.required,
            from: self.from,
            pattern,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawAttribute {
    /// The option spelling, e.g. `status` for `:status:`.
    pub(crate) name: String,
    /// Human-readable label for the rendered field table.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) label: Option<String>,
    #[serde(rename = "type")]
    pub(crate) value_type: RawAttributeType,
    /// The permitted values. Required for the two enum types, and refused for
    /// every other — both spellings look like they constrain the value, and
    /// only one does.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) values: Option<Vec<String>>,
    /// A regular expression the value must match; for `list<string>`, each
    /// item must. Searched for, as JSON Schema's `pattern` is: write `^…$` to
    /// match the whole value. Only the three text types take one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) pattern: Option<String>,
    /// Whether the option must be given.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[schemars(extend("default" = false))]
    pub(crate) required: bool,
    /// The value used when the option is absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) default: Option<String>,
}

/// The `type = "..."` spellings an attribute may declare.
///
/// A typed enum rather than a free string, so an unknown spelling is refused
/// by deserialization — which both shortens the loader and lets the generated
/// JSON Schema offer the seven valid values as completions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
pub(crate) enum RawAttributeType {
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
    pub(crate) const fn as_str(self) -> &'static str {
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
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawSection {
    /// The sub-directive name. May not shadow a directive the build already has.
    pub(crate) name: String,
    /// Heading shown by the built-in rendering; defaults to `name`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) label: Option<String>,
    /// Whether the sub-directive must appear at least once.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[schemars(extend("default" = false))]
    pub(crate) required: bool,
    /// Whether it may appear more than once, kept in document order.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[schemars(extend("default" = false))]
    pub(crate) multiple: bool,
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
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawRelation {
    /// The option spelling on the source entity, e.g. `links` for `:links:`.
    pub(crate) name: String,
    /// Label shown by the built-in rendering; defaults to `name`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) label: Option<String>,
    /// Entity types a target may have. Omit to accept any type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) to: Option<Vec<String>>,
    /// Whether at least one target id must be named.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[schemars(extend("default" = false))]
    pub(crate) required: bool,
    /// Whether more than one target id may be named.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[schemars(extend("default" = false))]
    pub(crate) multiple: bool,
    /// The back-link name derived on each target. Omit for a one-directional
    /// relation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) incoming: Option<String>,
    /// Heading for that back-link; defaults to `incoming`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) incoming_label: Option<String>,
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
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawRole {
    /// The role spelling, e.g. `req` for ``:req:`REQ_001` ``.
    pub(crate) name: String,
    /// Entity types the role may resolve to. Omit to accept any type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) types: Option<Vec<String>>,
}

#[cfg(test)]
mod declaration_tests;
#[cfg(test)]
mod shape_tests;

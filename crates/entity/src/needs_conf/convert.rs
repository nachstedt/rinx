//! Turning a sphinx-needs configuration into entity schema declarations.
//!
//! sphinx-needs scopes neither fields nor links to a need type, so every
//! entity type gets every one of them; `schemas.json` is the only thing that
//! can narrow a name down to one type — `required`, a pattern, or an enum on
//! it alone. What comes out is the loader's own grammar, [`RawSchema`], so a
//! converted schema is checked by exactly the rules a written one is.

use std::collections::{BTreeMap, BTreeSet};

use crate::load::{
    RawArgument, RawAttribute, RawAttributeType, RawEntityType, RawId, RawRelation, RawRole,
    RawSchema, RawSplit,
};

use super::config::{NeedField, NeedLink, NeedType, NeedsConfig};
use super::ordered_table::OrderedTable;
use super::report::{ReportCategory, ReportEntry};
use super::schemas_json::{SchemasFile, TypeConstraints, constraints_by_type, is_set, python_str};

/// sphinx-needs' own built-in need options, with their value types.
///
/// A project declares only its *extra* fields, so without these every
/// `:tags:` or `:collapse:` would be reported as an unknown attribute — noise
/// about sphinx-needs' vocabulary rather than about the project's. A
/// project's own declaration of the same name wins over the entry here.
pub(crate) const NEEDS_BUILTIN_OPTIONS: &[(&str, RawAttributeType)] = &[
    ("status", RawAttributeType::String),
    ("tags", RawAttributeType::StringList),
    ("collapse", RawAttributeType::Bool),
    ("hide", RawAttributeType::Bool),
    ("delete", RawAttributeType::Bool),
    ("duration", RawAttributeType::String),
    ("completion", RawAttributeType::String),
    ("constraints", RawAttributeType::StringList),
    ("layout", RawAttributeType::String),
    ("style", RawAttributeType::String),
    ("template", RawAttributeType::String),
    ("pre_template", RawAttributeType::String),
    ("post_template", RawAttributeType::String),
];

/// Options sphinx-needs treats specially and the entity model does not
/// express as an attribute at all: `id` *is* the entity id, `title` is filled
/// from the directive argument, and `type` is the directive.
const RESERVED_FIELDS: &[&str] = &["id", "title", "type"];

/// sphinx-needs derives a link's back-link option as `<name>_back`; the
/// relation declares that spelling explicitly, so a document writing
/// `:links_back:` resolves rather than warning.
const BACKLINK_SUFFIX: &str = "_back";

/// sphinx-needs' role pointing at a need of any type.
const NEED_ROLE: &str = "need";

/// The entity schema declarations `config` (and `schemas`) describe, and what
/// could not be carried over, in the order it was found.
#[must_use]
pub fn convert_needs(
    config: &NeedsConfig,
    schemas: Option<&SchemasFile>,
) -> (RawSchema, Vec<ReportEntry>) {
    let mut report = Vec::new();
    let fields = global_fields(config);
    let relations = relations_from_links(&config.all_links(), &mut report);
    let constraints = schemas.map_or_else(BTreeMap::new, |schemas| {
        constraints_by_type(schemas, &mut report)
    });
    let vocabulary = Vocabulary {
        fields,
        relations,
        constraints,
        id_required: config.id_required,
    };
    let types: Vec<RawEntityType> = config
        .types
        .iter()
        .map(|need_type| entity_type(need_type, &vocabulary, &mut report))
        .collect();
    report.extend(unsupported_constructs(config));
    let names: Vec<String> = types.iter().map(|entity| entity.name.clone()).collect();
    let schema = RawSchema {
        import_keys: config.import_keys.clone(),
        entity_type: types,
        role: roles(&names),
    };
    (schema, report)
}

/// What a sphinx-needs project declares once for all of its need types.
struct Vocabulary {
    fields: OrderedTable<RawAttribute>,
    relations: Vec<RawRelation>,
    constraints: BTreeMap<String, TypeConstraints>,
    /// `needs_id_required`: every need must be given an explicit id.
    id_required: bool,
}

/// One entity type from a `needs_types` entry.
fn entity_type(
    need_type: &NeedType,
    vocabulary: &Vocabulary,
    report: &mut Vec<ReportEntry>,
) -> RawEntityType {
    let name = &need_type.directive;
    let here = vocabulary
        .constraints
        .get(name)
        .cloned()
        .unwrap_or_default();

    let mut attributes = vec![attribute("title", "Title", RawAttributeType::String)];
    for (_, declared) in vocabulary.fields.iter() {
        let mut attribute = declared.clone();
        attribute.required |= here.required.contains(&attribute.name);
        attributes.push(attribute);
    }

    let declared: BTreeSet<String> = attributes.iter().map(|a| a.name.clone()).collect();
    let mut relations = Vec::new();
    for declared_relation in &vocabulary.relations {
        // A name cannot be an attribute and a relation on the same type; the
        // relation wins, since a link carries a reference rather than a value.
        if declared.contains(&declared_relation.name) {
            attributes.retain(|a| a.name != declared_relation.name);
        }
        let mut relation = declared_relation.clone();
        relation.required |= here.required.contains(&relation.name);
        relations.push(relation);
    }

    let attributes: Vec<RawAttribute> = attributes
        .into_iter()
        .map(|attribute| narrow_attribute(attribute, &here, report))
        .collect();
    report_unplaced_value_rules(name, &here, &attributes, &relations, report);

    RawEntityType {
        name: name.clone(),
        label: Some(need_type.title.clone().unwrap_or_else(|| name.clone())),
        template: None,
        argument: Some(RawArgument {
            fields: vec!["title".to_string()],
            split: RawSplit::Whole,
        }),
        id: id_spec_for(need_type, vocabulary.id_required, &here),
        attribute: attributes,
        section: Vec::new(),
        relation: relations,
    }
}

/// An attribute declaration with nothing but its name, label and type.
fn attribute(name: &str, label: &str, value_type: RawAttributeType) -> RawAttribute {
    RawAttribute {
        name: name.to_string(),
        label: Some(label.to_string()),
        value_type,
        values: None,
        pattern: None,
        required: false,
        default: None,
    }
}

/// Reports each value rule on type `name` that names no attribute of it.
fn report_unplaced_value_rules(
    name: &str,
    constraints: &TypeConstraints,
    attributes: &[RawAttribute],
    relations: &[RawRelation],
    report: &mut Vec<ReportEntry>,
) {
    for (field_name, rule_id) in &constraints.sources {
        if attributes.iter().any(|a| &a.name == field_name) {
            continue;
        }
        let detail = if relations.iter().any(|r| &r.name == field_name) {
            format!(
                "{rule_id}: constrains the value of link `{field_name}`, whose values are entity ids rather than text"
            )
        } else {
            format!("{rule_id}: constrains `{field_name}`, which `{name}` does not declare")
        };
        report.push(ReportEntry::new(ReportCategory::ValueSchemaRule, detail));
    }
}

/// The `id` table of one type — its prefix, whether it is required, its
/// pattern — or `None` when it sets none of them.
fn id_spec_for(
    need_type: &NeedType,
    id_required: bool,
    constraints: &TypeConstraints,
) -> Option<RawId> {
    let id = RawId {
        prefix: need_type.prefix.clone().filter(|prefix| !prefix.is_empty()),
        required: id_required,
        from: None,
        pattern: constraints.id_pattern.clone(),
    };
    (id != RawId::default()).then_some(id)
}

/// Applies one type's value narrowings from `schemas.json` to an attribute.
///
/// A pattern goes only onto a `string`, the one type both models read as a
/// single piece of text; an enum turns a `string` into an `enum`, or narrows
/// an existing one. Anything else is reported rather than carried over
/// half-honoured.
fn narrow_attribute(
    mut attribute: RawAttribute,
    constraints: &TypeConstraints,
    report: &mut Vec<ReportEntry>,
) -> RawAttribute {
    let name = attribute.name.clone();
    let pattern = constraints.patterns.get(&name);
    let values = constraints.enums.get(&name);
    let Some(rule_id) = constraints.sources.get(&name) else {
        return attribute;
    };
    if let Some(pattern) = pattern {
        if attribute.value_type == RawAttributeType::String {
            attribute.pattern = Some(pattern.clone());
        } else {
            report.push(ReportEntry::new(
                ReportCategory::ValueSchemaRule,
                format!(
                    "{rule_id}: a pattern on `{name}`, whose type is `{}` rather than text",
                    attribute.value_type.as_str()
                ),
            ));
        }
    }
    if let Some(values) = values {
        if matches!(
            attribute.value_type,
            RawAttributeType::String | RawAttributeType::Enum
        ) {
            attribute.value_type = RawAttributeType::Enum;
            attribute.values = Some(values.clone());
        } else {
            report.push(ReportEntry::new(
                ReportCategory::ValueSchemaRule,
                format!(
                    "{rule_id}: an enum on `{name}`, whose type is `{}`",
                    attribute.value_type.as_str()
                ),
            ));
        }
    }
    attribute
}

/// The attribute declaration for every option a need may carry: the
/// sphinx-needs built-ins, overridden by the project's own fields.
fn global_fields(config: &NeedsConfig) -> OrderedTable<RawAttribute> {
    let mut fields: OrderedTable<RawAttribute> = NEEDS_BUILTIN_OPTIONS
        .iter()
        .map(|(name, value_type)| {
            (
                (*name).to_string(),
                attribute(name, &label_for(name), *value_type),
            )
        })
        .collect();
    for (name, field) in config.all_fields().iter() {
        if RESERVED_FIELDS.contains(&name) {
            continue;
        }
        fields.insert(name.to_string(), attribute_from_field(name, field));
    }
    if !config.statuses.is_empty() && fields.contains_key("status") {
        let mut status = attribute("status", "Status", RawAttributeType::Enum);
        status.values = Some(config.statuses.iter().map(|s| s.name.clone()).collect());
        fields.insert("status".to_string(), status);
    }
    fields
}

/// The attribute declaration one `needs_fields` entry describes.
///
/// The value type comes from the entry's JSON-Schema fragment, the only place
/// sphinx-needs states one; a field without a `schema` is an untyped string.
fn attribute_from_field(name: &str, field: &NeedField) -> RawAttribute {
    let schema = field.schema.clone().unwrap_or_default();
    let label = field
        .description
        .clone()
        .filter(|description| !description.is_empty())
        .unwrap_or_else(|| label_for(name));
    let is_array = schema.json_type.as_deref() == Some("array");
    if let Some(values) = schema.values {
        let value_type = if is_array {
            RawAttributeType::EnumList
        } else {
            RawAttributeType::Enum
        };
        let mut declared = attribute(name, &label, value_type);
        // An enum's values are strings in the entity model even when
        // sphinx-needs writes them as numbers (`effort = [1, 2, 3, 5, ...]`).
        declared.values = Some(values.iter().map(python_str).collect());
        return declared;
    }
    let value_type = match schema.json_type.as_deref() {
        Some("integer" | "number") => RawAttributeType::Int,
        Some("boolean") => RawAttributeType::Bool,
        Some("array") => RawAttributeType::StringList,
        _ => RawAttributeType::String,
    };
    attribute(name, &label, value_type)
}

/// One relation per link option, reporting what about a link is lost.
///
/// The key is the option spelling; `outgoing`/`incoming` are *labels*, and
/// the incoming option's own spelling is sphinx-needs' derived `<name>_back`.
/// `to` is left off deliberately — sphinx-needs links are not scoped to a
/// target type, and omitting it accepts any.
fn relations_from_links(
    links: &OrderedTable<NeedLink>,
    report: &mut Vec<ReportEntry>,
) -> Vec<RawRelation> {
    let mut relations = Vec::new();
    for (name, link) in links.iter() {
        let incoming = format!("{name}{BACKLINK_SUFFIX}");
        relations.push(RawRelation {
            name: name.to_string(),
            label: Some(link.outgoing.clone().unwrap_or_else(|| name.to_string())),
            to: None,
            required: false,
            multiple: true,
            incoming_label: Some(link.incoming.clone().unwrap_or_else(|| incoming.clone())),
            incoming: Some(incoming),
        });
        if link.predicates.as_ref().is_some_and(is_set) {
            report.push(ReportEntry::new(
                ReportCategory::LinkPredicates,
                format!(
                    "[needs.links.{name}] computes targets with a dynamic function; the link is declared, but nothing fills it in"
                ),
            ));
        }
        if link.copy {
            report.push(ReportEntry::new(
                ReportCategory::LinkCopy,
                format!("[needs.links.{name}] sets copy = true"),
            ));
        }
    }
    relations
}

/// One role per entity type, plus sphinx-needs' own `:need:` over all of
/// them unless a type is called that.
///
/// Roles are sugar — every entity is a `:ref:` target regardless — so the
/// point of declaring them is the type check and the spelling a migrating
/// project already writes.
fn roles(names: &[String]) -> Vec<RawRole> {
    let mut declared: Vec<RawRole> = names
        .iter()
        .map(|name| RawRole {
            name: name.clone(),
            types: Some(vec![name.clone()]),
        })
        .collect();
    if !names.iter().any(|name| name == NEED_ROLE) {
        declared.push(RawRole {
            name: NEED_ROLE.to_string(),
            types: Some(names.to_vec()),
        });
    }
    declared
}

/// Everything in the configuration the entity model cannot express.
fn unsupported_constructs(config: &NeedsConfig) -> Vec<ReportEntry> {
    let mut report = Vec::new();
    if let Some(constraints) = config.constraints.as_ref().filter(|value| is_set(value)) {
        let count = constraints
            .as_object()
            .map(serde_json::Map::len)
            .or_else(|| constraints.as_array().map(Vec::len))
            .unwrap_or(1);
        report.push(ReportEntry::new(
            ReportCategory::Constraints,
            format!(
                "{count} [needs.constraints] rule(s): value constraints evaluated at build time"
            ),
        ));
    }
    if config.variants.as_ref().is_some_and(is_set)
        || config.variant_data_file.as_ref().is_some_and(is_set)
    {
        report.push(ReportEntry::new(
            ReportCategory::Variants,
            "needs variants select option values per build configuration",
        ));
    }
    for (name, field) in config.all_fields().iter() {
        if field.parse_variants {
            report.push(ReportEntry::new(
                ReportCategory::Variants,
                format!("[needs.fields.{name}] parses variant syntax"),
            ));
        }
    }
    if config.global_options.as_ref().is_some_and(is_set) {
        report.push(ReportEntry::new(
            ReportCategory::GlobalOptions,
            "[needs.global_options] assigns defaults by filter",
        ));
    }
    // sphinx-needs' built-in `.. need::` selects its type in a `:type:`
    // option. A project that declares `need` as a type of its own is using
    // the spelling, not the mechanism, so reporting it there would be noise.
    if !config.types.iter().any(|t| t.directive == NEED_ROLE) {
        report.push(ReportEntry::new(
            ReportCategory::GenericNeedDirective,
            "sphinx-needs' `.. need::` names its type in a `:type:` option; the entity model has one directive per type",
        ));
    }
    report
}

/// A human-readable heading for an option that declares no description, as
/// Python's `str.capitalize` writes one.
fn label_for(name: &str) -> String {
    let spaced = name.replace('_', " ");
    let mut chars = spaced.chars();
    chars.next().map_or_else(String::new, |first| {
        first
            .to_uppercase()
            .chain(chars.flat_map(char::to_lowercase))
            .collect()
    })
}

#[cfg(test)]
mod entity_type_tests;
#[cfg(test)]
mod field_tests;
#[cfg(test)]
mod link_tests;
#[cfg(test)]
mod narrowing_tests;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod unsupported_tests;

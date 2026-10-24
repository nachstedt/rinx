//! The completion items a directive name, option name or option value is
//! offered, from the parser's directive specs.
//!
//! The server keeps no list of directives of its own: the names, options and
//! values are the specs' (`rinx_parser::directive_specs`,
//! `domain_directive_specs`, an [`EntitySpec`] per type of the schema), the
//! very records the parser reads a block by — so completion cannot offer a
//! directive or option the parser does not take.
//!
//! `.. entity-update::` is the one directive whose options depend on the
//! project: besides `:strict:`, it takes the fields of the entities its
//! argument matches, resolved against the project index as the build
//! resolves them (`rinx_analyzer::matched_entities`). A relation's value is
//! the other: its ids are the project's entities, which `entity_id_items`
//! lists.

use std::ops::Range;

use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionItemTag, CompletionTextEdit, Documentation,
    InsertTextFormat, InsertTextMode, TextEdit,
};
use rinx_ast::{Domain, EntityId, EntityUpdateSource, UpdateTarget};
use rinx_entity::EntitySchema;
use rinx_index::ProjectIndex;
use rinx_parser::{
    DirectiveSpec, EntitySpec, OptionSpec, OptionSupport, OptionValue, directive_spec,
    directive_specs, domain_directive_specs,
};

use super::directive_context::{
    DirectiveContext, NameContext, OptionContext, ValueContext, WrittenDirective, list_item_at,
};
use super::entity_id_items::{TargetTypes, entity_id_items};

/// What a document's directives are resolved against: its project's default
/// domain, entity schema and index.
pub struct DirectiveVocabulary<'a> {
    /// The domain of an unqualified directive name.
    pub domain: Domain,
    /// The entity types, each a directive.
    pub schema: &'a EntitySchema,
    /// The entities an `.. entity-update::` may match.
    pub index: &'a ProjectIndex,
}

/// How a client takes a completion's text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextFormat {
    /// As it is written.
    Plain,
    /// As a snippet, with tab stops: a client announcing
    /// `textDocument.completion.completionItem.snippetSupport`.
    Snippet,
}

/// The items completing `context`, where `position` turns a character index
/// on the line into the protocol's position, and `format` says how the
/// client takes their text — and whether the list was cut, so the editor
/// should ask again as the text is typed.
#[must_use]
pub fn directive_items(
    context: &DirectiveContext,
    vocabulary: &DirectiveVocabulary<'_>,
    position: &dyn Fn(usize) -> lsp_types::Position,
    format: TextFormat,
) -> (Vec<CompletionItem>, bool) {
    let range = |characters: &Range<usize>| {
        lsp_types::Range::new(position(characters.start), position(characters.end))
    };
    let entity_specs: Vec<EntitySpec<'_>> = vocabulary
        .schema
        .types()
        .iter()
        .map(EntitySpec::new)
        .collect();
    match context {
        DirectiveContext::Name(name) => (
            name_items(
                name,
                vocabulary.domain,
                &entity_specs,
                range(&name.replace),
                format,
            ),
            false,
        ),
        DirectiveContext::Option(option) => {
            let Some(spec) = resolve(&option.directive, vocabulary, &entity_specs) else {
                return (Vec::new(), false);
            };
            let items = option_items(
                option,
                &spec,
                &update_fields(&option.directive, &spec, vocabulary, &entity_specs),
                range(&option.replace),
            );
            (items, false)
        }
        DirectiveContext::Value(value) => {
            with_value_option(value, vocabulary, |option, accepted| match option.value {
                OptionValue::EntityIds(_) => {
                    entity_id_items(value, accepted, vocabulary.index, vocabulary.schema, &range)
                }
                other => (value_items(value, other, &range), false),
            })
            .unwrap_or_default()
        }
    }
}

/// The entity id the cursor is on in `context`'s value when the option is a
/// relation, and the characters it covers on the line — `None` for any
/// other option, or an item that is no id.
#[must_use]
pub fn relation_target_at(
    context: &ValueContext,
    vocabulary: &DirectiveVocabulary<'_>,
) -> Option<(Range<usize>, EntityId)> {
    with_value_option(context, vocabulary, |option, _| {
        matches!(option.value, OptionValue::EntityIds(_)).then(|| list_item_at(context).0)
    })
    .flatten()
    .and_then(|characters| {
        let item: String = context
            .value
            .chars()
            .skip(characters.start - context.value_start)
            .take(characters.len())
            .collect();
        Some((characters, EntityId::new(&item).ok()?))
    })
}

/// `answer` asked of the option whose value `context` is on, and of the
/// types its targets may have when it is a relation (see [`TargetTypes`]) —
/// or `None` when the directive or the option is unknown.
fn with_value_option<R>(
    context: &ValueContext,
    vocabulary: &DirectiveVocabulary<'_>,
    answer: impl FnOnce(&OptionSpec<'_>, &TargetTypes<'_>) -> R,
) -> Option<R> {
    let entity_specs: Vec<EntitySpec<'_>> = vocabulary
        .schema
        .types()
        .iter()
        .map(EntitySpec::new)
        .collect();
    let spec = resolve(&context.directive, vocabulary, &entity_specs)?;
    if let Some(option) = spec
        .option_specs()
        .find(|option| option.is_spelled(&context.option))
    {
        let accepted = match option.value {
            OptionValue::EntityIds(types) => vec![types],
            _ => Vec::new(),
        };
        return Some(answer(option, &accepted));
    }
    let fields = update_fields(&context.directive, &spec, vocabulary, &entity_specs);
    let field = fields
        .iter()
        .find(|field| field.option.is_spelled(&context.option))?;
    Some(answer(field.option, &field.target_types))
}

/// The spec of the directive written as `directive`: a built-in's, a domain
/// object's or scope directive's, or an entity type's.
fn resolve<'s>(
    directive: &WrittenDirective,
    vocabulary: &DirectiveVocabulary<'_>,
    entity_specs: &'s [EntitySpec<'s>],
) -> Option<DirectiveSpec<'s>> {
    directive_spec(&directive.name, vocabulary.domain)
        .copied()
        .or_else(|| {
            entity_specs
                .iter()
                .map(EntitySpec::spec)
                .find(|spec| spec.names.contains(&directive.name.as_str()))
        })
}

/// One item per name a directive is written as where `domain` is the
/// default domain: every directive of the table, every domain directive by
/// its bare name where that resolves to it and by its qualified one, and
/// every entity type.
fn name_items(
    context: &NameContext,
    domain: Domain,
    entity_specs: &[EntitySpec<'_>],
    range: lsp_types::Range,
    format: TextFormat,
) -> Vec<CompletionItem> {
    let mut named: Vec<(String, DirectiveSpec<'_>)> = Vec::new();
    for spec in directive_specs() {
        named.extend(spec.names.iter().map(|name| (name.to_string(), *spec)));
    }
    for (spec_domain, spec) in domain_directive_specs() {
        for name in spec.names {
            if spec_domain.is_none_or(|spec_domain| spec_domain == domain) {
                named.push((name.to_string(), *spec));
            }
            if let Some(spec_domain) = spec_domain {
                named.push((format!("{}:{name}", spec_domain.as_str()), *spec));
            }
        }
    }
    for entity_spec in entity_specs {
        let spec = entity_spec.spec();
        named.extend(spec.names.iter().map(|name| (name.to_string(), spec)));
    }
    named
        .into_iter()
        .map(|(name, spec)| {
            let (new_text, written_as) = name_insertion(&name, &spec, context, format);
            CompletionItem {
                label: name,
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some(spec.doc.to_string()),
                insert_text_format: Some(written_as),
                insert_text_mode: (written_as == InsertTextFormat::SNIPPET)
                    .then_some(InsertTextMode::ADJUST_INDENTATION),
                text_edit: Some(CompletionTextEdit::Edit(TextEdit { range, new_text })),
                ..CompletionItem::default()
            }
        })
        .collect()
}

/// What completing the name `name` of `spec` writes: the name, then the
/// `::` unless it is written already, and — as a snippet — a tab stop for
/// the argument and the content's first line, relative to the marker.
fn name_insertion(
    name: &str,
    spec: &DirectiveSpec<'_>,
    context: &NameContext,
    format: TextFormat,
) -> (String, InsertTextFormat) {
    let space = if context.needs_space { " " } else { "" };
    if context.closed {
        return (format!("{space}{name}"), InsertTextFormat::PLAIN_TEXT);
    }
    let takes_argument = spec.arguments.takes_any();
    if format == TextFormat::Plain {
        let after = if takes_argument { " " } else { "" };
        return (
            format!("{space}{name}::{after}"),
            InsertTextFormat::PLAIN_TEXT,
        );
    }
    let argument = if takes_argument { " $1" } else { "" };
    let body = if spec.takes_content {
        "\n\n   $0"
    } else {
        "$0"
    };
    (
        format!("{space}{name}::{argument}{body}"),
        InsertTextFormat::SNIPPET,
    )
}

/// A field an `.. entity-update::` may change, how many of the matched
/// types declare it, and — for a relation — the types its targets may have
/// under each of them.
struct UpdateField<'s> {
    option: &'s OptionSpec<'s>,
    declared_by: usize,
    target_types: Vec<Option<&'s [String]>>,
}

/// The fields `directive` takes beyond its listed options when it is an
/// `.. entity-update::`: those of the types its argument matches — every
/// type's, when it matches none — each once, the ones every matched type
/// declares first.
fn update_fields<'s>(
    directive: &WrittenDirective,
    spec: &DirectiveSpec<'_>,
    vocabulary: &DirectiveVocabulary<'_>,
    entity_specs: &'s [EntitySpec<'s>],
) -> Vec<UpdateField<'s>> {
    let updates = [
        EntityUpdateSource::EntityUpdate,
        EntityUpdateSource::NeedExtend,
    ];
    if !spec.accepts_further
        || !updates
            .iter()
            .any(|source| spec.names.contains(&source.as_str()))
    {
        return Vec::new();
    }
    let matched_types: Vec<&str> = UpdateTarget::read(&directive.argument)
        .map(|target| {
            rinx_analyzer::matched_entities(&target, vocabulary.index, vocabulary.schema)
                .iter()
                .filter_map(|id| vocabulary.index.entities.get(id))
                .map(|record| record.type_name.as_str())
                .collect()
        })
        .unwrap_or_default();
    let types: Vec<&EntitySpec<'s>> = entity_specs
        .iter()
        .filter(|entity_spec| {
            matched_types.is_empty()
                || entity_spec
                    .spec()
                    .names
                    .iter()
                    .any(|name| matched_types.contains(name))
        })
        .collect();
    let mut fields: Vec<UpdateField<'s>> = Vec::new();
    for entity_spec in &types {
        for option in entity_spec.updatable_fields() {
            let target_types = match option.value {
                OptionValue::EntityIds(types) => Some(types),
                _ => None,
            };
            match fields
                .iter_mut()
                .find(|field| field.option.name == option.name)
            {
                Some(field) => {
                    field.declared_by += 1;
                    field.target_types.extend(target_types);
                }
                None => fields.push(UpdateField {
                    option,
                    declared_by: 1,
                    target_types: target_types.into_iter().collect(),
                }),
            }
        }
    }
    fields.sort_by_key(|field| std::cmp::Reverse(field.declared_by));
    fields
}

/// One item per option `spec` takes and the block does not already hold —
/// after a mode sign only an `.. entity-update::`'s list fields, which are
/// the ones a sign applies to.
fn option_items(
    context: &OptionContext,
    spec: &DirectiveSpec<'_>,
    fields: &[UpdateField<'_>],
    range: lsp_types::Range,
) -> Vec<CompletionItem> {
    let listed = spec.option_specs().filter(|_| context.mode.is_none());
    let most = fields.first().map_or(0, |field| field.declared_by);
    let further = fields
        .iter()
        .filter(|field| context.mode.is_none() || field.option.value.is_list())
        .map(|field| (field.option, field.declared_by < most));
    listed
        .map(|option| (option, false))
        .chain(further)
        .filter(|(option, _)| {
            !option
                .names()
                .any(|name| context.written.iter().any(|written| written == name))
        })
        .map(|(option, partial)| option_item(option, partial, context.closed, range))
        .collect()
}

/// The item completing `option`'s name over `range`: its canonical name, the
/// closing `:` unless it is written, and the space before a value. An option
/// this build does not implement is marked so and listed last, as is a field
/// only some of an update's matched types declare (`partial`).
fn option_item(
    option: &OptionSpec<'_>,
    partial: bool,
    closed: bool,
    range: lsp_types::Range,
) -> CompletionItem {
    let new_text = match (closed, option.value) {
        (true, _) => option.name.to_string(),
        (false, OptionValue::Flag) => format!("{}:", option.name),
        (false, _) => format!("{}: ", option.name),
    };
    let (documentation, tags, rank) = match option.support {
        OptionSupport::Supported => (None, None, u8::from(partial)),
        OptionSupport::Unsupported { advice } => {
            let refusal = "Not supported by this build, which ignores it.";
            let text = advice.map_or_else(
                || refusal.to_string(),
                |advice| format!("{refusal} {advice}"),
            );
            (
                Some(Documentation::String(text)),
                Some(vec![CompletionItemTag::DEPRECATED]),
                2,
            )
        }
    };
    let aliases = !option.aliases.is_empty();
    CompletionItem {
        label: option.name.to_string(),
        kind: Some(CompletionItemKind::PROPERTY),
        detail: Some(option.doc.to_string()),
        documentation,
        tags,
        sort_text: Some(format!("{rank}{}", option.name)),
        filter_text: aliases.then(|| option.names().collect::<Vec<_>>().join(" ")),
        text_edit: Some(CompletionTextEdit::Edit(TextEdit { range, new_text })),
        ..CompletionItem::default()
    }
}

/// One item per word an option of `value` is offered — for a list, the
/// words the list does not hold yet, each replacing the item at the cursor.
fn value_items(
    context: &ValueContext,
    value: OptionValue<'_>,
    range: &dyn Fn(&Range<usize>) -> lsp_types::Range,
) -> Vec<CompletionItem> {
    let words: Vec<&str> = match value {
        OptionValue::Choice(words) => words(),
        OptionValue::Declared(words) | OptionValue::DeclaredList(words) => {
            words.iter().map(String::as_str).collect()
        }
        _ => return Vec::new(),
    };
    let (replace, listed) = if value.is_list() {
        list_item_at(context)
    } else {
        let end = context.value_start + context.value.chars().count();
        (context.value_start..end, Vec::new())
    };
    words
        .into_iter()
        .filter(|word| !listed.contains(word))
        .map(|word| CompletionItem {
            label: word.to_string(),
            kind: Some(CompletionItemKind::ENUM_MEMBER),
            text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                range: range(&replace),
                new_text: word.to_string(),
            })),
            ..CompletionItem::default()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use rinx_entity::{NoReservedNames, load_schema};
    use rinx_index::EntityRecord;

    /// A schema of two types: `req`, with a `status` choice, a `tags` choice
    /// list and a `links` relation, and `spec`, with only `status`.
    fn schema() -> EntitySchema {
        load_schema(
            r#"
            [[entity_type]]
            name = "req"
            label = "Requirement"

              [[entity_type.attribute]]
              name = "status"
              type = "enum"
              values = ["open", "closed"]

              [[entity_type.attribute]]
              name = "tags"
              type = "list<enum>"
              values = ["ui", "core", "docs"]

              [[entity_type.relation]]
              name = "links"

            [[entity_type]]
            name = "spec"

              [[entity_type.attribute]]
              name = "status"
              type = "enum"
              values = ["draft"]
            "#,
            &NoReservedNames,
        )
        .expect("the test schema should load")
    }

    /// An index holding `REQ_1`, a `req`.
    fn index() -> ProjectIndex {
        let mut index = ProjectIndex::default();
        index.entities.insert(
            EntityId::new("REQ_1").unwrap(),
            EntityRecord {
                relation_spans: BTreeMap::new(),
                type_name: "req".to_string(),
                doc_path: "a.rst".to_string(),
                title: None,
                attributes: BTreeMap::new(),
                outgoing: BTreeMap::new(),
                uml: BTreeMap::new(),
            },
        );
        index
    }

    /// The labels of the items completing `context`, under the `py` default
    /// domain.
    fn labels(context: &DirectiveContext) -> Vec<String> {
        items(context, TextFormat::Plain)
            .into_iter()
            .map(|item| item.label)
            .collect()
    }

    fn items(context: &DirectiveContext, format: TextFormat) -> Vec<CompletionItem> {
        let schema = schema();
        let index = index();
        let vocabulary = DirectiveVocabulary {
            domain: Domain::Py,
            schema: &schema,
            index: &index,
        };
        directive_items(
            context,
            &vocabulary,
            &|character| lsp_types::Position::new(0, u32::try_from(character).unwrap()),
            format,
        )
        .0
    }

    fn name_context(closed: bool, needs_space: bool) -> DirectiveContext {
        DirectiveContext::Name(NameContext {
            typed: String::new(),
            replace: 3..3,
            needs_space,
            closed,
        })
    }

    fn option_context(directive: &str, argument: &str, mode: Option<char>) -> DirectiveContext {
        DirectiveContext::Option(OptionContext {
            directive: WrittenDirective {
                name: directive.to_string(),
                argument: argument.to_string(),
            },
            mode,
            typed: String::new(),
            replace: 4..4,
            closed: false,
            written: Vec::new(),
        })
    }

    fn value_context(
        directive: &str,
        option: &str,
        value: &str,
        cursor: usize,
    ) -> DirectiveContext {
        DirectiveContext::Value(ValueContext {
            directive: WrittenDirective {
                name: directive.to_string(),
                argument: "REQ_1".to_string(),
            },
            option: option.to_string(),
            value: value.to_string(),
            value_start: 10,
            cursor,
            id: None,
        })
    }

    fn new_text(item: &CompletionItem) -> &str {
        match &item.text_edit {
            Some(CompletionTextEdit::Edit(edit)) => &edit.new_text,
            other => panic!("no plain edit: {other:?}"),
        }
    }

    fn named<'i>(items: &'i [CompletionItem], label: &str) -> &'i CompletionItem {
        items
            .iter()
            .find(|item| item.label == label)
            .unwrap_or_else(|| panic!("no item `{label}`"))
    }

    #[test]
    fn test_name_items_offer_every_directive_by_each_spelling() {
        // When
        let labels = labels(&name_context(false, false));

        // Then — the table, the default domain's bare names and every
        // qualified one, and the schema's types
        for expected in [
            "note",
            "needtable",
            "function",
            "py:function",
            "c:function",
            "option",
            "currentmodule",
            "c:namespace",
            "req",
            "spec",
        ] {
            assert!(
                labels.iter().any(|label| label == expected),
                "no `{expected}`"
            );
        }
        assert_eq!(
            labels.iter().filter(|label| *label == "function").count(),
            1
        );
        assert!(!labels.iter().any(|label| label == "namespace"));
        assert!(!labels.iter().any(|label| label == "std:option"));
    }

    #[test]
    fn test_name_items_describe_a_directive_by_its_spec() {
        // When
        let items = items(&name_context(false, false), TextFormat::Plain);

        // Then
        assert_eq!(
            named(&items, "toctree").detail.as_deref(),
            Some("A table of contents linking other documents into the navigation.")
        );
        assert_eq!(named(&items, "req").detail.as_deref(), Some("Requirement"));
    }

    #[test]
    fn test_name_insertion_writes_the_colons_and_an_argument_s_space() {
        // When
        let items = items(&name_context(false, false), TextFormat::Plain);

        // Then
        assert_eq!(new_text(named(&items, "image")), "image:: ");
        assert_eq!(new_text(named(&items, "toctree")), "toctree::");
        assert_eq!(
            named(&items, "image").insert_text_format,
            Some(InsertTextFormat::PLAIN_TEXT)
        );
    }

    #[test]
    fn test_name_insertion_writes_only_the_name_when_the_colons_follow() {
        // When
        let items = items(&name_context(true, false), TextFormat::Snippet);

        // Then
        assert_eq!(new_text(named(&items, "image")), "image");
    }

    #[test]
    fn test_name_insertion_writes_the_space_after_the_dots() {
        // When
        let items = items(&name_context(false, true), TextFormat::Plain);

        // Then
        assert_eq!(new_text(named(&items, "note")), " note::");
    }

    #[test]
    fn test_name_insertion_as_a_snippet_stops_at_the_argument_and_the_content() {
        // When
        let items = items(&name_context(false, false), TextFormat::Snippet);

        // Then
        assert_eq!(new_text(named(&items, "figure")), "figure:: $1\n\n   $0");
        assert_eq!(new_text(named(&items, "image")), "image:: $1$0");
        assert_eq!(new_text(named(&items, "note")), "note::\n\n   $0");
        assert_eq!(
            named(&items, "note").insert_text_format,
            Some(InsertTextFormat::SNIPPET)
        );
    }

    #[test]
    fn test_option_items_list_a_directive_s_options_and_its_groups() {
        // When
        let labels = labels(&option_context("figure", "a.png", None));

        // Then — its own `:figwidth:` and the picture group's `:alt:`
        assert!(labels.iter().any(|label| label == "figwidth"));
        assert!(labels.iter().any(|label| label == "alt"));
    }

    #[test]
    fn test_option_items_leave_out_an_option_already_written_by_any_spelling() {
        // Given
        let mut context = option_context("py:function", "f()", None);
        if let DirectiveContext::Option(option) = &mut context {
            option.written = vec!["noindex".to_string()];
        }

        // When
        let labels = labels(&context);

        // Then
        assert!(!labels.iter().any(|label| label == "no-index"));
        assert!(labels.iter().any(|label| label == "async"));
    }

    #[test]
    fn test_option_items_resolve_a_bare_object_name_against_the_default_domain() {
        // When — `function` is `py:function` under the `py` default domain
        let labels = labels(&option_context("function", "f()", None));

        // Then
        assert!(labels.iter().any(|label| label == "async"));
    }

    #[test]
    fn test_option_item_writes_the_colon_and_a_value_s_space() {
        // When
        let items = items(&option_context("toctree", "", None), TextFormat::Plain);

        // Then
        assert_eq!(new_text(named(&items, "maxdepth")), "maxdepth: ");
        assert_eq!(new_text(named(&items, "hidden")), "hidden:");
    }

    #[test]
    fn test_option_item_marks_an_unsupported_option_and_ranks_it_last() {
        // When
        let items = items(
            &option_context("py:function", "f()", None),
            TextFormat::Plain,
        );

        // Then
        let canonical = named(&items, "canonical");
        assert_eq!(canonical.tags, Some(vec![CompletionItemTag::DEPRECATED]));
        assert!(canonical.sort_text.as_deref().unwrap().starts_with('2'));
        let supported = named(&items, "async");
        assert_eq!(supported.tags, None);
        assert!(supported.sort_text.as_deref().unwrap().starts_with('0'));
    }

    #[test]
    fn test_option_item_filters_by_every_spelling() {
        // When
        let items = items(
            &option_context("py:function", "f()", None),
            TextFormat::Plain,
        );

        // Then
        assert_eq!(
            named(&items, "no-index").filter_text.as_deref(),
            Some("no-index noindex")
        );
        assert_eq!(named(&items, "async").filter_text, None);
    }

    #[test]
    fn test_option_items_of_an_entity_type_are_its_id_attributes_and_relations() {
        // When
        let labels = labels(&option_context("req", "A title", None));

        // Then
        assert_eq!(labels, ["id", "status", "tags", "links"]);
    }

    #[test]
    fn test_option_items_of_an_update_are_the_matched_type_s_fields() {
        // When
        let labels = labels(&option_context("entity-update", "REQ_1", None));

        // Then — `:strict:`, then `req`'s fields, never its id
        assert_eq!(labels, ["strict", "status", "tags", "links"]);
    }

    #[test]
    fn test_option_items_of_an_unresolved_update_are_every_type_s_fields() {
        // When
        let items = items(
            &option_context("needextend", "NOPE", None),
            TextFormat::Plain,
        );

        // Then — `status`, which both types declare, ranks above the rest
        let labels: Vec<&str> = items.iter().map(|item| item.label.as_str()).collect();
        assert_eq!(labels, ["strict", "status", "tags", "links"]);
        assert!(
            named(&items, "status")
                .sort_text
                .as_deref()
                .unwrap()
                .starts_with('0')
        );
        assert!(
            named(&items, "tags")
                .sort_text
                .as_deref()
                .unwrap()
                .starts_with('1')
        );
    }

    #[test]
    fn test_option_items_after_a_mode_sign_are_an_update_s_list_fields() {
        // When / Then
        assert_eq!(
            labels(&option_context("entity-update", "REQ_1", Some('+'))),
            ["tags", "links"]
        );
        assert!(labels(&option_context("toctree", "", Some('+'))).is_empty());
    }

    #[test]
    fn test_option_items_of_an_unknown_directive_are_none() {
        // When / Then
        assert!(labels(&option_context("frobnicate", "", None)).is_empty());
    }

    #[test]
    fn test_value_items_offer_a_choice_s_words_over_the_whole_value() {
        // When
        let items = items(
            &value_context("figure", "align", "ce", 2),
            TextFormat::Plain,
        );

        // Then
        let center = named(&items, "center");
        let Some(CompletionTextEdit::Edit(edit)) = &center.text_edit else {
            panic!("no edit");
        };
        assert_eq!(edit.range.start.character, 10);
        assert_eq!(edit.range.end.character, 12);
    }

    #[test]
    fn test_value_items_offer_a_declared_choice_of_an_update_s_field() {
        // When
        let labels = labels(&value_context("entity-update", "status", "", 0));

        // Then — `req`'s values, which `REQ_1` is
        assert_eq!(labels, ["open", "closed"]);
    }

    #[test]
    fn test_value_items_of_a_list_leave_out_the_items_written() {
        // When
        let items = items(
            &value_context("req", "tags", "ui, co", 6),
            TextFormat::Plain,
        );

        // Then
        let labels: Vec<&str> = items.iter().map(|item| item.label.as_str()).collect();
        assert_eq!(labels, ["core", "docs"]);
        let Some(CompletionTextEdit::Edit(edit)) = &items[0].text_edit else {
            panic!("no edit");
        };
        assert_eq!(edit.range.start.character, 14);
        assert_eq!(edit.range.end.character, 16);
    }

    #[test]
    fn test_value_items_of_free_text_are_none() {
        // When / Then
        assert!(labels(&value_context("figure", "alt", "", 0)).is_empty());
        assert!(labels(&value_context("figure", "nope", "", 0)).is_empty());
    }
}

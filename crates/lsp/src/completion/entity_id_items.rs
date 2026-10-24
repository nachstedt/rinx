//! The items completing an entity id in a relation option: `:links:` on an
//! entity, or a relation an `.. entity-update::` changes.
//!
//! Offered are the entities the project index holds whose type every
//! declaration of the relation in force accepts — the rule the index phase
//! reports `entity.disallowed-relation` by, so no id offered is then
//! reported. An entity whose id two documents declare defines nothing and is
//! not in the index, so it is not offered either, as a reference to it would
//! be broken.

use std::ops::Range;

use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionItemLabelDetails, CompletionTextEdit,
    Documentation, MarkupContent, MarkupKind, TextEdit,
};
use rinx_entity::EntitySchema;
use rinx_index::ProjectIndex;

use super::directive_context::{ValueContext, list_item_at};
use crate::entity_hover::indexed_entity_markdown;
use crate::hover::escape_markdown;

/// How many ids a list holds before it is cut, and marked incomplete so the
/// editor asks again as the id is typed: a sphinx-needs project may declare
/// thousands.
const MOST_IDS: usize = 200;

/// The types a relation's targets may have, by each declaration of it in
/// force — `None` for a declaration accepting every type. An entity's own
/// relation has one; a relation an `.. entity-update::` changes has one per
/// matched type declaring it, since the build checks each updated entity by
/// its own type's.
pub type TargetTypes<'s> = [Option<&'s [String]>];

/// Whether an entity of `type_name` may be a target under every declaration
/// in `accepted`.
fn accepts(accepted: &TargetTypes<'_>, type_name: &str) -> bool {
    accepted
        .iter()
        .all(|types| types.is_none_or(|types| types.iter().any(|name| name == type_name)))
}

/// The items completing the id the cursor is on in `context`'s value, a
/// relation accepting `accepted` — leaving out the ids the list already
/// holds and the block's own — and whether the list was cut.
pub fn entity_id_items(
    context: &ValueContext,
    accepted: &TargetTypes<'_>,
    index: &ProjectIndex,
    schema: &EntitySchema,
    range: &dyn Fn(&Range<usize>) -> lsp_types::Range,
) -> (Vec<CompletionItem>, bool) {
    let (replace, listed) = list_item_at(context);
    let typed: String = context
        .value
        .chars()
        .skip(replace.start - context.value_start)
        .take(
            context
                .cursor
                .saturating_sub(replace.start - context.value_start),
        )
        .collect::<String>()
        .to_lowercase();
    let mut candidates: Vec<_> = index
        .entities
        .iter()
        .filter(|(id, record)| {
            accepts(accepted, &record.type_name)
                && !listed.contains(&id.as_str())
                && context.id.as_deref() != Some(id.as_str())
                && id.as_str().to_lowercase().contains(&typed)
        })
        .collect();
    // Those starting with what is typed first, then the ids in order.
    candidates.sort_by_key(|(id, _)| !id.as_str().to_lowercase().starts_with(&typed));
    let cut = candidates.len() > MOST_IDS;
    candidates.truncate(MOST_IDS);
    let items = candidates
        .into_iter()
        .enumerate()
        .map(|(rank, (id, record))| {
            let documentation =
                indexed_entity_markdown(id, index, schema, &escape_markdown(&record.doc_path));
            CompletionItem {
                label: id.to_string(),
                kind: Some(CompletionItemKind::REFERENCE),
                detail: record.title.clone(),
                label_details: Some(CompletionItemLabelDetails {
                    detail: None,
                    description: Some(
                        schema
                            .entity_type(&record.type_name)
                            .map_or(record.type_name.as_str(), |declared| {
                                declared.display_label()
                            })
                            .to_string(),
                    ),
                }),
                documentation: documentation.map(|value| {
                    Documentation::MarkupContent(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value,
                    })
                }),
                sort_text: Some(format!("{rank:04}")),
                text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                    range: range(&replace),
                    new_text: id.to_string(),
                })),
                ..CompletionItem::default()
            }
        })
        .collect();
    (items, cut)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use rinx_ast::EntityId;
    use rinx_entity::{NoReservedNames, load_schema};
    use rinx_index::EntityRecord;

    use super::*;
    use crate::completion::directive_context::WrittenDirective;

    fn schema() -> EntitySchema {
        load_schema(
            "[[entity_type]]\nname = \"req\"\nlabel = \"Requirement\"\n\n\
             [[entity_type]]\nname = \"spec\"\n",
            &NoReservedNames,
        )
        .unwrap()
    }

    /// An index holding each `(id, type)` of `entities`, in `a.rst`.
    fn index(entities: &[(&str, &str)]) -> ProjectIndex {
        let mut index = ProjectIndex::default();
        for (id, type_name) in entities {
            index.entities.insert(
                EntityId::new(id).unwrap(),
                EntityRecord {
                    type_name: (*type_name).to_string(),
                    doc_path: "a.rst".to_string(),
                    title: Some(format!("Title of {id}")),
                    attributes: BTreeMap::new(),
                    outgoing: BTreeMap::new(),
                    uml: BTreeMap::new(),
                    relation_spans: BTreeMap::new(),
                },
            );
        }
        index
    }

    /// The value `value` of `:links:`, its cursor at `cursor`, in the block
    /// of the entity `id`.
    fn context(value: &str, cursor: usize, id: Option<&str>) -> ValueContext {
        ValueContext {
            directive: WrittenDirective {
                name: "req".to_string(),
                argument: String::new(),
            },
            option: "links".to_string(),
            value: value.to_string(),
            value_start: 11,
            cursor,
            id: id.map(str::to_string),
        }
    }

    fn items_for(
        context: &ValueContext,
        accepted: &TargetTypes<'_>,
        index: &ProjectIndex,
    ) -> (Vec<CompletionItem>, bool) {
        let at = |character: &Range<usize>| {
            lsp_types::Range::new(
                lsp_types::Position::new(0, u32::try_from(character.start).unwrap()),
                lsp_types::Position::new(0, u32::try_from(character.end).unwrap()),
            )
        };
        entity_id_items(context, accepted, index, &schema(), &at)
    }

    fn labels(items: &[CompletionItem]) -> Vec<&str> {
        items.iter().map(|item| item.label.as_str()).collect()
    }

    #[test]
    fn test_entity_id_items_offer_only_the_types_the_relation_accepts() {
        // Given
        let index = index(&[("R_1", "req"), ("S_1", "spec"), ("S_2", "spec")]);
        let specs = ["spec".to_string()];

        // When
        let (items, cut) = items_for(&context("", 0, None), &[Some(&specs)], &index);

        // Then
        assert_eq!(labels(&items), ["S_1", "S_2"]);
        assert!(!cut);
    }

    #[test]
    fn test_entity_id_items_need_every_declaration_in_force_to_accept() {
        // Given — one matched type accepts specs, another every type
        let index = index(&[("R_1", "req"), ("S_1", "spec")]);
        let specs = ["spec".to_string()];
        let reqs = ["req".to_string()];

        // When
        let (any_and_specs, _) = items_for(&context("", 0, None), &[None, Some(&specs)], &index);
        let (specs_and_reqs, _) =
            items_for(&context("", 0, None), &[Some(&specs), Some(&reqs)], &index);

        // Then
        assert_eq!(labels(&any_and_specs), ["S_1"]);
        assert!(specs_and_reqs.is_empty());
    }

    #[test]
    fn test_entity_id_items_leave_out_listed_ids_and_the_block_s_own() {
        // Given
        let index = index(&[("R_1", "req"), ("R_2", "req"), ("R_3", "req")]);

        // When — `R_1, |` in the block of `R_3`
        let (items, _) = items_for(&context("R_1, ", 5, Some("R_3")), &[None], &index);

        // Then
        assert_eq!(labels(&items), ["R_2"]);
    }

    #[test]
    fn test_entity_id_items_replace_the_item_under_the_cursor_ranking_prefixes_first() {
        // Given
        let index = index(&[("AB_1", "req"), ("B_2", "req"), ("C_3", "req")]);

        // When — `R_9, b|` with the item starting at character 16
        let (items, _) = items_for(&context("R_9, b", 6, None), &[None], &index);

        // Then
        assert_eq!(labels(&items), ["B_2", "AB_1"]);
        let Some(CompletionTextEdit::Edit(edit)) = &items[0].text_edit else {
            panic!("a plain edit");
        };
        assert_eq!(edit.range.start.character, 16);
        assert_eq!(edit.range.end.character, 17);
        assert_eq!(edit.new_text, "B_2");
    }

    #[test]
    fn test_entity_id_items_describe_each_entity() {
        // Given
        let index = index(&[("R_1", "req")]);

        // When
        let (items, _) = items_for(&context("", 0, None), &[None], &index);

        // Then
        let item = &items[0];
        assert_eq!(item.detail.as_deref(), Some("Title of R_1"));
        assert_eq!(
            item.label_details
                .as_ref()
                .and_then(|details| details.description.as_deref()),
            Some("Requirement")
        );
        let Some(Documentation::MarkupContent(markup)) = &item.documentation else {
            panic!("markdown documentation");
        };
        assert!(
            markup
                .value
                .starts_with("**Requirement** R\\_1 — Title of R\\_1\n\na\\.rst"),
            "{}",
            markup.value
        );
    }

    #[test]
    fn test_entity_id_items_cut_a_long_list_and_say_so() {
        // Given
        let ids: Vec<String> = (0..250).map(|n| format!("R_{n:03}")).collect();
        let entities: Vec<(&str, &str)> = ids.iter().map(|id| (id.as_str(), "req")).collect();
        let index = index(&entities);

        // When
        let (items, cut) = items_for(&context("", 0, None), &[None], &index);

        // Then
        assert_eq!(items.len(), MOST_IDS);
        assert!(cut);
    }
}

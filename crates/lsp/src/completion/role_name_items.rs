//! The completion items a role name is offered: every role this build reads
//! in the document at the cursor.
//!
//! The server keeps no list of roles of its own. The names are the parser's
//! role table's (`rinx_parser::role_specs`), spelled for the document's
//! default domain; the entity roles are the schema's, the way the parser asks
//! it; and the roles a `.. role::` defined are the ones the parse of the text
//! above the cursor holds (`rinx_parser::RolesInForce`) — so completion
//! cannot offer a name the parser does not read as a role there.

use std::collections::BTreeSet;

use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionTextEdit, InsertTextFormat, TextEdit,
};
use rinx_ast::Domain;
use rinx_entity::{BUILTIN_ROLE, EntitySchema, RoleSpec as EntityRoleSpec};
use rinx_parser::{ExternalPrefix, RolesInForce, role_specs};

use super::directive_items::TextFormat;
use super::role_name_context::RoleNameContext;

/// What a document's role names are resolved against: its default domain,
/// its project's entity schema, and the roles it has defined by the cursor.
pub struct RoleVocabulary<'a> {
    /// The domain of an unqualified domain role.
    pub domain: Domain,
    /// The entity roles.
    pub schema: &'a EntitySchema,
    /// The roles `.. role::` defined above the cursor.
    pub in_force: &'a RolesInForce,
}

/// The items completing the role name `context`, each replacing `range`.
///
/// After an `external:` or `external+name:` prefix, only the roles whose
/// target an inventory may hold are offered, spelled with the prefix.
#[must_use]
pub fn role_name_items(
    context: &RoleNameContext,
    vocabulary: &RoleVocabulary<'_>,
    range: lsp_types::Range,
    format: TextFormat,
) -> Vec<CompletionItem> {
    let named = match external_prefix(&context.typed) {
        Some(prefix) => external_names(prefix, vocabulary.domain),
        None => role_names(vocabulary),
    };
    let mut seen = BTreeSet::new();
    named
        .into_iter()
        .filter(|(name, _)| seen.insert(name.clone()))
        .map(|(name, detail)| {
            let (new_text, written_as) = insertion(&name, context.closed, format);
            CompletionItem {
                label: name,
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some(detail),
                insert_text_format: Some(written_as),
                text_edit: Some(CompletionTextEdit::Edit(TextEdit { range, new_text })),
                ..CompletionItem::default()
            }
        })
        .collect()
}

/// Every role name the document reads at the cursor, with what it is for:
/// the table's, then the schema's, then the document's own.
fn role_names(vocabulary: &RoleVocabulary<'_>) -> Vec<(String, String)> {
    let mut named: Vec<(String, String)> = role_specs()
        .flat_map(|spec| {
            spec.spellings(vocabulary.domain)
                .map(|name| (name, spec.doc.to_string()))
        })
        .collect();
    let builtin = EntityRoleSpec::builtin();
    named.extend(
        vocabulary
            .schema
            .roles()
            .iter()
            .chain(
                vocabulary
                    .schema
                    .role(BUILTIN_ROLE)
                    .is_some()
                    .then_some(&builtin),
            )
            .map(|role| (role.name.clone(), entity_role_doc(role))),
    );
    named.extend(vocabulary.in_force.defined.iter().map(|role| {
        (
            role.name.clone(),
            format!("Defined in this document from `{}`.", role.base),
        )
    }));
    named
}

/// The roles an inventory may hold a target of, spelled after `prefix`.
fn external_names(prefix: &str, domain: Domain) -> Vec<(String, String)> {
    role_specs()
        .filter(|spec| spec.external == ExternalPrefix::Accepted)
        .flat_map(|spec| {
            spec.spellings(domain)
                .map(|name| (format!("{prefix}{name}"), spec.doc.to_string()))
        })
        .collect()
}

/// The `external:` or `external+name:` prefix `typed` opens with, if any.
fn external_prefix(typed: &str) -> Option<&str> {
    let rest = typed.strip_prefix("external")?;
    let end = if rest.starts_with(':') {
        0
    } else {
        rest.strip_prefix('+')?.find(':')? + 1
    };
    Some(&typed[..="external".len() + end])
}

/// What an entity role is for.
fn entity_role_doc(role: &EntityRoleSpec) -> String {
    match &role.types {
        None => "Reference an entity of any type.".to_string(),
        Some(types) => format!("Reference an entity of type {}.", types.join(", ")),
    }
}

/// What completing the role name `name` writes: the name, then the `:` and
/// the opening backtick unless the `:` is written already — and, as a
/// snippet, the closing backtick after a tab stop for the target.
fn insertion(name: &str, closed: bool, format: TextFormat) -> (String, InsertTextFormat) {
    if closed {
        return (name.to_string(), InsertTextFormat::PLAIN_TEXT);
    }
    match format {
        TextFormat::Plain => (format!("{name}:`"), InsertTextFormat::PLAIN_TEXT),
        TextFormat::Snippet => (format!("{name}:`$1`$0"), InsertTextFormat::SNIPPET),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::Position;
    use rinx_entity::{NoReservedNames, load_schema};
    use rinx_parser::DefinedRole;

    fn range() -> lsp_types::Range {
        lsp_types::Range::new(Position::new(0, 5), Position::new(0, 5))
    }

    fn context(typed: &str, closed: bool) -> RoleNameContext {
        RoleNameContext {
            typed: typed.to_string(),
            replace: 5..5 + typed.chars().count(),
            closed,
        }
    }

    fn labels(items: &[CompletionItem]) -> Vec<&str> {
        items.iter().map(|item| item.label.as_str()).collect()
    }

    fn items_in(domain: Domain, typed: &str, in_force: &RolesInForce) -> Vec<CompletionItem> {
        let schema = EntitySchema::empty();
        let vocabulary = RoleVocabulary {
            domain,
            schema: &schema,
            in_force,
        };
        role_name_items(
            &context(typed, false),
            &vocabulary,
            range(),
            TextFormat::Plain,
        )
    }

    fn new_text(item: &CompletionItem) -> &str {
        match &item.text_edit {
            Some(CompletionTextEdit::Edit(edit)) => &edit.new_text,
            other => panic!("not an edit: {other:?}"),
        }
    }

    #[test]
    fn test_role_name_items_offers_the_tables_roles_with_their_doc() {
        // When
        let items = items_in(Domain::Py, "", &RolesInForce::default());

        // Then
        let ref_ = items.iter().find(|item| item.label == "ref").unwrap();
        assert_eq!(
            ref_.detail.as_deref(),
            Some("Link a label, showing the title of what it labels.")
        );
        assert!(labels(&items).contains(&"py:func"));
        assert!(labels(&items).contains(&"c:macro"));
    }

    #[test]
    fn test_role_name_items_offers_a_bare_domain_role_only_in_its_domain() {
        // When
        let python = items_in(Domain::Py, "", &RolesInForce::default());
        let c = items_in(Domain::C, "", &RolesInForce::default());

        // Then
        assert!(!labels(&python).contains(&"macro"));
        assert!(labels(&c).contains(&"macro"));
        assert!(labels(&python).contains(&"class"));
        assert!(!labels(&c).contains(&"class"));
    }

    #[test]
    fn test_role_name_items_offers_a_name_two_roles_share_once() {
        // When — `data` is `c`'s member role and `py`'s data role
        let items = items_in(Domain::C, "", &RolesInForce::default());

        // Then
        assert_eq!(labels(&items).iter().filter(|l| **l == "data").count(), 1);
    }

    #[test]
    fn test_role_name_items_offers_the_entity_roles() {
        // Given
        let schema = load_schema(
            r#"
            [[entity_type]]
            name = "req"

            [[role]]
            name = "need"
            types = ["req"]
            "#,
            &NoReservedNames,
        )
        .expect("the test schema should load");
        let in_force = RolesInForce::default();
        let vocabulary = RoleVocabulary {
            domain: Domain::Py,
            schema: &schema,
            in_force: &in_force,
        };

        // When
        let items = role_name_items(&context("", false), &vocabulary, range(), TextFormat::Plain);

        // Then
        let need = items.iter().find(|item| item.label == "need").unwrap();
        assert_eq!(
            need.detail.as_deref(),
            Some("Reference an entity of type req.")
        );
        let entity = items.iter().find(|item| item.label == "entity").unwrap();
        assert_eq!(
            entity.detail.as_deref(),
            Some("Reference an entity of any type.")
        );
    }

    #[test]
    fn test_role_name_items_offers_the_roles_the_document_defined() {
        // Given
        let in_force = RolesInForce {
            defined: vec![DefinedRole {
                name: "python".to_string(),
                base: "code",
            }],
            default_role: None,
        };

        // When
        let items = items_in(Domain::Py, "", &in_force);

        // Then
        let python = items.iter().find(|item| item.label == "python").unwrap();
        assert_eq!(
            python.detail.as_deref(),
            Some("Defined in this document from `code`.")
        );
    }

    #[test]
    fn test_role_name_items_after_an_external_prefix_offers_only_inventory_roles() {
        // When
        let items = items_in(Domain::Py, "external+python:", &RolesInForce::default());

        // Then
        let labels = labels(&items);
        assert!(labels.contains(&"external+python:py:class"));
        assert!(labels.contains(&"external+python:ref"));
        assert!(!labels.iter().any(|label| label.ends_with(":numref")));
        assert!(!labels.iter().any(|label| label.ends_with(":math")));
    }

    #[test]
    fn test_role_name_items_writes_the_colon_and_backtick() {
        // When
        let items = items_in(Domain::Py, "", &RolesInForce::default());

        // Then
        let ref_ = items.iter().find(|item| item.label == "ref").unwrap();
        assert_eq!(new_text(ref_), "ref:`");
    }

    #[test]
    fn test_insertion_as_a_snippet_closes_the_backticks_around_a_tab_stop() {
        // When / Then
        assert_eq!(
            insertion("ref", false, TextFormat::Snippet),
            ("ref:`$1`$0".to_string(), InsertTextFormat::SNIPPET)
        );
    }

    #[test]
    fn test_insertion_writes_only_the_name_when_the_colon_is_written() {
        // When / Then
        assert_eq!(
            insertion("ref", true, TextFormat::Snippet),
            ("ref".to_string(), InsertTextFormat::PLAIN_TEXT)
        );
    }

    #[test]
    fn test_external_prefix_reads_both_forms() {
        // When / Then
        assert_eq!(external_prefix("external:py:fu"), Some("external:"));
        assert_eq!(
            external_prefix("external+python:"),
            Some("external+python:")
        );
        assert_eq!(external_prefix("external+python"), None);
        assert_eq!(external_prefix("ext"), None);
    }
}

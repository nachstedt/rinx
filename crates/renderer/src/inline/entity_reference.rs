//! Entity role (`:req:`, `:need:`, `:entity:`) reference rendering.

use std::fmt::Write as _;

use super::RefText;
use rusty_sphinx_index::EntityRecord;

use crate::resolution::{EntityResolution, EntityResolver};
use crate::{BrokenLink, BrokenLinkKind};

/// Renders an entity reference, linking to the entity's anchor in its document.
///
/// A type mismatch still links: the target exists, and the reader is better
/// served by reaching it than by a dead link. What the mismatch buys is the
/// diagnostic — which is the whole reason a project declares typed roles
/// rather than relying on `:ref:`.
pub(super) fn render_inline_entity_reference(
    html: &mut String,
    role: &str,
    reference: RefText<'_>,
    resolver: &EntityResolver<'_>,
    doc_path: &str,
    broken_links: &mut Vec<BrokenLink>,
) {
    let RefText {
        display,
        target,
        span,
    } = reference;

    let resolution = resolver.resolve(role, target);
    let record = match resolution {
        EntityResolution::Resolved(record) => record,
        EntityResolution::TypeMismatch(record) => {
            broken_links.push(BrokenLink {
                kind: BrokenLinkKind::EntityTypeMismatch {
                    role: role.to_string(),
                    found_type: record.type_name.clone(),
                },
                target: target.to_string(),
                span,
            });
            record
        }
        EntityResolution::NotFound => {
            let display_escaped = html_escape::encode_text(display);
            let _ = write!(
                html,
                "<a href=\"#\" class=\"broken-link\"><span class=\"xref entity\">{display_escaped}</span></a>"
            );
            broken_links.push(BrokenLink {
                kind: BrokenLinkKind::EntityReference(role.to_string()),
                target: target.to_string(),
                span,
            });
            return;
        }
    };

    // A reference written with no explicit title shows the entity's own title
    // when it has one, so `:req:`REQ_001`` reads as prose rather than as an
    // identifier. An explicit title always wins.
    let id = entity_id_of(target);
    let shown = if display == target {
        record.display_text(&id)
    } else {
        display
    };
    let href = entity_href(record, doc_path, target);
    let _ = write!(
        html,
        "<a class=\"reference internal\" href=\"{}\"><span class=\"xref entity entity-{}\">{}</span></a>",
        html_escape::encode_double_quoted_attribute(&href),
        html_escape::encode_double_quoted_attribute(&record.type_name),
        html_escape::encode_text(shown)
    );
}

/// The id this reference names, for the display-text fallback.
///
/// Rebuilt rather than threaded through, because the resolver already proved
/// it parses — a target that did not would have been `NotFound`.
fn entity_id_of(target: &str) -> rusty_sphinx_ast::EntityId {
    rusty_sphinx_ast::EntityId::new(target)
        .expect("the resolver only returns a record for a target that parses")
}

/// The relative link from `doc_path` to `record`'s anchor.
pub(crate) fn entity_href(record: &EntityRecord, doc_path: &str, target: &str) -> String {
    let current_dir = std::path::Path::new(doc_path)
        .parent()
        .unwrap_or_else(|| std::path::Path::new(""));
    let target_html_path = std::path::Path::new(&record.doc_path).with_extension("html");
    let relative_path =
        pathdiff::diff_paths(&target_html_path, current_dir).unwrap_or(target_html_path);
    format!(
        "{}#{}",
        relative_path.display(),
        crate::blocks::entity_anchor(target)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_sphinx_ast::EntityId;
    use rusty_sphinx_entity::{EntitySchema, NoReservedNames, load_schema};
    use rusty_sphinx_index::ProjectIndex;
    use std::collections::BTreeMap;

    fn schema() -> EntitySchema {
        load_schema(
            r#"
            [[entity_type]]
            name = "req"

            [[entity_type]]
            name = "spec"

            [[role]]
            name = "req"
            types = ["req"]
            "#,
            &NoReservedNames,
        )
        .unwrap()
    }

    fn index(title: Option<&str>) -> ProjectIndex {
        let mut index = ProjectIndex::default();
        index.entities.insert(
            EntityId::new("REQ_001").unwrap(),
            EntityRecord {
                type_name: "req".to_string(),
                doc_path: "specs/boot".to_string(),
                title: title.map(ToString::to_string),
                attributes: BTreeMap::new(),
                outgoing: BTreeMap::new(),
                uml: BTreeMap::new(),
            },
        );
        index.entities.insert(
            EntityId::new("SPEC_003").unwrap(),
            EntityRecord {
                type_name: "spec".to_string(),
                doc_path: "specs/boot".to_string(),
                title: None,
                attributes: BTreeMap::new(),
                outgoing: BTreeMap::new(),
                uml: BTreeMap::new(),
            },
        );
        index
    }

    fn render(
        role: &str,
        target: &str,
        display: &str,
        title: Option<&str>,
    ) -> (String, Vec<BrokenLink>) {
        let index = index(title);
        let schema = schema();
        let resolver = EntityResolver::new(&index, &schema);
        let mut html = String::new();
        let mut broken = Vec::new();
        render_inline_entity_reference(
            &mut html,
            role,
            RefText {
                display,
                target,
                span: None,
            },
            &resolver,
            "index",
            &mut broken,
        );
        (html, broken)
    }

    #[test]
    fn test_a_resolved_reference_links_to_the_entitys_anchor() {
        // Given / When
        let (html, broken) = render("req", "REQ_001", "REQ_001", None);

        // Then
        assert!(
            html.contains("href=\"specs/boot.html#entity-REQ_001\""),
            "unexpected html: {html}"
        );
        assert!(broken.is_empty());
    }

    #[test]
    fn test_a_bare_reference_shows_the_entitys_title() {
        // Given / When
        let (html, _) = render("req", "REQ_001", "REQ_001", Some("Boot quickly"));

        // Then
        assert!(html.contains(">Boot quickly<"), "unexpected html: {html}");
    }

    #[test]
    fn test_a_bare_reference_falls_back_to_the_id() {
        // Given — a type that maps no title
        let (html, _) = render("req", "REQ_001", "REQ_001", None);

        // Then
        assert!(html.contains(">REQ_001<"), "unexpected html: {html}");
    }

    #[test]
    fn test_an_explicit_title_wins_over_the_entitys_own() {
        // Given / When
        let (html, _) = render("req", "REQ_001", "the boot rule", Some("Boot quickly"));

        // Then
        assert!(html.contains(">the boot rule<"), "unexpected html: {html}");
    }

    #[test]
    fn test_the_type_is_carried_into_the_css_class() {
        // Given / When
        let (html, _) = render("req", "REQ_001", "REQ_001", None);

        // Then
        assert!(
            html.contains("xref entity entity-req"),
            "unexpected html: {html}"
        );
    }

    #[test]
    fn test_a_missing_target_renders_a_broken_link_and_reports_it() {
        // Given / When
        let (html, broken) = render("req", "NOWHERE", "NOWHERE", None);

        // Then
        assert!(html.contains("class=\"broken-link\""));
        assert_eq!(
            broken[0].kind,
            BrokenLinkKind::EntityReference("req".to_string())
        );
    }

    #[test]
    fn test_a_type_mismatch_links_anyway_and_reports_it() {
        // Given — the target exists, so refusing the link would help nobody
        let (html, broken) = render("req", "SPEC_003", "SPEC_003", None);

        // Then
        assert!(html.contains("class=\"reference internal\""));
        assert_eq!(
            broken[0].kind,
            BrokenLinkKind::EntityTypeMismatch {
                role: "req".to_string(),
                found_type: "spec".to_string(),
            }
        );
    }

    #[test]
    fn test_the_href_is_relative_to_the_referring_document() {
        // Given
        let index = index(None);
        let record = &index.entities[&EntityId::new("REQ_001").unwrap()];

        // When
        let href = entity_href(record, "guide/intro", "REQ_001");

        // Then
        assert_eq!(href, "../specs/boot.html#entity-REQ_001");
    }
}

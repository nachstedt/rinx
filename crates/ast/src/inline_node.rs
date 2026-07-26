use serde::{Deserialize, Serialize};

use crate::object_type::ObjectType;
use crate::target_search_order::TargetSearchOrder;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InlineNode {
    Text(String),
    /// An inline cross-reference produced by the `:ref:` role, linking to a
    /// labeled location elsewhere in the site.
    ///
    /// `display` and `target` differ when the role is written with an
    /// explicit display-text override (the angle-bracket form), same as
    /// [`TermReference`](Self::TermReference).
    Reference {
        display: String,
        target: String,
    },
    Hyperlink {
        text: String,
        target: String,
    },
    AnonymousReference(String),
    AnonymousHyperlink {
        text: String,
        target: String,
    },
    Emphasis(String),
    Strong(String),
    Literal(String),
    Program(String),
    /// An inline cross-reference produced by the term role, linking to a glossary entry.
    ///
    /// The `display` field is the visible link text and `term` is the glossary key.
    /// They differ when the role is written with an explicit display-text override,
    /// i.e. the angle-bracket form where the text before the angle bracket is shown
    /// and the text inside the angle brackets is looked up in the glossary index.
    TermReference {
        display: String,
        term: String,
    },
    /// An inline cross-reference produced by a domain role (e.g. `:func:`,
    /// `:py:func:`, `:c:func:`), linking to a `Directive::DomainObject`.
    ///
    /// `object_type` is always concrete by the time this node exists — the
    /// parser resolves a bare (unprefixed) role via the file's default
    /// domain immediately, mirroring how `Directive::DomainObject` is
    /// resolved.
    ///
    /// `name` and `display` differ when the role target uses a `~` prefix
    /// (e.g. `~pkg.mod.func`): `name` is the full name used to resolve the
    /// cross-reference, `display` is the shortened text shown to the reader
    /// (just the last dotted component). A `!` prefix instead sets `link`
    /// to `false`, suppressing the hyperlink entirely (the target is never
    /// looked up, so a missing target produces no broken-link warning).
    ///
    /// A leading `.` prefix is likewise consumed by the parser: it never
    /// survives into `name` or `display` (it is markup, not part of any
    /// object's name), and is recorded as `search_order` instead.
    DomainObjectReference {
        object_type: ObjectType,
        name: String,
        display: String,
        link: bool,
        /// Which resolution order the target asked for. `#[serde(default)]`
        /// keeps `.ast` files written before this field existed loadable —
        /// they predate leading-dot support, so the default (no dot) is the
        /// faithful reading.
        #[serde(default)]
        search_order: TargetSearchOrder,
    },
}

/// Flattens a sequence of inline nodes down to the plain text a reader would
/// see, discarding all markup/links. Used where a data field requires a
/// plain `String` — e.g. a nav-sidebar label or an HTML `<title>` — rather
/// than for rendering visible HTML body content (which uses `InlineNode`
/// directly so links/emphasis are preserved).
#[must_use]
pub fn inline_plain_text(nodes: &[InlineNode]) -> String {
    nodes
        .iter()
        .map(|node| match node {
            InlineNode::Text(text)
            | InlineNode::Emphasis(text)
            | InlineNode::Strong(text)
            | InlineNode::Literal(text)
            | InlineNode::Program(text)
            | InlineNode::AnonymousReference(text) => text.as_str(),
            InlineNode::Hyperlink { text, .. } | InlineNode::AnonymousHyperlink { text, .. } => {
                text.as_str()
            }
            InlineNode::Reference { display, .. }
            | InlineNode::TermReference { display, .. }
            | InlineNode::DomainObjectReference { display, .. } => display.as_str(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::py_object_type::PyObjectType;

    #[test]
    fn test_inline_node_program_serialization_roundtrip() {
        // Given
        let node = InlineNode::Program("curl".to_string());

        // When
        let json = serde_json::to_string(&node).expect("Failed to serialize");
        let deserialized: InlineNode = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(node, deserialized);
    }

    #[test]
    fn test_reference_serialization_roundtrip() {
        // Given
        let node = InlineNode::Reference {
            display: "GenericAlias".to_string(),
            target: "types-genericalias".to_string(),
        };

        // When
        let json = serde_json::to_string(&node).expect("Failed to serialize");
        let deserialized: InlineNode = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(node, deserialized);
    }

    #[test]
    fn test_reference_display_equals_target_when_no_alias() {
        // Given
        let label = "home-index";

        // When
        let node = InlineNode::Reference {
            display: label.to_string(),
            target: label.to_string(),
        };

        // Then
        if let InlineNode::Reference { display, target } = node {
            assert_eq!(display, target);
        } else {
            panic!("Expected Reference");
        }
    }

    #[test]
    fn test_term_reference_serialization_roundtrip() {
        // Given
        let node = InlineNode::TermReference {
            display: "the environment".to_string(),
            term: "environment".to_string(),
        };

        // When
        let json = serde_json::to_string(&node).expect("Failed to serialize");
        let deserialized: InlineNode = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(node, deserialized);
    }

    #[test]
    fn test_term_reference_display_equals_term_when_no_alias() {
        // Given
        let term_text = "environment";

        // When
        let node = InlineNode::TermReference {
            display: term_text.to_string(),
            term: term_text.to_string(),
        };

        // Then
        if let InlineNode::TermReference { display, term } = node {
            assert_eq!(display, term);
        } else {
            panic!("Expected TermReference");
        }
    }

    #[test]
    fn test_domain_object_reference_serialization_roundtrip() {
        // Given
        let node = InlineNode::DomainObjectReference {
            object_type: ObjectType::Py(PyObjectType::Function),
            name: "foo".to_string(),
            display: "foo".to_string(),
            link: true,
            search_order: TargetSearchOrder::MostQualifiedFirst,
        };

        // When
        let json = serde_json::to_string(&node).expect("Failed to serialize");
        let deserialized: InlineNode = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(node, deserialized);
    }

    #[test]
    fn test_domain_object_reference_deserializes_without_search_order_field() {
        // Given — an `.ast` file written before leading-dot support existed.
        let json = r#"{"DomainObjectReference":{"object_type":"py:function","name":"foo","display":"foo","link":true}}"#;

        // When
        let deserialized: InlineNode = serde_json::from_str(json).expect("Failed to deserialize");

        // Then — it reads as an unprefixed target, not a parse failure.
        assert_eq!(
            deserialized,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(PyObjectType::Function),
                name: "foo".to_string(),
                display: "foo".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
            }
        );
    }

    #[test]
    fn test_inline_plain_text_concatenates_plain_text_nodes() {
        // Given
        let nodes = vec![
            InlineNode::Text("Hello ".to_string()),
            InlineNode::Strong("world".to_string()),
        ];

        // When
        let text = inline_plain_text(&nodes);

        // Then
        assert_eq!(text, "Hello world");
    }

    #[test]
    fn test_inline_plain_text_uses_display_for_reference() {
        // Given — an explicit-title `:ref:`
        let nodes = vec![InlineNode::Reference {
            display: "GenericAlias".to_string(),
            target: "types-genericalias".to_string(),
        }];

        // When
        let text = inline_plain_text(&nodes);

        // Then
        assert_eq!(text, "GenericAlias");
    }

    #[test]
    fn test_inline_plain_text_uses_shortened_display_for_domain_object_reference() {
        // Given — a `~`-shortened domain-object reference
        let nodes = vec![InlineNode::DomainObjectReference {
            object_type: ObjectType::Py(PyObjectType::Module),
            name: "pkg.submodule".to_string(),
            display: "submodule".to_string(),
            link: true,
            search_order: TargetSearchOrder::LeastQualifiedFirst,
        }];

        // When
        let text = inline_plain_text(&nodes);

        // Then
        assert_eq!(text, "submodule");
    }

    #[test]
    fn test_inline_plain_text_uses_display_for_term_reference() {
        // Given
        let nodes = vec![InlineNode::TermReference {
            display: "the env".to_string(),
            term: "environment".to_string(),
        }];

        // When
        let text = inline_plain_text(&nodes);

        // Then
        assert_eq!(text, "the env");
    }

    #[test]
    fn test_inline_plain_text_returns_empty_string_for_empty_input() {
        assert_eq!(inline_plain_text(&[]), "");
    }
}

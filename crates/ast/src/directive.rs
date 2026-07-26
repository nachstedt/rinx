use serde::{Deserialize, Serialize};

use crate::admonition_kind::AdmonitionKind;
use crate::domain_object_body::DomainObjectBody;
use crate::glossary_entry::GlossaryEntry;
use crate::hashed_content::HashedContent;
use crate::index_entry::IndexEntry;
use crate::node::Node;
use crate::version_change_kind::VersionChangeKind;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Directive {
    Toctree {
        paths: Vec<String>,
        maxdepth: Option<usize>,
        ignored_options: Vec<String>,
    },
    PlantUml(HashedContent),
    Admonition {
        kind: AdmonitionKind,
        title: Option<String>,
        collapsible: Option<bool>,
        body: Vec<Node>,
    },
    VersionChange {
        kind: VersionChangeKind,
        version: String,
        body: Vec<Node>,
    },
    SeeAlso {
        body: Vec<Node>,
    },
    Glossary {
        entries: Vec<GlossaryEntry>,
        sorted: bool,
    },
    /// A `.. index::` directive. `id` is the anchor the genindex page links
    /// back to — assigned by a post-parse pass (unique within this document
    /// only, see `rusty_sphinx_parser`'s `assign_index_ids`), not at
    /// construction time, since there's no content-derived identity for a
    /// directive that marks a bare location.
    Index {
        entries: Vec<IndexEntry>,
        id: String,
    },
    DomainObject(DomainObjectBody),
    /// `.. py:currentmodule::` — sets the `py`-domain module context for the
    /// rest of the document without documenting a module. `None` is the
    /// reset form (`.. currentmodule:: None`); the sentinel is resolved by
    /// the parser so no later phase re-interprets the literal string.
    PyCurrentModule {
        module: Option<String>,
    },
    Unknown {
        name: String,
        argument: String,
        body: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inline_node::InlineNode;

    #[test]
    fn test_glossary_directive_serialization_roundtrip() {
        // Given
        let directive = Directive::Glossary {
            entries: vec![GlossaryEntry {
                terms: vec!["term".to_string()],
                definition: vec![],
            }],
            sorted: true,
        };

        // When
        let json = serde_json::to_string(&directive).expect("Failed to serialize");
        let deserialized: Directive = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(directive, deserialized);
    }

    #[test]
    fn test_index_directive_serialization_roundtrip() {
        // Given
        let directive = Directive::Index {
            entries: vec![IndexEntry::Term {
                primary: "foo".to_string(),
                subentry: None,
                main: false,
            }],
            id: "index-0".to_string(),
        };

        // When
        let json = serde_json::to_string(&directive).expect("Failed to serialize");
        let deserialized: Directive = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(directive, deserialized);
    }

    #[test]
    fn test_domain_object_directive_serialization_roundtrip() {
        // Given
        let directive = Directive::DomainObject(DomainObjectBody::CFunction {
            signature: "int add(int a, int b)".to_string(),
            body: vec![Node::Paragraph(vec![InlineNode::Text(
                "Adds two numbers.".to_string(),
            )])],
        });

        // When
        let json = serde_json::to_string(&directive).expect("Failed to serialize");
        let deserialized: Directive = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(directive, deserialized);
    }

    #[test]
    fn test_domain_object_directive_serialization_roundtrip_with_module_options() {
        // Given
        let directive = Directive::DomainObject(DomainObjectBody::PyModule {
            name: "greetings".to_string(),
            platform: Some("Unix, Windows".to_string()),
            synopsis: Some("Greeting utilities.".to_string()),
            deprecated: true,
            body: vec![],
        });

        // When
        let json = serde_json::to_string(&directive).expect("Failed to serialize");
        let deserialized: Directive = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(directive, deserialized);
    }

    #[test]
    fn test_domain_object_directive_serialization_roundtrip_with_data_options() {
        // Given
        let directive = Directive::DomainObject(DomainObjectBody::PyData {
            name: "DEFAULT_TIMEOUT".to_string(),
            type_: Some("int".to_string()),
            value: Some("30".to_string()),
            body: vec![],
        });

        // When
        let json = serde_json::to_string(&directive).expect("Failed to serialize");
        let deserialized: Directive = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(directive, deserialized);
    }

    #[test]
    fn test_py_current_module_directive_serialization_roundtrip_with_module() {
        // Given
        let directive = Directive::PyCurrentModule {
            module: Some("enum".to_string()),
        };

        // When
        let json = serde_json::to_string(&directive).expect("Failed to serialize");
        let deserialized: Directive = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(directive, deserialized);
    }

    #[test]
    fn test_py_current_module_directive_serialization_roundtrip_with_reset() {
        // Given
        let directive = Directive::PyCurrentModule { module: None };

        // When
        let json = serde_json::to_string(&directive).expect("Failed to serialize");
        let deserialized: Directive = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(directive, deserialized);
    }
}

use serde::{Deserialize, Serialize};

use crate::admonition_kind::AdmonitionKind;
use crate::doctest_block::DocTestBlock;
use crate::domain_object_body::DomainObjectBody;
use crate::glossary_entry::GlossaryEntry;
use crate::hashed_content::HashedContent;
use crate::index_entry::IndexEntry;
use crate::list_table_widths::ListTableWidths;
use crate::node::Node;
use crate::table::TableRow;
use crate::table_align::TableAlign;
use crate::target_name::TargetName;
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
    /// `.. list-table::` — a table specified as a nested bullet list (outer
    /// list = rows, each row's own bullet list = cells) rather than
    /// character-art. Reuses [`TableRow`]/[`TableCell`] from the grid-table
    /// implementation for its rows (`colspan`/`rowspan` always 1, since
    /// list-table has no span syntax), but gets its own variant rather than
    /// folding into `Node::Table` because grid tables have none of these
    /// options and would otherwise carry meaningless defaults forever.
    ListTable {
        /// The directive argument — the table's title/caption. `None` when
        /// no argument was given.
        title: Option<String>,
        /// `:header-rows:` — how many leading rows in `rows` are header
        /// rows. 0 (the spec default) when the option is omitted; clamped
        /// to `rows.len()` at parse time.
        header_rows: usize,
        /// `:stub-columns:` — how many leading columns in every row are
        /// stub (row-header) columns. 0 by default; clamped to the actual
        /// column count at parse time.
        stub_columns: usize,
        widths: Option<ListTableWidths>,
        /// `:width:` — an opaque CSS length/percentage (e.g. `"100%"`),
        /// passed through verbatim since it's only ever re-emitted as a
        /// `style` attribute.
        width: Option<String>,
        align: Option<TableAlign>,
        /// `:class:` — space-separated class names, already split.
        classes: Vec<String>,
        /// `:name:` — reuses [`TargetName`] (the same type explicit
        /// hyperlink targets use) so it can be registered in
        /// `ProjectIndex::targets` with no extra conversion.
        name: Option<TargetName>,
        rows: Vec<TableRow>,
    },
    DomainObject(DomainObjectBody),
    /// One block of the `sphinx.ext.doctest` family (`doctest`, `testcode`,
    /// `testoutput`, `testsetup`, `testcleanup`).
    ///
    /// A newtype variant, like [`Self::DomainObject`], because the five
    /// directives form their own closed family with their own option sets —
    /// see [`DocTestBlock`] for why they are not one struct.
    ///
    /// Note that *rendering* these is entirely independent of *executing*
    /// them: the AST carries what a page needs to display, and a separate,
    /// opt-in Bazel test target runs the code. Nothing on this variant depends
    /// on a test having been run.
    DocTest(DocTestBlock),
    /// `.. py:currentmodule::` — sets the `py`-domain module context for the
    /// rest of the document without documenting a module. `None` is the
    /// reset form (`.. currentmodule:: None`); the sentinel is resolved by
    /// the parser so no later phase re-interprets the literal string.
    PyCurrentModule {
        module: Option<String>,
    },
    /// `.. c:namespace::` — sets the `c`-domain scope absolutely for the rest
    /// of the document and resets the namespace push/pop stack. `None` is the
    /// reset-to-global form (`NULL` or `0`); both sentinels are resolved by
    /// the parser so no later phase re-interprets the literal string.
    CNamespace {
        namespace: Option<String>,
    },
    /// `.. c:namespace-push::` — extends the current `c`-domain scope
    /// relatively. Always carries a scope: an empty argument is malformed and
    /// stays a [`Directive::Unknown`] rather than becoming a no-op push.
    CNamespacePush {
        namespace: String,
    },
    /// `.. c:namespace-pop::` — undoes the most recent
    /// [`Directive::CNamespacePush`] in its entirety.
    CNamespacePop,
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
    use crate::non_empty_vector::NonEmptyVector;

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
            signatures: NonEmptyVector::single("int add(int a, int b)".into()),
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
            module: None,
            signatures: NonEmptyVector::single("DEFAULT_TIMEOUT".to_string()),
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

    #[test]
    fn test_c_namespace_directive_serialization_roundtrip_with_scope() {
        // Given
        let directive = Directive::CNamespace {
            namespace: Some("A.B".to_string()),
        };

        // When
        let json = serde_json::to_string(&directive).expect("Failed to serialize");
        let deserialized: Directive = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(directive, deserialized);
    }

    #[test]
    fn test_c_namespace_directive_serialization_roundtrip_with_reset() {
        // Given — the `NULL`/`0` reset form.
        let directive = Directive::CNamespace { namespace: None };

        // When
        let json = serde_json::to_string(&directive).expect("Failed to serialize");
        let deserialized: Directive = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(directive, deserialized);
    }

    #[test]
    fn test_c_namespace_push_directive_serialization_roundtrip() {
        // Given
        let directive = Directive::CNamespacePush {
            namespace: "C.D".to_string(),
        };

        // When
        let json = serde_json::to_string(&directive).expect("Failed to serialize");
        let deserialized: Directive = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(directive, deserialized);
    }

    #[test]
    fn test_c_namespace_pop_directive_serialization_roundtrip() {
        // Given
        let directive = Directive::CNamespacePop;

        // When
        let json = serde_json::to_string(&directive).expect("Failed to serialize");
        let deserialized: Directive = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(directive, deserialized);
    }

    #[test]
    fn test_list_table_directive_serialization_roundtrip() {
        // Given
        use crate::table::TableCell;

        let directive = Directive::ListTable {
            title: Some("Fruit".to_string()),
            header_rows: 1,
            stub_columns: 0,
            widths: Some(ListTableWidths::Explicit(vec![30, 70])),
            width: Some("100%".to_string()),
            align: Some(TableAlign::Center),
            classes: vec!["custom".to_string()],
            name: Some(TargetName::new("fruit-table")),
            rows: vec![TableRow {
                cells: vec![TableCell {
                    colspan: 1,
                    rowspan: 1,
                    content: vec![Node::Paragraph(vec![InlineNode::Text("Fruit".to_string())])],
                }],
            }],
        };

        // When
        let json = serde_json::to_string(&directive).expect("Failed to serialize");
        let deserialized: Directive = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(directive, deserialized);
    }
}

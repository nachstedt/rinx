//! Abstract Syntax Tree representations for the Rusty-Sphinx Document.
//!
//! One module per node type or supporting value type, with four families
//! grouped into their own trees: `object_type` (the `domain:objtype` tags),
//! `doctest` (the `sphinx.ext.doctest` node family), `enumerator`
//! (enumerated-list markers) and `table` (table content and layout).
//! `object_naming` holds the signature/option naming helpers every later
//! pipeline phase shares, and `visit` the traversal every phase walks with.
//!
//! `span`, `diagnostic`, `diagnostic_code` and `suppression` are the
//! reporting vocabulary
//! rather than document content: a [`Diagnostic`] is what a phase records
//! instead of failing, and a [`Span`] is where in the `.rst` it points. They
//! live here because [`Document`] carries them and every later phase both
//! produces and forwards them.

mod admonition_kind;
mod c_signature;
mod definition_list_item;
mod diagnostic;
mod diagnostic_code;
mod directive;
mod doctest;
mod document;
mod domain;
mod domain_object_body;
mod enumerator;
mod glossary_entry;
mod hashed_content;
mod index_entry;
mod inline_node;
mod list_item;
mod node;
mod non_empty_vector;
mod object_naming;
mod object_type;
mod py_version_spec;
mod span;
mod suppression;
mod table;
mod target_name;
mod target_search_order;
mod version_change_kind;
mod visit;

pub use admonition_kind::AdmonitionKind;
pub use c_signature::{CSignature, NameSource, extract_c_object_name};
pub use definition_list_item::DefinitionListItem;
pub use diagnostic::Diagnostic;
pub use diagnostic_code::{DiagnosticCode, UnknownDiagnosticCode};
pub use directive::Directive;
pub use doctest::{
    DocTestBlock, DocTestFlag, DocTestFlagName, DocTestGroup, DocTestGroupSelector, DocTestTrim,
};
pub use document::Document;
pub use domain::Domain;
pub use domain_object_body::DomainObjectBody;
pub use enumerator::{Enumerator, EnumeratorError, EnumeratorFormat, EnumeratorSequence};
pub use glossary_entry::{GlossaryEntry, term_id};
pub use hashed_content::HashedContent;
pub use index_entry::IndexEntry;
pub use inline_node::{InlineNode, inline_plain_text};
pub use list_item::ListItem;
pub use node::Node;
pub use non_empty_vector::NonEmptyVector;
pub use object_naming::{
    build_domain_object_key, extract_option_name, extract_python_object_name,
    split_option_line_specs,
};
pub use object_type::{CObjectType, ObjectType, PyObjectType, StdObjectType};
pub use py_version_spec::{PyVersionClause, PyVersionSpec, PythonVersion, VersionComparison};
pub use span::{Position, Span};
pub use suppression::{Suppression, SuppressionCodes};
pub use table::{TableAlign, TableCell, TableRow, TableSource, TableWidths};
pub use target_name::TargetName;
pub use target_search_order::TargetSearchOrder;
pub use version_change_kind::VersionChangeKind;
pub use visit::walk_nodes;

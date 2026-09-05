//! Abstract Syntax Tree representations for the Rusty-Sphinx Document.
//!
//! One module per node type or supporting value type, with five families
//! grouped into their own trees: `object_type` (the `domain:objtype` tags),
//! `doctest` (the `sphinx.ext.doctest` node family), `enumerator`
//! (enumerated-list markers), `table` (table content and layout) and `image`
//! (the URI, measurement and alignment vocabulary the two image directives
//! share).
//! `object_naming` holds the signature/option naming helpers every later
//! pipeline phase shares, and `visit` the traversal every phase walks with.
//!
//! `span`, `diagnostic`, `diagnostic_code` and `suppression` are the
//! reporting vocabulary
//! rather than document content: a [`Diagnostic`] is what a phase records
//! instead of failing, and a [`Span`] is where in the `.rst` it points. They
//! live here because [`Document`] carries them and every later phase both
//! produces and forwards them.
//!
//! `path_normalization` is the odd one out: a plain `.`/`..` resolver with no
//! document vocabulary in it at all. It lives here because both
//! [`ImageUri::resolve`] and `rusty_sphinx_toctree` need it and the toctree
//! crate already depends on this one, so this is the only place the two can
//! share one implementation.

mod admonition_kind;
mod c_signature;
mod code_block;
mod code_language;
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
mod image;
mod index_entry;
mod inline_node;
mod line_block;
mod list_item;
mod node;
mod non_empty_vector;
mod object_naming;
mod object_type;
mod option_list_item;
mod path_normalization;
mod py_version_spec;
mod section_id;
mod span;
mod suppression;
mod table;
mod target_name;
mod target_search_order;
mod toctree;
mod version_change_kind;
mod visit;

pub use admonition_kind::AdmonitionKind;
pub use c_signature::{CSignature, NameSource, extract_c_object_name};
pub use code_block::{CodeBlock, CodeBlockSource};
pub use code_language::{CodeLanguage, EmptyLanguageName, LanguageName, ResolvedLanguage};
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
pub use image::{
    Figure, FigureWidth, ImageAlign, ImageLoading, ImageOptions, ImageTarget, ImageUri,
    InvalidLength, Length, LengthOrPercentage, LengthUnit, Percentage, is_vertical_name,
};
pub use index_entry::IndexEntry;
pub use inline_node::{InlineNode, inline_plain_text};
pub use line_block::LineBlockItem;
pub use list_item::ListItem;
pub use node::Node;
pub use non_empty_vector::NonEmptyVector;
pub use object_naming::{
    build_domain_object_key, extract_option_name, extract_python_object_name,
    split_option_line_specs,
};
pub use object_type::{CObjectType, ObjectType, PyObjectType, StdObjectType};
pub use option_list_item::{OptionArgument, OptionArgumentDelimiter, OptionListItem, OptionSpec};
pub use path_normalization::{normalize_path, resolve_from_document};
pub use py_version_spec::{PyVersionClause, PyVersionSpec, PythonVersion, VersionComparison};
pub use section_id::{SectionId, SectionIdAllocator, allocate_section_ids, section_slug};
pub use span::{FileId, Position, Span};
pub use suppression::{Suppression, SuppressionCodes};
pub use table::{TableAlign, TableCell, TableRow, TableSource, TableWidths};
pub use target_name::TargetName;
pub use target_search_order::TargetSearchOrder;
pub use toctree::{NumberedDepth, TocEntry, Toctree, ToctreeFlag, ToctreeOptions};
pub use version_change_kind::VersionChangeKind;
pub use visit::walk_nodes;

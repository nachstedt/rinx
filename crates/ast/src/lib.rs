//! Abstract Syntax Tree representations for the Rinx Document.
//!
//! One module per node type or supporting value type, with nine families
//! grouped into their own trees: `object_type` (the `domain:objtype` tags),
//! `doctest` (the `sphinx.ext.doctest` node family), `enumerator`
//! (enumerated-list markers), `table` (table content and layout), `image`
//! (the URI, measurement and alignment vocabulary the two image directives
//! share), `dropdown` (the colour, marker and icon vocabulary
//! sphinx-design's collapsible container is written in), `grid` (the
//! breakpoint, column and layout vocabulary sphinx-design's responsive row is
//! written in) and `entity_table`
//! (the listing directive that asks the entity graph a question, as opposed
//! to `entity`, which holds what an author wrote about one thing) and `uml`
//! (every spelling of a `PlantUML` diagram, which like `entity_table` carries a
//! question the project index answers later).
//! `spacing` sits flat beside those two rather than inside either: the
//! one-or-four `:margin:`/`:padding:` scale is the one piece of vocabulary
//! `dropdown` and `grid` share.
//! `object_naming` holds the signature/option naming helpers every later
//! pipeline phase shares, and `visit` the traversal every phase walks with —
//! `inline_lists` its counterpart over inline content.
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
//! [`AssetUri::resolve`] and `rinx_toctree` need it and the toctree
//! crate already depends on this one, so this is the only place the two can
//! share one implementation.

mod admonition_kind;
mod asset_uri;
mod button_link;
mod c_signature;
mod chart_color;
mod chart_value;
mod code_block;
mod code_language;
mod contents;
mod definition_list_item;
mod description_flags;
mod diagnostic;
mod diagnostic_code;
mod directive;
mod doctest;
mod document;
mod docutils_pep_number;
mod docutils_rfc_number;
mod domain;
mod domain_object_body;
mod dropdown;
mod entity;
mod entity_bar;
mod entity_flow;
mod entity_pie;
mod entity_sequence;
mod entity_table;
mod entity_update;
mod enumerable;
mod enumerator;
mod glossary_entry;
mod grid;
mod hashed_content;
mod image;
mod include_site;
mod index_entry;
mod inline_lists;
mod inline_node;
mod inventory_selector;
mod label_rotation;
mod line_block;
mod link_destination;
mod list_item;
mod module_options;
mod node;
mod non_empty_vector;
mod number_format;
mod object_naming;
mod object_type;
mod option_list_item;
mod path_normalization;
mod py_version_spec;
mod registry_target;
mod script_position;
mod section_id;
mod sectnum;
mod spacing;
mod span;
mod substitution;
mod suppression;
mod table;
mod target_name;
mod target_search_order;
mod toctree;
mod uml;
mod version_change_kind;
mod visit;

pub use admonition_kind::AdmonitionKind;
pub use asset_uri::AssetUri;
pub use button_link::{ButtonFlag, ButtonLink, ButtonTarget, TextAlign};
pub use c_signature::{CSignature, NameSource, extract_c_object_name};
pub use chart_color::{ChartColor, InvalidChartColor};
pub use chart_value::ChartValue;
pub use code_block::{CodeBlock, CodeBlockSource};
pub use code_language::{CodeLanguage, EmptyLanguageName, LanguageName, ResolvedLanguage};
pub use contents::{Contents, ContentsBacklinks, ContentsOptions};
pub use definition_list_item::DefinitionListItem;
pub use description_flags::{DescriptionFlag, DescriptionFlags};
pub use diagnostic::Diagnostic;
pub use diagnostic_code::{DiagnosticCode, UnknownDiagnosticCode};
pub use directive::Directive;
pub use doctest::{
    DocTestBlock, DocTestFlag, DocTestFlagName, DocTestGroup, DocTestGroupSelector, DocTestTrim,
};
pub use document::Document;
pub use docutils_pep_number::{DocutilsPepNumber, InvalidDocutilsPepNumber};
pub use docutils_rfc_number::{DocutilsRfcNumber, InvalidDocutilsRfcNumber};
pub use domain::Domain;
pub use domain_object_body::DomainObjectBody;
pub use dropdown::{Animation, Chevron, Dropdown, InvalidOcticonName, OcticonName, SemanticColor};
pub use entity::{AttributeValue, EntityBody, EntityId, EntityIdError, EntitySection, SectionKind};
pub use entity_bar::{
    BarArrangement, BarGrid, BarOrientation, BarValueLabels, EntityBar, EntityBarSource, RaggedGrid,
};
pub use entity_flow::{EntityFlow, EntityFlowSource, FlowDirection, InvalidFlowDirection};
pub use entity_pie::{EntityPie, EntityPieSource, PieSlice};
pub use entity_sequence::{EntitySequence, EntitySequenceSource};
pub use entity_table::{EntityTable, EntityTableSource};
pub use entity_update::{
    EntityUpdate, EntityUpdateSource, FieldMutation, FieldMutationMode, UpdateTarget,
};
pub use enumerable::{EnumerableElement, EnumerableKind, enumerable_elements, preceding_labels};
pub use enumerator::{Enumerator, EnumeratorError, EnumeratorFormat, EnumeratorSequence};
pub use glossary_entry::{GlossaryEntry, term_id};
pub use grid::{
    ChildAlign, ChildDirection, ColumnPrefix, ColumnSpec, Grid, GridItem, Gutter, InvalidMediaSpec,
    MediaDomain, MediaSpec, MediaValue,
};
pub use hashed_content::HashedContent;
pub use image::{
    Figure, FigureWidth, ImageAlign, ImageLoading, ImageOptions, ImageTarget, InvalidLength,
    Length, LengthOrPercentage, LengthUnit, Percentage, is_vertical_name, scaled_width,
};
pub use include_site::IncludeSite;
pub use index_entry::{IndexEntry, IndexEntryType, InvalidIndexEntry};
pub use inline_lists::{for_each_inline_list, for_each_inline_list_mut};
pub use inline_node::{InlineNode, NumberReferenceRefusal, RoleRefusal, inline_plain_text};
pub use inventory_selector::InventorySelector;
pub use label_rotation::{InvalidLabelRotation, LabelRotation};
pub use line_block::LineBlockItem;
pub use link_destination::{HyperlinkTarget, LinkDestination};
pub use list_item::ListItem;
pub use module_options::{ModuleFlag, ModuleOptions};
pub use node::Node;
pub use non_empty_vector::NonEmptyVector;
pub use number_format::{InvalidNumberFormat, NumberFormat};
pub use object_naming::{
    build_domain_object_key, extract_option_name, extract_python_object_name,
    split_option_line_specs,
};
pub use object_type::{CObjectType, ObjectType, PyObjectType, StdObjectType};
pub use option_list_item::{OptionArgument, OptionArgumentDelimiter, OptionListItem, OptionSpec};
pub use path_normalization::{normalize_path, resolve_from_document};
pub use py_version_spec::{PyVersionClause, PyVersionSpec, PythonVersion, VersionComparison};
pub use registry_target::{
    CveTarget, CweTarget, InvalidRegistryTarget, InvalidRegistryTargetReason, PepTarget, Registry,
    RegistryTarget, RfcTarget,
};
pub use rinx_inventory::InventoryName;
pub use script_position::ScriptPosition;
pub use section_id::{SectionId, SectionIdAllocator, allocate_section_ids, section_slug};
pub use sectnum::SectnumOptions;
pub use spacing::{InvalidSpacing, Spacing, SpacingKind, SpacingValue};
pub use span::{FileId, Position, Span};
pub use substitution::{SubstitutionDefinition, SubstitutionKind, TrimSides};
pub use suppression::{Reported, Suppression, SuppressionCodes, is_suppressed, retain_reportable};
pub use table::{TableAlign, TableCell, TableRow, TableSource, TableWidths};
pub use target_name::TargetName;
pub use target_search_order::TargetSearchOrder;
pub use toctree::{NumberedDepth, TocEntry, Toctree, ToctreeFlag, ToctreeOptions};
pub use uml::{Uml, UmlSource};
pub use version_change_kind::VersionChangeKind;
pub use visit::{Child, for_each_child, walk_nodes, walk_nodes_with_siblings};

//! The parser module converts RST text into an Abstract Syntax Tree (Document).

pub(crate) mod blocks;
pub(crate) mod context;
pub(crate) mod default_role;
pub(crate) mod diagnostics;
pub(crate) mod directives;
pub(crate) mod document_roles;
pub(crate) mod entity_schema;
pub(crate) mod explicit_title;
pub(crate) mod headings;
pub(crate) mod indent;
pub(crate) mod inline;
pub(crate) mod templating;
pub(crate) mod width;

pub use blocks::{parse, parse_with_ctx, parse_with_domain};
pub use context::{LoadedFile, ParseCtx, ParseFileLoader, RejectParseFiles, SplicedText};
pub use default_role::{DefaultRole, UnknownRole};
pub use directives::{
    Arguments, DirectiveShape, DirectiveSpec, OptionSpec, OptionSupport, OptionValue,
    directive_specs, is_builtin_directive_name,
};
pub use entity_schema::load_entity_schema;
pub use templating::rendered_source;

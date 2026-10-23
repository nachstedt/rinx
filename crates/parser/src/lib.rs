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

pub use blocks::{PrefixParse, parse, parse_prefix_with_ctx, parse_with_ctx, parse_with_domain};
pub use context::{LoadedFile, ParseCtx, ParseFileLoader, RejectParseFiles, SplicedText};
pub use default_role::{DefaultRole, UnknownRole};
pub use directives::{
    Arguments, DirectiveSpec, EntitySpec, OptionSet, OptionSpec, OptionSupport, OptionValue,
    directive_spec, directive_specs, domain_directive_specs, is_builtin_directive_name,
};
pub use document_roles::{DefinedRole, RolesInForce};
pub use entity_schema::load_entity_schema;
pub use inline::{ExternalPrefix, RoleSpec, can_precede_inline_markup, role_specs};
pub use templating::rendered_source;

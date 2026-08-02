use super::blocks::parse_blocks;
use super::bullet_list::unindent_body_lines;
use super::directives::DirectiveObjectType;
use super::headings::Adornment;
use rusty_sphinx_ast::{Domain, DomainObjectBody, Node, NonEmptyVector};

/// Parses a domain object directive body (e.g. `.. py:function::`,
/// `.. py:module::`, `.. c:function::`) into the matching [`DomainObjectBody`]
/// variant, dispatching on `object_type` since each object type has its own
/// shape (only `py:module` has `platform`/`synopsis`/`deprecated`, for
/// instance). The legacy `Classmethod`/`Staticmethod` directive-name aliases
/// map to the same `py:method` body as `PyMethod`, with the matching flag
/// forced on.
///
/// `argument` is the directive's own argument line and `continuations` the
/// further argument lines that followed it (see
/// [`super::blocks::collect_argument_continuation_lines`]) — each declares
/// another alias for the same object. Taking them as two parameters rather
/// than one list is what makes the resulting [`NonEmptyVector`] non-empty by
/// construction, with no validation step and no error path. `py:module` is
/// the one object type that ignores `continuations`: real Sphinx's `module`
/// directive takes exactly one argument.
pub(super) fn parse_domain_object(
    object_type: DirectiveObjectType,
    argument: String,
    continuations: Vec<String>,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
    default_domain: Domain,
) -> DomainObjectBody {
    let signatures = NonEmptyVector::new(argument, continuations);
    match object_type {
        DirectiveObjectType::PyFunction => DomainObjectBody::PyFunction {
            signatures,
            body: parse_body(body_lines, adornment_order, diagnostics, default_domain),
        },
        DirectiveObjectType::CFunction
        | DirectiveObjectType::CMacro
        | DirectiveObjectType::CStruct
        | DirectiveObjectType::CUnion
        | DirectiveObjectType::CMember
        | DirectiveObjectType::CType => parse_c_domain_object(
            object_type,
            signatures,
            body_lines,
            adornment_order,
            diagnostics,
            default_domain,
        ),
        DirectiveObjectType::PyModule => parse_py_module(
            signatures.first().clone(),
            body_lines,
            adornment_order,
            diagnostics,
            default_domain,
        ),
        DirectiveObjectType::PyData => parse_py_data(
            signatures,
            body_lines,
            adornment_order,
            diagnostics,
            default_domain,
        ),
        DirectiveObjectType::PyMethod => parse_py_method(
            signatures,
            body_lines,
            adornment_order,
            diagnostics,
            default_domain,
            false,
            false,
        ),
        DirectiveObjectType::PyClassmethod => parse_py_method(
            signatures,
            body_lines,
            adornment_order,
            diagnostics,
            default_domain,
            true,
            false,
        ),
        DirectiveObjectType::PyStaticmethod => parse_py_method(
            signatures,
            body_lines,
            adornment_order,
            diagnostics,
            default_domain,
            false,
            true,
        ),
        DirectiveObjectType::PyClass => parse_py_class(
            signatures,
            body_lines,
            adornment_order,
            diagnostics,
            default_domain,
        ),
        DirectiveObjectType::PyException => parse_py_exception(
            signatures,
            body_lines,
            adornment_order,
            diagnostics,
            default_domain,
        ),
        DirectiveObjectType::PyAttribute => parse_py_attribute(
            signatures,
            body_lines,
            adornment_order,
            diagnostics,
            default_domain,
        ),
    }
}

/// Dispatches the six `c`-domain object types to their respective parsers —
/// factored out of [`parse_domain_object`] purely to keep that function's
/// line count manageable. Only ever called with a `DirectiveObjectType::C*`
/// variant (enforced by `parse_domain_object`'s own match arm), so the
/// non-`c` variants are unreachable here.
fn parse_c_domain_object(
    object_type: DirectiveObjectType,
    signatures: NonEmptyVector<String>,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
    default_domain: Domain,
) -> DomainObjectBody {
    match object_type {
        DirectiveObjectType::CFunction => DomainObjectBody::CFunction {
            signatures,
            body: parse_body(body_lines, adornment_order, diagnostics, default_domain),
        },
        DirectiveObjectType::CMacro => DomainObjectBody::CMacro {
            signatures,
            body: parse_body(body_lines, adornment_order, diagnostics, default_domain),
        },
        DirectiveObjectType::CStruct => parse_c_struct(
            signatures,
            body_lines,
            adornment_order,
            diagnostics,
            default_domain,
        ),
        DirectiveObjectType::CUnion => parse_c_union(
            signatures,
            body_lines,
            adornment_order,
            diagnostics,
            default_domain,
        ),
        DirectiveObjectType::CMember => parse_c_member(
            signatures,
            body_lines,
            adornment_order,
            diagnostics,
            default_domain,
        ),
        DirectiveObjectType::CType => parse_c_type(
            signatures,
            body_lines,
            adornment_order,
            diagnostics,
            default_domain,
        ),
        DirectiveObjectType::PyFunction
        | DirectiveObjectType::PyModule
        | DirectiveObjectType::PyData
        | DirectiveObjectType::PyMethod
        | DirectiveObjectType::PyClassmethod
        | DirectiveObjectType::PyStaticmethod
        | DirectiveObjectType::PyClass
        | DirectiveObjectType::PyAttribute
        | DirectiveObjectType::PyException => {
            unreachable!("parse_c_domain_object called with a non-c object type")
        }
    }
}

/// Strips the body's common leading indentation and parses the remaining
/// lines as block-level nodes. Shared by every domain object type that has
/// no directive-specific options to strip out first.
fn parse_body(
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
    default_domain: Domain,
) -> Vec<Node> {
    let unindented_lines = unindent_body_lines(body_lines);
    let body_content: Vec<&str> = unindented_lines.iter().map(String::as_str).collect();
    parse_blocks(&body_content, adornment_order, diagnostics, default_domain)
}

/// Parses a `.. py:module::` body: strips `:platform:`/`:synopsis:`/
/// `:deprecated:` option lines off the front before parsing the rest as the
/// docstring body.
fn parse_py_module(
    name: String,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
    default_domain: Domain,
) -> DomainObjectBody {
    let unindented_lines = unindent_body_lines(body_lines);
    let (platform, synopsis, deprecated, options_consumed) =
        extract_module_options(&unindented_lines);

    let body_content: Vec<&str> = unindented_lines[options_consumed..]
        .iter()
        .map(String::as_str)
        .collect();
    let body = parse_blocks(&body_content, adornment_order, diagnostics, default_domain);

    DomainObjectBody::PyModule {
        name,
        platform,
        synopsis,
        deprecated,
        body,
    }
}

/// Parses a `.. py:data::` body: strips `:type:`/`:value:` option lines off
/// the front before parsing the rest as the docstring body.
fn parse_py_data(
    signatures: NonEmptyVector<String>,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
    default_domain: Domain,
) -> DomainObjectBody {
    let unindented_lines = unindent_body_lines(body_lines);
    let (type_, value, options_consumed) = extract_data_options(&unindented_lines);

    let body_content: Vec<&str> = unindented_lines[options_consumed..]
        .iter()
        .map(String::as_str)
        .collect();
    let body = parse_blocks(&body_content, adornment_order, diagnostics, default_domain);

    DomainObjectBody::PyData {
        signatures,
        type_,
        value,
        body,
    }
}

/// Parses a `.. py:method::` body: strips `:classmethod:`/`:staticmethod:`/
/// `:abstractmethod:`/`:async:` flag lines off the front before parsing the
/// rest as the docstring body.
///
/// `forced_classmethod`/`forced_staticmethod` come from the legacy
/// `.. classmethod::`/`.. staticmethod::` directive-name aliases (which are
/// just `py:method` with the matching flag implied); they are OR-ed with any
/// flag the body's own `:classmethod:`/`:staticmethod:` option lines set, so
/// the alias spelling and the explicit option spelling compose rather than
/// conflict.
fn parse_py_method(
    signatures: NonEmptyVector<String>,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
    default_domain: Domain,
    forced_classmethod: bool,
    forced_staticmethod: bool,
) -> DomainObjectBody {
    let unindented_lines = unindent_body_lines(body_lines);
    let (is_classmethod, is_staticmethod, is_abstractmethod, is_async, options_consumed) =
        extract_method_options(&unindented_lines);

    let body_content: Vec<&str> = unindented_lines[options_consumed..]
        .iter()
        .map(String::as_str)
        .collect();
    let body = parse_blocks(&body_content, adornment_order, diagnostics, default_domain);

    DomainObjectBody::PyMethod {
        signatures,
        is_classmethod: is_classmethod || forced_classmethod,
        is_staticmethod: is_staticmethod || forced_staticmethod,
        is_abstractmethod,
        is_async,
        body,
    }
}

/// Parses a `.. py:class::` body: strips a leading `:final:` flag line off
/// the front before parsing the rest as the docstring body. Any nested
/// domain object directives (e.g. `.. py:method::`) in the body are parsed
/// through the same recursive `parse_blocks` call every other domain object
/// uses — qualifying their cross-reference names by this class is the
/// analyzer/renderer's job, not the parser's.
fn parse_py_class(
    signatures: NonEmptyVector<String>,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
    default_domain: Domain,
) -> DomainObjectBody {
    let unindented_lines = unindent_body_lines(body_lines);
    let (is_final, options_consumed) = extract_class_options(&unindented_lines);

    let body_content: Vec<&str> = unindented_lines[options_consumed..]
        .iter()
        .map(String::as_str)
        .collect();
    let body = parse_blocks(&body_content, adornment_order, diagnostics, default_domain);

    DomainObjectBody::PyClass {
        signatures,
        is_final,
        body,
    }
}

/// Parses a `.. py:exception::` body: strips a leading `:final:` flag line
/// off the front before parsing the rest as the docstring body. Shares
/// `extract_class_options` with `.. py:class::` since both directives have
/// the same option set; only the object type (and thus the produced
/// [`DomainObjectBody`] variant) differs.
fn parse_py_exception(
    signatures: NonEmptyVector<String>,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
    default_domain: Domain,
) -> DomainObjectBody {
    let unindented_lines = unindent_body_lines(body_lines);
    let (is_final, options_consumed) = extract_class_options(&unindented_lines);

    let body_content: Vec<&str> = unindented_lines[options_consumed..]
        .iter()
        .map(String::as_str)
        .collect();
    let body = parse_blocks(&body_content, adornment_order, diagnostics, default_domain);

    DomainObjectBody::PyException {
        signatures,
        is_final,
        body,
    }
}

/// Parses a `.. py:attribute::` body: strips `:type:`/`:value:`/`:canonical:`
/// option lines off the front before parsing the rest as the docstring body.
fn parse_py_attribute(
    signatures: NonEmptyVector<String>,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
    default_domain: Domain,
) -> DomainObjectBody {
    let unindented_lines = unindent_body_lines(body_lines);
    let (type_, value, canonical, options_consumed) = extract_attribute_options(&unindented_lines);

    let body_content: Vec<&str> = unindented_lines[options_consumed..]
        .iter()
        .map(String::as_str)
        .collect();
    let body = parse_blocks(&body_content, adornment_order, diagnostics, default_domain);

    DomainObjectBody::PyAttribute {
        signatures,
        type_,
        value,
        canonical,
        body,
    }
}

/// Parses a `.. c:struct::` body: strips the common object-description flag
/// lines (`:no-index:`, `:no-index-entry:`, `:no-contents-entry:`, and their
/// legacy spellings) off the front before parsing the rest as the docstring
/// body.
fn parse_c_struct(
    signatures: NonEmptyVector<String>,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
    default_domain: Domain,
) -> DomainObjectBody {
    let unindented_lines = unindent_body_lines(body_lines);
    let (no_index, no_index_entry, no_contents_entry, options_consumed) =
        extract_common_object_description_options(&unindented_lines);

    let body_content: Vec<&str> = unindented_lines[options_consumed..]
        .iter()
        .map(String::as_str)
        .collect();
    let body = parse_blocks(&body_content, adornment_order, diagnostics, default_domain);

    DomainObjectBody::CStruct {
        signatures,
        no_index,
        no_index_entry,
        no_contents_entry,
        body,
    }
}

/// Parses a `.. c:union::` body — identical shape to [`parse_c_struct`],
/// just producing the other container variant.
fn parse_c_union(
    signatures: NonEmptyVector<String>,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
    default_domain: Domain,
) -> DomainObjectBody {
    let unindented_lines = unindent_body_lines(body_lines);
    let (no_index, no_index_entry, no_contents_entry, options_consumed) =
        extract_common_object_description_options(&unindented_lines);

    let body_content: Vec<&str> = unindented_lines[options_consumed..]
        .iter()
        .map(String::as_str)
        .collect();
    let body = parse_blocks(&body_content, adornment_order, diagnostics, default_domain);

    DomainObjectBody::CUnion {
        signatures,
        no_index,
        no_index_entry,
        no_contents_entry,
        body,
    }
}

/// Parses a `.. c:member::`/`.. c:var::` body — same common flags as
/// [`parse_c_struct`]/[`parse_c_union`]; the real spec has no member-specific
/// options beyond them (the type is embedded in the signature itself, e.g.
/// `int count`, unlike `py:data`/`py:attribute`'s separate `:type:` option).
fn parse_c_member(
    signatures: NonEmptyVector<String>,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
    default_domain: Domain,
) -> DomainObjectBody {
    let unindented_lines = unindent_body_lines(body_lines);
    let (no_index, no_index_entry, no_contents_entry, options_consumed) =
        extract_common_object_description_options(&unindented_lines);

    let body_content: Vec<&str> = unindented_lines[options_consumed..]
        .iter()
        .map(String::as_str)
        .collect();
    let body = parse_blocks(&body_content, adornment_order, diagnostics, default_domain);

    DomainObjectBody::CMember {
        signatures,
        no_index,
        no_index_entry,
        no_contents_entry,
        body,
    }
}

/// Parses a `.. c:type::` body — same common flags and shape as
/// [`parse_c_struct`]/[`parse_c_union`]; real Sphinx documents no options
/// specific to `c:type` beyond the shared object-description ones. Unlike
/// `c:struct`/`c:union`, `c:type` has no members of its own, but its body is
/// still parsed as full block content (rather than left opaque) so that
/// nested definitions (e.g. enum-style `.. c:macro::` constants) are indexed
/// instead of silently dropped — see `known_bugs.md`'s former `c:type` entry.
fn parse_c_type(
    signatures: NonEmptyVector<String>,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
    default_domain: Domain,
) -> DomainObjectBody {
    let unindented_lines = unindent_body_lines(body_lines);
    let (no_index, no_index_entry, no_contents_entry, options_consumed) =
        extract_common_object_description_options(&unindented_lines);

    let body_content: Vec<&str> = unindented_lines[options_consumed..]
        .iter()
        .map(String::as_str)
        .collect();
    let body = parse_blocks(&body_content, adornment_order, diagnostics, default_domain);

    DomainObjectBody::CType {
        signatures,
        no_index,
        no_index_entry,
        no_contents_entry,
        body,
    }
}

/// Extracts the object-description flag options common across domains
/// (`:no-index:`, `:no-index-entry:`, `:no-contents-entry:`, plus their
/// legacy pre-Sphinx-7 spellings `:noindex:`/`:noindexentry:`/
/// `:nocontentsentry:`) from the leading lines of a domain object's body.
/// Currently only wired up for `c:struct`/`c:union`/`c:member`, the first
/// object types in this codebase to model them.
///
/// Scans from the start and stops at the first line that isn't one of these
/// recognized flags (e.g. a blank line or the start of the docstring body),
/// returning how many leading lines were consumed as options so the caller
/// can slice them off before parsing the remaining body content.
fn extract_common_object_description_options(lines: &[String]) -> (bool, bool, bool, usize) {
    let mut no_index = false;
    let mut no_index_entry = false;
    let mut no_contents_entry = false;
    let mut consumed = 0;

    for line in lines {
        match line.trim() {
            ":no-index:" | ":noindex:" => no_index = true,
            ":no-index-entry:" | ":noindexentry:" => no_index_entry = true,
            ":no-contents-entry:" | ":nocontentsentry:" => no_contents_entry = true,
            _ => break,
        }
        consumed += 1;
    }

    (no_index, no_index_entry, no_contents_entry, consumed)
}

/// Extracts `.. py:module::`-specific options (`:platform:`, `:synopsis:`,
/// `:deprecated:`) from the leading lines of a domain object's body.
///
/// Scans from the start and stops at the first line that isn't one of these
/// recognized options (e.g. a blank line or the start of the docstring body),
/// returning how many leading lines were consumed as options so the caller
/// can slice them off before parsing the remaining body content.
fn extract_module_options(lines: &[String]) -> (Option<String>, Option<String>, bool, usize) {
    let mut platform = None;
    let mut synopsis = None;
    let mut deprecated = false;
    let mut consumed = 0;

    for line in lines {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(":platform:") {
            platform = Some(rest.trim().to_string());
        } else if let Some(rest) = trimmed.strip_prefix(":synopsis:") {
            synopsis = Some(rest.trim().to_string());
        } else if trimmed == ":deprecated:" {
            deprecated = true;
        } else {
            break;
        }
        consumed += 1;
    }

    (platform, synopsis, deprecated, consumed)
}

/// Extracts `.. py:data::`-specific options (`:type:`, `:value:`) from the
/// leading lines of a domain object's body.
///
/// Scans from the start and stops at the first line that isn't one of these
/// recognized options (e.g. a blank line or the start of the docstring body),
/// returning how many leading lines were consumed as options so the caller
/// can slice them off before parsing the remaining body content.
fn extract_data_options(lines: &[String]) -> (Option<String>, Option<String>, usize) {
    let mut type_ = None;
    let mut value = None;
    let mut consumed = 0;

    for line in lines {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(":type:") {
            type_ = Some(rest.trim().to_string());
        } else if let Some(rest) = trimmed.strip_prefix(":value:") {
            value = Some(rest.trim().to_string());
        } else {
            break;
        }
        consumed += 1;
    }

    (type_, value, consumed)
}

/// Extracts `.. py:method::`-specific flag options (`:classmethod:`,
/// `:staticmethod:`, `:abstractmethod:`, `:async:`) from the leading lines of
/// a domain object's body.
///
/// Scans from the start and stops at the first line that isn't one of these
/// recognized flags (e.g. a blank line or the start of the docstring body),
/// returning how many leading lines were consumed as options so the caller
/// can slice them off before parsing the remaining body content.
fn extract_method_options(lines: &[String]) -> (bool, bool, bool, bool, usize) {
    let mut is_classmethod = false;
    let mut is_staticmethod = false;
    let mut is_abstractmethod = false;
    let mut is_async = false;
    let mut consumed = 0;

    for line in lines {
        match line.trim() {
            ":classmethod:" => is_classmethod = true,
            ":staticmethod:" => is_staticmethod = true,
            ":abstractmethod:" => is_abstractmethod = true,
            ":async:" => is_async = true,
            _ => break,
        }
        consumed += 1;
    }

    (
        is_classmethod,
        is_staticmethod,
        is_abstractmethod,
        is_async,
        consumed,
    )
}

/// Extracts `.. py:class::`-specific flag options (`:final:`) from the
/// leading lines of a domain object's body.
///
/// Scans from the start and stops at the first line that isn't `:final:`
/// (e.g. a blank line, a nested directive, or the start of the docstring
/// body), returning how many leading lines were consumed as options so the
/// caller can slice them off before parsing the remaining body content.
fn extract_class_options(lines: &[String]) -> (bool, usize) {
    let mut is_final = false;
    let mut consumed = 0;

    for line in lines {
        match line.trim() {
            ":final:" => is_final = true,
            _ => break,
        }
        consumed += 1;
    }

    (is_final, consumed)
}

/// Extracts `.. py:attribute::`-specific options (`:type:`, `:value:`,
/// `:canonical:`) from the leading lines of a domain object's body.
///
/// Scans from the start and stops at the first line that isn't one of these
/// recognized options (e.g. a blank line or the start of the docstring body),
/// returning how many leading lines were consumed as options so the caller
/// can slice them off before parsing the remaining body content.
fn extract_attribute_options(
    lines: &[String],
) -> (Option<String>, Option<String>, Option<String>, usize) {
    let mut type_ = None;
    let mut value = None;
    let mut canonical = None;
    let mut consumed = 0;

    for line in lines {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(":type:") {
            type_ = Some(rest.trim().to_string());
        } else if let Some(rest) = trimmed.strip_prefix(":value:") {
            value = Some(rest.trim().to_string());
        } else if let Some(rest) = trimmed.strip_prefix(":canonical:") {
            canonical = Some(rest.trim().to_string());
        } else {
            break;
        }
        consumed += 1;
    }

    (type_, value, canonical, consumed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use rusty_sphinx_ast::Directive;

    #[test]
    fn test_extract_module_options_parses_all_three_options() {
        // Given
        let lines = vec![
            ":platform: Unix, Windows".to_string(),
            ":synopsis: Greeting utilities.".to_string(),
            ":deprecated:".to_string(),
            String::new(),
            "A module of greetings.".to_string(),
        ];

        // When
        let (platform, synopsis, deprecated, consumed) = extract_module_options(&lines);

        // Then
        assert_eq!(platform.as_deref(), Some("Unix, Windows"));
        assert_eq!(synopsis.as_deref(), Some("Greeting utilities."));
        assert!(deprecated);
        assert_eq!(consumed, 3);
    }

    #[test]
    fn test_extract_module_options_stops_at_first_non_option_line() {
        // Given
        let lines = vec![
            ":platform: Unix".to_string(),
            "A module of greetings.".to_string(),
        ];

        // When
        let (platform, synopsis, deprecated, consumed) = extract_module_options(&lines);

        // Then
        assert_eq!(platform.as_deref(), Some("Unix"));
        assert_eq!(synopsis, None);
        assert!(!deprecated);
        assert_eq!(consumed, 1);
    }

    #[test]
    fn test_extract_module_options_returns_defaults_when_no_options_present() {
        // Given
        let lines = vec!["A module of greetings.".to_string()];

        // When
        let (platform, synopsis, deprecated, consumed) = extract_module_options(&lines);

        // Then
        assert_eq!(platform, None);
        assert_eq!(synopsis, None);
        assert!(!deprecated);
        assert_eq!(consumed, 0);
    }

    #[test]
    fn test_extract_data_options_parses_both_options() {
        // Given
        let lines = vec![
            ":type: int".to_string(),
            ":value: 30".to_string(),
            String::new(),
            "The default timeout in seconds.".to_string(),
        ];

        // When
        let (type_, value, consumed) = extract_data_options(&lines);

        // Then
        assert_eq!(type_.as_deref(), Some("int"));
        assert_eq!(value.as_deref(), Some("30"));
        assert_eq!(consumed, 2);
    }

    #[test]
    fn test_extract_data_options_stops_at_first_non_option_line() {
        // Given
        let lines = vec![
            ":type: int".to_string(),
            "The default timeout in seconds.".to_string(),
        ];

        // When
        let (type_, value, consumed) = extract_data_options(&lines);

        // Then
        assert_eq!(type_.as_deref(), Some("int"));
        assert_eq!(value, None);
        assert_eq!(consumed, 1);
    }

    #[test]
    fn test_extract_data_options_returns_defaults_when_no_options_present() {
        // Given
        let lines = vec!["The default timeout in seconds.".to_string()];

        // When
        let (type_, value, consumed) = extract_data_options(&lines);

        // Then
        assert_eq!(type_, None);
        assert_eq!(value, None);
        assert_eq!(consumed, 0);
    }

    #[test]
    fn test_extract_method_options_parses_all_four_flags() {
        // Given
        let lines = vec![
            ":classmethod:".to_string(),
            ":staticmethod:".to_string(),
            ":abstractmethod:".to_string(),
            ":async:".to_string(),
            String::new(),
            "Does the thing.".to_string(),
        ];

        // When
        let (is_classmethod, is_staticmethod, is_abstractmethod, is_async, consumed) =
            extract_method_options(&lines);

        // Then
        assert!(is_classmethod);
        assert!(is_staticmethod);
        assert!(is_abstractmethod);
        assert!(is_async);
        assert_eq!(consumed, 4);
    }

    #[test]
    fn test_extract_method_options_stops_at_first_non_option_line() {
        // Given
        let lines = vec![":classmethod:".to_string(), "Does the thing.".to_string()];

        // When
        let (is_classmethod, is_staticmethod, is_abstractmethod, is_async, consumed) =
            extract_method_options(&lines);

        // Then
        assert!(is_classmethod);
        assert!(!is_staticmethod);
        assert!(!is_abstractmethod);
        assert!(!is_async);
        assert_eq!(consumed, 1);
    }

    #[test]
    fn test_extract_method_options_returns_defaults_when_no_options_present() {
        // Given
        let lines = vec!["Does the thing.".to_string()];

        // When
        let (is_classmethod, is_staticmethod, is_abstractmethod, is_async, consumed) =
            extract_method_options(&lines);

        // Then
        assert!(!is_classmethod);
        assert!(!is_staticmethod);
        assert!(!is_abstractmethod);
        assert!(!is_async);
        assert_eq!(consumed, 0);
    }

    #[test]
    fn test_extract_attribute_options_parses_all_three_options() {
        // Given
        let lines = vec![
            ":type: str".to_string(),
            ":value: \"anonymous\"".to_string(),
            ":canonical: mymodule.MyClass.name".to_string(),
            String::new(),
            "The greeter's name.".to_string(),
        ];

        // When
        let (type_, value, canonical, consumed) = extract_attribute_options(&lines);

        // Then
        assert_eq!(type_.as_deref(), Some("str"));
        assert_eq!(value.as_deref(), Some("\"anonymous\""));
        assert_eq!(canonical.as_deref(), Some("mymodule.MyClass.name"));
        assert_eq!(consumed, 3);
    }

    #[test]
    fn test_extract_attribute_options_stops_at_first_non_option_line() {
        // Given
        let lines = vec![":type: str".to_string(), "The greeter's name.".to_string()];

        // When
        let (type_, value, canonical, consumed) = extract_attribute_options(&lines);

        // Then
        assert_eq!(type_.as_deref(), Some("str"));
        assert_eq!(value, None);
        assert_eq!(canonical, None);
        assert_eq!(consumed, 1);
    }

    #[test]
    fn test_extract_attribute_options_returns_defaults_when_no_options_present() {
        // Given
        let lines = vec!["The greeter's name.".to_string()];

        // When
        let (type_, value, canonical, consumed) = extract_attribute_options(&lines);

        // Then
        assert_eq!(type_, None);
        assert_eq!(value, None);
        assert_eq!(canonical, None);
        assert_eq!(consumed, 0);
    }

    #[test]
    fn test_extract_attribute_options_parses_options_in_any_order() {
        // Given
        let lines = vec![
            ":canonical: mymodule.MyClass.name".to_string(),
            ":value: \"anonymous\"".to_string(),
            ":type: str".to_string(),
        ];

        // When
        let (type_, value, canonical, consumed) = extract_attribute_options(&lines);

        // Then
        assert_eq!(type_.as_deref(), Some("str"));
        assert_eq!(value.as_deref(), Some("\"anonymous\""));
        assert_eq!(canonical.as_deref(), Some("mymodule.MyClass.name"));
        assert_eq!(consumed, 3);
    }

    #[test]
    fn test_parse_creates_py_method_domain_object() {
        // Given
        let input = ".. py:method:: greet(self, name)\n\n   Greets the given name.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyMethod {
            signatures,
            is_classmethod,
            is_staticmethod,
            is_abstractmethod,
            is_async,
            body,
        })) = &doc.nodes[0]
        {
            assert_eq!(signatures.as_slice(), ["greet(self, name)"]);
            assert!(!is_classmethod);
            assert!(!is_staticmethod);
            assert!(!is_abstractmethod);
            assert!(!is_async);
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected PyMethod, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_py_method_with_classmethod_and_abstractmethod_options() {
        // Given
        let input = ".. py:method:: create(cls)\n   :classmethod:\n   :abstractmethod:\n\n   Creates an instance.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyMethod {
            is_classmethod,
            is_staticmethod,
            is_abstractmethod,
            is_async,
            body,
            ..
        })) = &doc.nodes[0]
        {
            assert!(*is_classmethod);
            assert!(!is_staticmethod);
            assert!(*is_abstractmethod);
            assert!(!is_async);
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected PyMethod, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_py_method_options_in_any_order_with_no_body() {
        // Given — async before staticmethod, and no docstring body
        let input = ".. py:method:: run()\n   :async:\n   :staticmethod:";

        // When
        let doc = parse("test.rst", input);

        // Then
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyMethod {
            is_classmethod,
            is_staticmethod,
            is_abstractmethod,
            is_async,
            body,
            ..
        })) = &doc.nodes[0]
        {
            assert!(!is_classmethod);
            assert!(*is_staticmethod);
            assert!(!is_abstractmethod);
            assert!(*is_async);
            assert!(body.is_empty());
        } else {
            panic!("Expected PyMethod, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_classmethod_alias_directive_forces_classmethod_flag() {
        // Given — the legacy `.. classmethod::` directive spelling (the shape
        // CPython's `zoneinfo` docs use for `ZoneInfo.clear_cache`), a bare
        // name under the default `py` domain
        let input =
            ".. classmethod:: ZoneInfo.clear_cache(*, only_keys=None)\n\n   Clear the cache.";

        // When
        let doc = parse("test.rst", input);

        // Then — it parses as a `py:method` with `is_classmethod` forced on
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyMethod {
            signatures,
            is_classmethod,
            is_staticmethod,
            is_abstractmethod,
            is_async,
            body,
        })) = &doc.nodes[0]
        {
            assert_eq!(
                signatures.as_slice(),
                ["ZoneInfo.clear_cache(*, only_keys=None)"]
            );
            assert!(*is_classmethod);
            assert!(!is_staticmethod);
            assert!(!is_abstractmethod);
            assert!(!is_async);
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected PyMethod, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_staticmethod_alias_directive_forces_staticmethod_flag() {
        // Given — the legacy `.. staticmethod::` directive spelling
        let input = ".. staticmethod:: Greeter.default_name()\n\n   The default name.";

        // When
        let doc = parse("test.rst", input);

        // Then — it parses as a `py:method` with `is_staticmethod` forced on
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyMethod {
            signatures,
            is_classmethod,
            is_staticmethod,
            is_abstractmethod,
            is_async,
            body,
        })) = &doc.nodes[0]
        {
            assert_eq!(signatures.as_slice(), ["Greeter.default_name()"]);
            assert!(!is_classmethod);
            assert!(*is_staticmethod);
            assert!(!is_abstractmethod);
            assert!(!is_async);
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected PyMethod, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_classmethod_alias_composes_with_explicit_abstractmethod_option() {
        // Given — the alias directive name forces `classmethod`, and an
        // explicit `:abstractmethod:` option line in the body is still parsed
        // and OR-ed in on top of it
        let input = ".. classmethod:: validate(cls, name)\n   :abstractmethod:\n\n   Validate.";

        // When
        let doc = parse("test.rst", input);

        // Then — both flags are set
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyMethod {
            is_classmethod,
            is_staticmethod,
            is_abstractmethod,
            is_async,
            ..
        })) = &doc.nodes[0]
        {
            assert!(*is_classmethod);
            assert!(!is_staticmethod);
            assert!(*is_abstractmethod);
            assert!(!is_async);
        } else {
            panic!("Expected PyMethod, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_extract_class_options_parses_final_flag() {
        // Given
        let lines = vec![
            ":final:".to_string(),
            String::new(),
            "A greeter.".to_string(),
        ];

        // When
        let (is_final, consumed) = extract_class_options(&lines);

        // Then
        assert!(is_final);
        assert_eq!(consumed, 1);
    }

    #[test]
    fn test_extract_class_options_returns_defaults_when_no_options_present() {
        // Given
        let lines = vec!["A greeter.".to_string()];

        // When
        let (is_final, consumed) = extract_class_options(&lines);

        // Then
        assert!(!is_final);
        assert_eq!(consumed, 0);
    }

    #[test]
    fn test_extract_common_object_description_options_parses_hyphenated_spellings() {
        // Given
        let lines = vec![
            ":no-index:".to_string(),
            ":no-index-entry:".to_string(),
            ":no-contents-entry:".to_string(),
            String::new(),
            "A struct.".to_string(),
        ];

        // When
        let (no_index, no_index_entry, no_contents_entry, consumed) =
            extract_common_object_description_options(&lines);

        // Then
        assert!(no_index);
        assert!(no_index_entry);
        assert!(no_contents_entry);
        assert_eq!(consumed, 3);
    }

    #[test]
    fn test_extract_common_object_description_options_parses_legacy_spellings() {
        // Given
        let lines = vec![
            ":noindex:".to_string(),
            ":noindexentry:".to_string(),
            ":nocontentsentry:".to_string(),
        ];

        // When
        let (no_index, no_index_entry, no_contents_entry, consumed) =
            extract_common_object_description_options(&lines);

        // Then
        assert!(no_index);
        assert!(no_index_entry);
        assert!(no_contents_entry);
        assert_eq!(consumed, 3);
    }

    #[test]
    fn test_extract_common_object_description_options_stops_at_first_non_option_line() {
        // Given
        let lines = vec![
            ":no-index:".to_string(),
            "A struct.".to_string(),
            ":no-index-entry:".to_string(),
        ];

        // When
        let (no_index, no_index_entry, _, consumed) =
            extract_common_object_description_options(&lines);

        // Then
        assert!(no_index);
        assert!(!no_index_entry);
        assert_eq!(consumed, 1);
    }

    #[test]
    fn test_extract_common_object_description_options_returns_defaults_when_no_options_present() {
        // Given
        let lines = vec!["A struct.".to_string()];

        // When
        let (no_index, no_index_entry, no_contents_entry, consumed) =
            extract_common_object_description_options(&lines);

        // Then
        assert!(!no_index);
        assert!(!no_index_entry);
        assert!(!no_contents_entry);
        assert_eq!(consumed, 0);
    }

    #[test]
    fn test_parse_creates_py_class_domain_object() {
        // Given
        let input = ".. py:class:: Greeter\n\n   A greeter.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyClass {
            signatures,
            is_final,
            body,
        })) = &doc.nodes[0]
        {
            assert_eq!(signatures.as_slice(), ["Greeter"]);
            assert!(!is_final);
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected PyClass, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_py_class_with_final_option_and_base_class_signature() {
        // Given
        let input = ".. py:class:: Greeter(Base)\n   :final:\n\n   A greeter.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyClass {
            signatures,
            is_final,
            body,
        })) = &doc.nodes[0]
        {
            assert_eq!(signatures.as_slice(), ["Greeter(Base)"]);
            assert!(*is_final);
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected PyClass, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_py_class_with_no_options_and_no_body() {
        // Given
        let input = ".. py:class:: Greeter";

        // When
        let doc = parse("test.rst", input);

        // Then
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyClass {
            is_final,
            body,
            ..
        })) = &doc.nodes[0]
        {
            assert!(!is_final);
            assert!(body.is_empty());
        } else {
            panic!("Expected PyClass, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_py_class_with_nested_py_method() {
        // Given — a `py:method` nested inside a `py:class` body, indented
        // like any other nested directive (e.g. `py:data` inside a table).
        let input = ".. py:class:: Greeter\n\n   A greeter.\n\n   .. py:method:: greet(self, name)\n\n      Greets the given name.";

        // When
        let doc = parse("test.rst", input);

        // Then
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyClass {
            body, ..
        })) = &doc.nodes[0]
        {
            assert!(body.iter().any(|node| matches!(
                node,
                Node::Directive(Directive::DomainObject(DomainObjectBody::PyMethod { .. }))
            )));
        } else {
            panic!("Expected PyClass, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_creates_py_exception_domain_object() {
        // Given
        let input = ".. py:exception:: GreeterError\n\n   Raised when greeting fails.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyException {
            signatures,
            is_final,
            body,
        })) = &doc.nodes[0]
        {
            assert_eq!(signatures.as_slice(), ["GreeterError"]);
            assert!(!is_final);
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected PyException, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_py_exception_with_final_option_and_base_class_signature() {
        // Given
        let input = ".. py:exception:: InvalidNameError(GreeterError)\n   :final:\n\n   Raised for an invalid name.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyException {
            signatures,
            is_final,
            body,
        })) = &doc.nodes[0]
        {
            assert_eq!(signatures.as_slice(), ["InvalidNameError(GreeterError)"]);
            assert!(*is_final);
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected PyException, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_py_exception_with_no_options_and_no_body() {
        // Given
        let input = ".. py:exception:: GreeterError";

        // When
        let doc = parse("test.rst", input);

        // Then
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyException {
            is_final,
            body,
            ..
        })) = &doc.nodes[0]
        {
            assert!(!is_final);
            assert!(body.is_empty());
        } else {
            panic!("Expected PyException, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_py_exception_with_nested_py_method() {
        // Given — a `py:method` nested inside a `py:exception` body, indented
        // like any other nested directive.
        let input = ".. py:exception:: GreeterError\n\n   Raised when greeting fails.\n\n   .. py:method:: reason(self)\n\n      Returns the failure reason.";

        // When
        let doc = parse("test.rst", input);

        // Then
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyException {
            body,
            ..
        })) = &doc.nodes[0]
        {
            assert!(body.iter().any(|node| matches!(
                node,
                Node::Directive(Directive::DomainObject(DomainObjectBody::PyMethod { .. }))
            )));
        } else {
            panic!("Expected PyException, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_bare_exception_directive_resolves_via_default_domain() {
        // Given
        let input = ".. exception:: GreeterError\n\n   Raised when greeting fails.";

        // When
        let doc = crate::parse_with_domain("test.rst", input, Domain::Py);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        assert!(matches!(
            &doc.nodes[0],
            Node::Directive(Directive::DomainObject(
                DomainObjectBody::PyException { .. }
            ))
        ));
    }

    #[test]
    fn test_parse_bare_class_directive_resolves_via_default_domain() {
        // Given
        let input = ".. class:: Greeter\n\n   A greeter.";

        // When
        let doc = crate::parse_with_domain("test.rst", input, Domain::Py);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        assert!(matches!(
            &doc.nodes[0],
            Node::Directive(Directive::DomainObject(DomainObjectBody::PyClass { .. }))
        ));
    }

    #[test]
    fn test_parse_domain_object_with_paragraph_body() {
        // Given
        let object_type = DirectiveObjectType::PyFunction;
        let signature = "greet(name)".to_string();
        let body_lines = vec!["   Greets the given name."];
        let mut adornment_order = Vec::new();
        let mut diagnostics = Vec::new();

        // When
        let domain_object = parse_domain_object(
            object_type,
            signature.clone(),
            Vec::new(),
            &body_lines,
            &mut adornment_order,
            &mut diagnostics,
            Domain::Py,
        );

        // Then
        if let DomainObjectBody::PyFunction { signatures, body } = domain_object {
            assert_eq!(signatures.as_slice(), [signature]);
            assert_eq!(body.len(), 1);
            assert!(matches!(body[0], Node::Paragraph(_)));
        } else {
            panic!("Expected PyFunction, got {domain_object:?}");
        }
    }

    #[test]
    fn test_parse_domain_object_with_bullet_list_body() {
        // Given
        let object_type = DirectiveObjectType::CFunction;
        let signature = "int add(int a, int b)".to_string();
        let body_lines = vec!["   * Adds two numbers.", "   * Returns their sum."];
        let mut adornment_order = Vec::new();
        let mut diagnostics = Vec::new();

        // When
        let domain_object = parse_domain_object(
            object_type,
            signature,
            Vec::new(),
            &body_lines,
            &mut adornment_order,
            &mut diagnostics,
            Domain::C,
        );

        // Then
        if let DomainObjectBody::CFunction { body, .. } = domain_object {
            assert_eq!(body.len(), 1);
            assert!(matches!(body[0], Node::BulletList { .. }));
        } else {
            panic!("Expected CFunction, got {domain_object:?}");
        }
    }

    #[test]
    fn test_parse_domain_object_with_empty_body() {
        // Given
        let object_type = DirectiveObjectType::PyFunction;
        let signature = "greet(name)".to_string();
        let body_lines: Vec<&str> = vec![];
        let mut adornment_order = Vec::new();
        let mut diagnostics = Vec::new();

        // When
        let domain_object = parse_domain_object(
            object_type,
            signature,
            Vec::new(),
            &body_lines,
            &mut adornment_order,
            &mut diagnostics,
            Domain::Py,
        );

        // Then
        if let DomainObjectBody::PyFunction { body, .. } = domain_object {
            assert!(body.is_empty());
        } else {
            panic!("Expected PyFunction, got {domain_object:?}");
        }
    }

    #[test]
    fn test_parse_domain_object_strips_common_indentation() {
        // Given
        let object_type = DirectiveObjectType::PyFunction;
        let signature = "greet(name)".to_string();
        let body_lines = vec!["     Indented more than needed."];
        let mut adornment_order = Vec::new();
        let mut diagnostics = Vec::new();

        // When
        let domain_object = parse_domain_object(
            object_type,
            signature,
            Vec::new(),
            &body_lines,
            &mut adornment_order,
            &mut diagnostics,
            Domain::Py,
        );

        // Then
        if let DomainObjectBody::PyFunction { body, .. } = domain_object {
            if let Node::Paragraph(inlines) = &body[0] {
                assert_eq!(
                    inlines[0],
                    rusty_sphinx_ast::InlineNode::Text("Indented more than needed.".to_string())
                );
            } else {
                panic!("Expected Paragraph, got {:?}", body[0]);
            }
        } else {
            panic!("Expected PyFunction, got {domain_object:?}");
        }
    }

    #[test]
    fn test_parse_domain_object_does_not_panic_on_multi_byte_char_in_a_short_line() {
        // Given a body whose first line has a 3-char indent and a second,
        // less-indented line containing a multi-byte character at the byte
        // offset the old byte-index slicing would have panicked on
        let object_type = DirectiveObjectType::PyFunction;
        let signature = "greet(name)".to_string();
        let body_lines = vec!["   First line normal indent.", "  éfoo"];
        let mut adornment_order = Vec::new();
        let mut diagnostics = Vec::new();

        // When parsing the domain object body
        let domain_object = parse_domain_object(
            object_type,
            signature,
            Vec::new(),
            &body_lines,
            &mut adornment_order,
            &mut diagnostics,
            Domain::Py,
        );

        // Then it does not panic
        if let DomainObjectBody::PyFunction { .. } = domain_object {
            // no-op: reaching here means parsing succeeded without panicking
        } else {
            panic!("Expected PyFunction, got {domain_object:?}");
        }
    }

    #[test]
    fn test_parse_creates_py_function_domain_object() {
        // Given
        let input = ".. py:function:: greet(name)\n\n   Greets the given name.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyFunction {
            signatures,
            body,
        })) = &doc.nodes[0]
        {
            assert_eq!(signatures.as_slice(), ["greet(name)"]);
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected PyFunction, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_creates_py_module_domain_object() {
        // Given
        let input = ".. py:module:: greetings\n\n   A module of greetings.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyModule {
            name,
            platform,
            synopsis,
            deprecated,
            body,
        })) = &doc.nodes[0]
        {
            assert_eq!(name, "greetings");
            assert_eq!(platform, &None);
            assert_eq!(synopsis, &None);
            assert!(!deprecated);
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected PyModule, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_py_module_with_platform_synopsis_and_deprecated_options() {
        // Given
        let input = ".. py:module:: greetings\n   :platform: Unix, Windows\n   :synopsis: Greeting utilities.\n   :deprecated:\n\n   A module of greetings.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyModule {
            platform,
            synopsis,
            deprecated,
            body,
            ..
        })) = &doc.nodes[0]
        {
            assert_eq!(platform.as_deref(), Some("Unix, Windows"));
            assert_eq!(synopsis.as_deref(), Some("Greeting utilities."));
            assert!(*deprecated);
            assert_eq!(body.len(), 1);
            assert!(matches!(body[0], Node::Paragraph(_)));
        } else {
            panic!("Expected PyModule, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_py_module_options_in_any_order_with_no_body() {
        // Given — synopsis and deprecated before platform, and no docstring body
        let input = ".. py:module:: greetings\n   :synopsis: Greeting utilities.\n   :deprecated:\n   :platform: Unix";

        // When
        let doc = parse("test.rst", input);

        // Then
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyModule {
            platform,
            synopsis,
            deprecated,
            body,
            ..
        })) = &doc.nodes[0]
        {
            assert_eq!(platform.as_deref(), Some("Unix"));
            assert_eq!(synopsis.as_deref(), Some("Greeting utilities."));
            assert!(*deprecated);
            assert!(body.is_empty());
        } else {
            panic!("Expected PyModule, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_creates_py_data_domain_object() {
        // Given
        let input = ".. py:data:: DEFAULT_TIMEOUT\n\n   The default timeout in seconds.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyData {
            signatures,
            type_,
            value,
            body,
        })) = &doc.nodes[0]
        {
            assert_eq!(signatures.as_slice(), ["DEFAULT_TIMEOUT"]);
            assert_eq!(type_, &None);
            assert_eq!(value, &None);
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected PyData, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_py_data_with_type_and_value_options() {
        // Given
        let input = ".. py:data:: DEFAULT_TIMEOUT\n   :type: int\n   :value: 30\n\n   The default timeout in seconds.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyData {
            type_,
            value,
            body,
            ..
        })) = &doc.nodes[0]
        {
            assert_eq!(type_.as_deref(), Some("int"));
            assert_eq!(value.as_deref(), Some("30"));
            assert_eq!(body.len(), 1);
            assert!(matches!(body[0], Node::Paragraph(_)));
        } else {
            panic!("Expected PyData, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_py_data_collects_every_argument_line_as_a_signature() {
        // Given — the shape `library/socket.rst` uses to declare three
        // aliases for one documented object, which real Sphinx indexes as
        // three separate targets sharing one docstring.
        let input = ".. py:data:: AF_UNIX\n             AF_INET\n             AF_INET6\n\n   The address families.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyData {
            signatures,
            body,
            ..
        })) = &doc.nodes[0]
        {
            assert_eq!(signatures.as_slice(), ["AF_UNIX", "AF_INET", "AF_INET6"]);
            // The continuation lines must not leak into the docstring.
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected PyData, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_py_data_collects_signatures_before_option_lines() {
        // Given — continuation lines come first, then the option block; both
        // have to be recognized.
        let input = ".. py:data:: A\n             ASCII\n   :type: int\n\n   The ASCII flag.";

        // When
        let doc = parse("test.rst", input);

        // Then
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyData {
            signatures,
            type_,
            body,
            ..
        })) = &doc.nodes[0]
        {
            assert_eq!(signatures.as_slice(), ["A", "ASCII"]);
            assert_eq!(type_.as_deref(), Some("int"));
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected PyData, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_py_function_collects_every_argument_line_as_a_signature() {
        // Given — multi-signature declarations are not limited to bare names;
        // each continuation line may be a full signature.
        let input = ".. py:function:: spawnl(mode, file, *args)\n                 spawnle(mode, file, *args, env)\n\n   Spawn a process.";

        // When
        let doc = parse("test.rst", input);

        // Then
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyFunction {
            signatures,
            body,
        })) = &doc.nodes[0]
        {
            assert_eq!(
                signatures.as_slice(),
                [
                    "spawnl(mode, file, *args)",
                    "spawnle(mode, file, *args, env)"
                ]
            );
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected PyFunction, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_py_module_treats_a_following_line_as_body_not_a_second_name() {
        // Given — real Sphinx's `module` directive takes exactly one
        // argument, so an immediately following line is body content even
        // though it looks like a continuation.
        let input = ".. py:module:: greetings\n   A module of greetings.";

        // When
        let doc = parse("test.rst", input);

        // Then
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyModule {
            name,
            body,
            ..
        })) = &doc.nodes[0]
        {
            assert_eq!(name, "greetings");
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected PyModule, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_py_data_options_in_any_order_with_no_body() {
        // Given — value before type, and no docstring body
        let input = ".. py:data:: DEFAULT_TIMEOUT\n   :value: 30\n   :type: int";

        // When
        let doc = parse("test.rst", input);

        // Then
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyData {
            type_,
            value,
            body,
            ..
        })) = &doc.nodes[0]
        {
            assert_eq!(type_.as_deref(), Some("int"));
            assert_eq!(value.as_deref(), Some("30"));
            assert!(body.is_empty());
        } else {
            panic!("Expected PyData, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_creates_py_attribute_domain_object() {
        // Given
        let input = ".. py:attribute:: Greeter.name\n\n   The greeter's name.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyAttribute {
            signatures,
            type_,
            value,
            canonical,
            body,
        })) = &doc.nodes[0]
        {
            assert_eq!(signatures.as_slice(), ["Greeter.name"]);
            assert_eq!(type_, &None);
            assert_eq!(value, &None);
            assert_eq!(canonical, &None);
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected PyAttribute, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_py_attribute_with_type_value_and_canonical_options() {
        // Given
        let input = ".. py:attribute:: Greeter.name\n   :type: str\n   :value: \"anonymous\"\n   :canonical: mymodule.MyClass.name\n\n   The greeter's name.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyAttribute {
            type_,
            value,
            canonical,
            body,
            ..
        })) = &doc.nodes[0]
        {
            assert_eq!(type_.as_deref(), Some("str"));
            assert_eq!(value.as_deref(), Some("\"anonymous\""));
            assert_eq!(canonical.as_deref(), Some("mymodule.MyClass.name"));
            assert_eq!(body.len(), 1);
            assert!(matches!(body[0], Node::Paragraph(_)));
        } else {
            panic!("Expected PyAttribute, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_py_function_ignores_module_only_options() {
        // Given — platform/synopsis/deprecated are py:module-only per Sphinx's
        // spec (and, since `PyFunction` has no such fields at all, it's a
        // compile error for a function to carry them) — so on a py:function
        // this text must remain part of the docstring body instead.
        let input = ".. py:function:: greet(name)\n\n   :platform: Unix";

        // When
        let doc = parse("test.rst", input);

        // Then
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyFunction {
            body, ..
        })) = &doc.nodes[0]
        {
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected PyFunction, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_creates_c_function_domain_object() {
        // Given
        let input = ".. c:function:: int add(int a, int b)\n\n   Adds two numbers.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::CFunction {
            signatures,
            ..
        })) = &doc.nodes[0]
        {
            assert_eq!(signatures.as_slice(), ["int add(int a, int b)"]);
        } else {
            panic!("Expected CFunction, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_creates_c_macro_domain_object_bare_name() {
        // Given — an object-like macro, with no parens
        let input = ".. c:macro:: PY_SSIZE_T_MAX\n\n   The maximum value of a Py_ssize_t.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::CMacro {
            signatures,
            body,
        })) = &doc.nodes[0]
        {
            assert_eq!(signatures.as_slice(), ["PY_SSIZE_T_MAX"]);
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected CMacro, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_creates_c_macro_domain_object_function_like() {
        // Given — a function-like macro, with no return type or param types
        let input = ".. c:macro:: MAX(a, b)\n\n   Expands to whichever of a or b is greater.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::CMacro {
            signatures,
            ..
        })) = &doc.nodes[0]
        {
            assert_eq!(signatures.as_slice(), ["MAX(a, b)"]);
        } else {
            panic!("Expected CMacro, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_creates_c_struct_domain_object() {
        // Given
        let input = ".. c:struct:: Data\n\n   A data record.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::CStruct {
            signatures,
            no_index,
            no_index_entry,
            no_contents_entry,
            body,
        })) = &doc.nodes[0]
        {
            assert_eq!(signatures.as_slice(), ["Data"]);
            assert!(!no_index);
            assert!(!no_index_entry);
            assert!(!no_contents_entry);
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected CStruct, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_creates_c_union_domain_object() {
        // Given
        let input = ".. c:union:: Number\n\n   A numeric union.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::CUnion {
            signatures,
            ..
        })) = &doc.nodes[0]
        {
            assert_eq!(signatures.as_slice(), ["Number"]);
        } else {
            panic!("Expected CUnion, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_creates_c_member_domain_object_flat_dotted_signature() {
        // Given — the real CPython-docs shape: no enclosing `.. c:struct::`.
        let input = ".. c:member:: PyObject *PyTypeObject.tp_bases\n\n   The type's base classes.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::CMember {
            signatures,
            ..
        })) = &doc.nodes[0]
        {
            assert_eq!(signatures.as_slice(), ["PyObject *PyTypeObject.tp_bases"]);
        } else {
            panic!("Expected CMember, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_creates_c_member_domain_object_with_no_index_option() {
        // Given
        let input = ".. c:member:: int count\n   :no-index:\n\n   A count.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::CMember {
            no_index,
            ..
        })) = &doc.nodes[0]
        {
            assert!(no_index);
        } else {
            panic!("Expected CMember, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_c_var_directive_creates_c_member_domain_object() {
        // Given — `.. c:var::` is a pure directive-name alias for `c:member`.
        let input = ".. c:var:: int errno\n\n   The last error number.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::CMember {
            signatures,
            ..
        })) = &doc.nodes[0]
        {
            assert_eq!(signatures.as_slice(), ["int errno"]);
        } else {
            panic!("Expected CMember, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_creates_c_type_domain_object_bare_name() {
        // Given
        let input = ".. c:type:: PyMemAllocatorDomain\n\n   Enumeration of allocator domains.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::CType {
            signatures,
            no_index,
            no_index_entry,
            no_contents_entry,
            body,
        })) = &doc.nodes[0]
        {
            assert_eq!(signatures.as_slice(), ["PyMemAllocatorDomain"]);
            assert!(!no_index);
            assert!(!no_index_entry);
            assert!(!no_contents_entry);
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected CType, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_creates_c_type_domain_object_typedef_alias_signature() {
        // Given — real Sphinx's `type name` typedef-alias form.
        let input = ".. c:type:: unsigned long ulong\n\n   An unsigned long alias.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::CType {
            signatures,
            ..
        })) = &doc.nodes[0]
        {
            assert_eq!(signatures.as_slice(), ["unsigned long ulong"]);
        } else {
            panic!("Expected CType, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_creates_c_type_domain_object_with_no_index_option() {
        // Given
        let input = ".. c:type:: Hidden\n   :no-index:\n\n   A hidden type.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::CType {
            no_index, ..
        })) = &doc.nodes[0]
        {
            assert!(no_index);
        } else {
            panic!("Expected CType, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_nested_c_macro_under_c_type_is_recursively_parsed() {
        // Given — the real CPython `c-api/memory.rst` shape (`known_bugs.md`):
        // a `.. c:type::` body nesting `.. c:macro::` constants, previously
        // swallowed as opaque `Directive::Unknown` text.
        let input = ".. c:type:: PyMemAllocatorDomain\n\n   .. c:macro:: PYMEM_DOMAIN_RAW\n\n      The raw domain.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::CType { body, .. })) =
            &doc.nodes[0]
        {
            assert_eq!(body.len(), 1);
            assert!(matches!(
                &body[0],
                Node::Directive(Directive::DomainObject(DomainObjectBody::CMacro { .. }))
            ));
        } else {
            panic!("Expected CType, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_nested_c_member_under_c_struct() {
        // Given — real nesting: the member's signature is bare, relying on
        // the enclosing struct for qualification (an analyzer/renderer-time
        // concern; the parser just needs to nest the node correctly).
        let input = ".. c:struct:: Data\n\n   .. c:member:: int count\n\n      A count.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::CStruct {
            body, ..
        })) = &doc.nodes[0]
        {
            assert_eq!(body.len(), 1);
            assert!(matches!(
                &body[0],
                Node::Directive(Directive::DomainObject(DomainObjectBody::CMember { .. }))
            ));
        } else {
            panic!("Expected CStruct, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_bare_macro_directive_resolves_via_default_domain() {
        // Given
        let input = ".. macro:: MAX(a, b)\n\n   Expands to whichever of a or b is greater.";

        // When
        let doc = crate::parse_with_domain("test.rst", input, Domain::C);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        assert!(matches!(
            &doc.nodes[0],
            Node::Directive(Directive::DomainObject(DomainObjectBody::CMacro { .. }))
        ));
    }

    #[test]
    fn test_parse_bare_function_directive_resolves_via_default_domain() {
        // Given
        let input = ".. function:: greet(name)\n\n   Greets the given name.";

        // When
        let doc = crate::parse_with_domain("test.rst", input, Domain::C);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        assert!(matches!(
            &doc.nodes[0],
            Node::Directive(Directive::DomainObject(DomainObjectBody::CFunction { .. }))
        ));
    }

    #[test]
    fn test_parse_unknown_domain_prefix_falls_through_to_unknown() {
        // Given
        let input = ".. rust:function:: greet(name)\n\n   Body.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        assert!(
            matches!(&doc.nodes[0], Node::Directive(Directive::Unknown { .. })),
            "Expected Unknown directive, got {:?}",
            doc.nodes[0]
        );
    }

    #[test]
    fn test_parse_unknown_object_type_in_known_domain_falls_through_to_unknown() {
        // Given
        let input = ".. py:struct:: Greeter\n\n   Body.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        assert!(
            matches!(&doc.nodes[0], Node::Directive(Directive::Unknown { .. })),
            "Expected Unknown directive, got {:?}",
            doc.nodes[0]
        );
    }
}

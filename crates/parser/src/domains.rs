use super::blocks::parse_blocks;
use super::bullet_list::unindent_body_lines;
use super::headings::Adornment;
use rusty_sphinx_ast::{CObjectType, Domain, DomainObjectBody, Node, ObjectType, PyObjectType};

/// Parses a domain object directive body (e.g. `.. py:function::`,
/// `.. py:module::`, `.. c:function::`) into the matching [`DomainObjectBody`]
/// variant, dispatching on `object_type` since each object type has its own
/// shape (only `py:module` has `platform`/`synopsis`/`deprecated`, for
/// instance).
pub(super) fn parse_domain_object(
    object_type: ObjectType,
    argument: String,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
    default_domain: Domain,
) -> DomainObjectBody {
    match object_type {
        ObjectType::Py(PyObjectType::Function) => DomainObjectBody::PyFunction {
            signature: argument,
            body: parse_body(body_lines, adornment_order, diagnostics, default_domain),
        },
        ObjectType::C(CObjectType::Function) => DomainObjectBody::CFunction {
            signature: argument,
            body: parse_body(body_lines, adornment_order, diagnostics, default_domain),
        },
        ObjectType::Py(PyObjectType::Module) => parse_py_module(
            argument,
            body_lines,
            adornment_order,
            diagnostics,
            default_domain,
        ),
        ObjectType::Py(PyObjectType::Data) => parse_py_data(
            argument,
            body_lines,
            adornment_order,
            diagnostics,
            default_domain,
        ),
        ObjectType::Py(PyObjectType::Method) => parse_py_method(
            argument,
            body_lines,
            adornment_order,
            diagnostics,
            default_domain,
        ),
        ObjectType::Py(PyObjectType::Class) => parse_py_class(
            argument,
            body_lines,
            adornment_order,
            diagnostics,
            default_domain,
        ),
        ObjectType::Py(PyObjectType::Attribute) => parse_py_attribute(
            argument,
            body_lines,
            adornment_order,
            diagnostics,
            default_domain,
        ),
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
    name: String,
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
        name,
        type_,
        value,
        body,
    }
}

/// Parses a `.. py:method::` body: strips `:classmethod:`/`:staticmethod:`/
/// `:abstractmethod:`/`:async:` flag lines off the front before parsing the
/// rest as the docstring body.
fn parse_py_method(
    signature: String,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
    default_domain: Domain,
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
        signature,
        is_classmethod,
        is_staticmethod,
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
    signature: String,
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
        signature,
        is_final,
        body,
    }
}

/// Parses a `.. py:attribute::` body: strips `:type:`/`:value:`/`:canonical:`
/// option lines off the front before parsing the rest as the docstring body.
fn parse_py_attribute(
    name: String,
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
        name,
        type_,
        value,
        canonical,
        body,
    }
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
            signature,
            is_classmethod,
            is_staticmethod,
            is_abstractmethod,
            is_async,
            body,
        })) = &doc.nodes[0]
        {
            assert_eq!(signature, "greet(self, name)");
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
    fn test_parse_creates_py_class_domain_object() {
        // Given
        let input = ".. py:class:: Greeter\n\n   A greeter.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyClass {
            signature,
            is_final,
            body,
        })) = &doc.nodes[0]
        {
            assert_eq!(signature, "Greeter");
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
            signature,
            is_final,
            body,
        })) = &doc.nodes[0]
        {
            assert_eq!(signature, "Greeter(Base)");
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
        let object_type = ObjectType::Py(PyObjectType::Function);
        let signature = "greet(name)".to_string();
        let body_lines = vec!["   Greets the given name."];
        let mut adornment_order = Vec::new();
        let mut diagnostics = Vec::new();

        // When
        let domain_object = parse_domain_object(
            object_type,
            signature.clone(),
            &body_lines,
            &mut adornment_order,
            &mut diagnostics,
            Domain::Py,
        );

        // Then
        if let DomainObjectBody::PyFunction {
            signature: sig,
            body,
        } = domain_object
        {
            assert_eq!(sig, signature);
            assert_eq!(body.len(), 1);
            assert!(matches!(body[0], Node::Paragraph(_)));
        } else {
            panic!("Expected PyFunction, got {domain_object:?}");
        }
    }

    #[test]
    fn test_parse_domain_object_with_bullet_list_body() {
        // Given
        let object_type = ObjectType::C(CObjectType::Function);
        let signature = "int add(int a, int b)".to_string();
        let body_lines = vec!["   * Adds two numbers.", "   * Returns their sum."];
        let mut adornment_order = Vec::new();
        let mut diagnostics = Vec::new();

        // When
        let domain_object = parse_domain_object(
            object_type,
            signature,
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
        let object_type = ObjectType::Py(PyObjectType::Function);
        let signature = "greet(name)".to_string();
        let body_lines: Vec<&str> = vec![];
        let mut adornment_order = Vec::new();
        let mut diagnostics = Vec::new();

        // When
        let domain_object = parse_domain_object(
            object_type,
            signature,
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
        let object_type = ObjectType::Py(PyObjectType::Function);
        let signature = "greet(name)".to_string();
        let body_lines = vec!["     Indented more than needed."];
        let mut adornment_order = Vec::new();
        let mut diagnostics = Vec::new();

        // When
        let domain_object = parse_domain_object(
            object_type,
            signature,
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
        let object_type = ObjectType::Py(PyObjectType::Function);
        let signature = "greet(name)".to_string();
        let body_lines = vec!["   First line normal indent.", "  éfoo"];
        let mut adornment_order = Vec::new();
        let mut diagnostics = Vec::new();

        // When parsing the domain object body
        let domain_object = parse_domain_object(
            object_type,
            signature,
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
            signature,
            body,
        })) = &doc.nodes[0]
        {
            assert_eq!(signature, "greet(name)");
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
            name,
            type_,
            value,
            body,
        })) = &doc.nodes[0]
        {
            assert_eq!(name, "DEFAULT_TIMEOUT");
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
            name,
            type_,
            value,
            canonical,
            body,
        })) = &doc.nodes[0]
        {
            assert_eq!(name, "Greeter.name");
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
            signature,
            ..
        })) = &doc.nodes[0]
        {
            assert_eq!(signature, "int add(int a, int b)");
        } else {
            panic!("Expected CFunction, got {:?}", doc.nodes[0]);
        }
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

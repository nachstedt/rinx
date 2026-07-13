use serde::{Deserialize, Serialize};

use crate::c_object_type::CObjectType;
use crate::domain::Domain;
use crate::node::Node;
use crate::object_type::ObjectType;
use crate::py_object_type::PyObjectType;
use crate::target_name::TargetName;

/// Extracts the referenceable name from a `py:*` domain object signature.
///
/// Takes the text before the first `(` (or the whole string if there is
/// none), then its last whitespace-separated token — e.g. `"foo(bar)"` ->
/// `"foo"`. This also strips an optional base-class list for free, e.g.
/// `"Greeter(Base)"` -> `"Greeter"`. Python-specific: identifiers never
/// start with a pointer sigil, so unlike the C extractor below, there is
/// nothing to strip from the extracted token.
#[must_use]
pub fn extract_python_object_name(signature: &str) -> String {
    let before_parens = signature.split('(').next().unwrap_or(signature).trim();
    before_parens
        .split_whitespace()
        .next_back()
        .unwrap_or(before_parens)
        .to_string()
}

/// Extracts the referenceable name from a `c:*` domain object signature,
/// e.g. `"int foo(int bar)"` -> `"foo"`.
///
/// Like [`extract_python_object_name`], takes the text before the first `(`
/// and its last whitespace-separated token, but also strips a
/// pointer-return-type sigil (`*`, `**`, ...) that ends up glued to the
/// front of the name when the author writes `Type *name(...)` rather than
/// `Type* name(...)` — e.g. `CPython`'s
/// `"PyObject *PyUnicode_FromString(const char *str)"` ->
/// `"PyUnicode_FromString"`. Still not real C declarator parsing: array
/// declarators, function-pointer declarators, etc. are out of scope
/// (tracked in `spec_gaps.md`).
#[must_use]
pub fn extract_c_object_name(signature: &str) -> String {
    let before_parens = signature.split('(').next().unwrap_or(signature).trim();
    let last_token = before_parens
        .split_whitespace()
        .next_back()
        .unwrap_or(before_parens);
    last_token.trim_start_matches('*').to_string()
}

/// Builds the qualified [`TargetName`] key shared by domain object
/// registration (analyzer) and cross-reference resolution (renderer), so
/// both always agree on the key for the same object.
#[must_use]
pub fn build_domain_object_key(object_type: ObjectType, name: &str) -> TargetName {
    TargetName::new(&format!(
        "{}:{}:{}",
        object_type.domain().as_str(),
        object_type.as_str(),
        name
    ))
}

/// Picks the qualifier to use for a domain object that isn't itself nested
/// inside a `py:class`/`py:exception` body: the enclosing class's qualified
/// name takes precedence (lexical nesting), falling back to the current
/// module set by the most recent `py:module` seen earlier in the document
/// (sequential, document-order state — real Sphinx docs write `py:module`
/// and the functions/classes it documents as siblings, not nested).
///
/// `current_module` only ever applies to `domain` == [`Domain::Py`] — real
/// Sphinx's module context is a `py`-domain-only concept (`env.ref_context
/// ['py:module']`) and never qualifies `c:function` or other non-`py`
/// domain objects, even when they're written as later siblings in the same
/// document.
///
/// Shared by the analyzer (`index_nodes`) and the renderer
/// (`render_domain_object`) so both always agree on which qualifier applies
/// to a given object.
#[must_use]
pub fn effective_qualifier<'a>(
    class_qualifier: Option<&'a str>,
    current_module: Option<&'a str>,
    domain: Domain,
) -> Option<&'a str> {
    class_qualifier.or(match domain {
        Domain::Py => current_module,
        Domain::C => None,
    })
}

/// Prefixes `name` with `qualifier` (e.g. an enclosing `py:class`'s own
/// qualified name), joined with `.`, or returns `name` unchanged if there is
/// no enclosing qualifier *or* `name` is already written fully qualified
/// (starts with `"{qualifier}."`) — real-world Sphinx docs commonly nest a
/// domain object under its already-dotted name (e.g. `CPython`'s
/// `.. attribute:: StopIteration.value` nested inside
/// `.. exception:: StopIteration`), and real Sphinx's own `py` domain avoids
/// double-prepending in that case too. Shared by the analyzer (when indexing
/// a nested domain object) and the renderer (when computing its anchor
/// `id`), so both always agree on the qualified name for the same nested
/// object.
#[must_use]
pub fn qualify_name(qualifier: Option<&str>, name: &str) -> String {
    match qualifier {
        Some(prefix) if !name.starts_with(&format!("{prefix}.")) => {
            format!("{prefix}.{name}")
        }
        _ => name.to_string(),
    }
}

/// The body of a domain object *definition* directive (e.g. `.. py:function::`,
/// `.. py:module::`, `.. c:function::`) — one variant per concrete object
/// type, each carrying exactly the fields/options meaningful to it.
///
/// This is deliberately separate from [`ObjectType`]: `ObjectType` is a
/// lightweight tag shared with cross-reference roles (`InlineNode::
/// DomainObjectReference`), which never carry options — only definitions do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DomainObjectBody {
    PyFunction {
        signature: String,
        body: Vec<Node>,
    },
    PyModule {
        name: String,
        /// Comma-separated platform identifiers (e.g. `"Unix, Windows"`).
        platform: Option<String>,
        /// One-sentence module summary.
        synopsis: Option<String>,
        /// Marks the module as deprecated.
        deprecated: bool,
        body: Vec<Node>,
    },
    PyData {
        name: String,
        /// The data item's type annotation (e.g. `"int"`).
        type_: Option<String>,
        /// The data item's value (e.g. `"30"`).
        value: Option<String>,
        body: Vec<Node>,
    },
    PyAttribute {
        name: String,
        /// The attribute's type annotation (e.g. `"int"`).
        type_: Option<String>,
        /// The attribute's initial value (e.g. `"30"`).
        value: Option<String>,
        /// The fully qualified name (including module) of where the
        /// attribute is actually defined, when documented via a re-export.
        /// Rendered as metadata only — no alias/cross-reference-redirect
        /// semantics.
        canonical: Option<String>,
        body: Vec<Node>,
    },
    CFunction {
        signature: String,
        body: Vec<Node>,
    },
    CMacro {
        signature: String,
        body: Vec<Node>,
    },
    PyMethod {
        signature: String,
        is_classmethod: bool,
        is_staticmethod: bool,
        is_abstractmethod: bool,
        is_async: bool,
        body: Vec<Node>,
    },
    PyClass {
        signature: String,
        is_final: bool,
        body: Vec<Node>,
    },
    PyException {
        signature: String,
        is_final: bool,
        body: Vec<Node>,
    },
}

impl DomainObjectBody {
    /// The [`ObjectType`] this definition belongs to.
    #[must_use]
    pub const fn object_type(&self) -> ObjectType {
        match self {
            Self::PyFunction { .. } => ObjectType::Py(PyObjectType::Function),
            Self::PyModule { .. } => ObjectType::Py(PyObjectType::Module),
            Self::PyData { .. } => ObjectType::Py(PyObjectType::Data),
            Self::PyAttribute { .. } => ObjectType::Py(PyObjectType::Attribute),
            Self::CFunction { .. } => ObjectType::C(CObjectType::Function),
            Self::CMacro { .. } => ObjectType::C(CObjectType::Macro),
            Self::PyMethod { .. } => ObjectType::Py(PyObjectType::Method),
            Self::PyClass { .. } => ObjectType::Py(PyObjectType::Class),
            Self::PyException { .. } => ObjectType::Py(PyObjectType::Exception),
        }
    }

    /// The referenceable name used to build the cross-reference key
    /// ([`build_domain_object_key`]) — extracted from the signature for
    /// function-like objects, or the dotted name directly for modules.
    #[must_use]
    pub fn name(&self) -> String {
        match self {
            Self::PyFunction { signature, .. }
            | Self::PyMethod { signature, .. }
            | Self::PyClass { signature, .. }
            | Self::PyException { signature, .. } => extract_python_object_name(signature),
            Self::CFunction { signature, .. } | Self::CMacro { signature, .. } => {
                extract_c_object_name(signature)
            }
            Self::PyModule { name, .. }
            | Self::PyData { name, .. }
            | Self::PyAttribute { name, .. } => name.clone(),
        }
    }

    /// The raw text shown in the rendered `<dt>` — the full signature for
    /// function-like objects, or the bare dotted name for modules/data/attributes.
    #[must_use]
    pub fn signature_text(&self) -> &str {
        match self {
            Self::PyFunction { signature, .. }
            | Self::CFunction { signature, .. }
            | Self::CMacro { signature, .. }
            | Self::PyMethod { signature, .. }
            | Self::PyClass { signature, .. }
            | Self::PyException { signature, .. } => signature,
            Self::PyModule { name, .. }
            | Self::PyData { name, .. }
            | Self::PyAttribute { name, .. } => name,
        }
    }

    /// The parsed docstring body shared by every object type.
    #[must_use]
    pub fn body(&self) -> &[Node] {
        match self {
            Self::PyFunction { body, .. }
            | Self::PyModule { body, .. }
            | Self::PyData { body, .. }
            | Self::PyAttribute { body, .. }
            | Self::CFunction { body, .. }
            | Self::CMacro { body, .. }
            | Self::PyMethod { body, .. }
            | Self::PyClass { body, .. }
            | Self::PyException { body, .. } => body,
        }
    }

    /// Mutable access to the parsed docstring body, for passes that rewrite
    /// nested nodes in place (e.g. assigning `.. index::` anchor ids).
    #[must_use]
    pub fn body_mut(&mut self) -> &mut Vec<Node> {
        match self {
            Self::PyFunction { body, .. }
            | Self::PyModule { body, .. }
            | Self::PyData { body, .. }
            | Self::PyAttribute { body, .. }
            | Self::CFunction { body, .. }
            | Self::CMacro { body, .. }
            | Self::PyMethod { body, .. }
            | Self::PyClass { body, .. }
            | Self::PyException { body, .. } => body,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inline_node::InlineNode;

    #[test]
    fn test_extract_python_object_name_simple_call() {
        // Given
        let signature = "foo(bar)";

        // When
        let name = extract_python_object_name(signature);

        // Then
        assert_eq!(name, "foo");
    }

    #[test]
    fn test_extract_python_object_name_no_parens() {
        // Given
        let signature = "foo";

        // When
        let name = extract_python_object_name(signature);

        // Then
        assert_eq!(name, "foo");
    }

    #[test]
    fn test_extract_python_object_name_no_args() {
        // Given
        let signature = "foo()";

        // When
        let name = extract_python_object_name(signature);

        // Then
        assert_eq!(name, "foo");
    }

    #[test]
    fn test_extract_python_object_name_extra_whitespace() {
        // Given
        let signature = "  foo   (bar)";

        // When
        let name = extract_python_object_name(signature);

        // Then
        assert_eq!(name, "foo");
    }

    #[test]
    fn test_extract_python_object_name_empty_string() {
        // Given
        let signature = "";

        // When
        let name = extract_python_object_name(signature);

        // Then
        assert_eq!(name, "");
    }

    #[test]
    fn test_extract_c_object_name_simple_call() {
        // Given
        let signature = "foo(bar)";

        // When
        let name = extract_c_object_name(signature);

        // Then
        assert_eq!(name, "foo");
    }

    #[test]
    fn test_extract_c_object_name_no_parens() {
        // Given
        let signature = "foo";

        // When
        let name = extract_c_object_name(signature);

        // Then
        assert_eq!(name, "foo");
    }

    #[test]
    fn test_extract_c_object_name_empty_string() {
        // Given
        let signature = "";

        // When
        let name = extract_c_object_name(signature);

        // Then
        assert_eq!(name, "");
    }

    #[test]
    fn test_extract_c_object_name_return_type_prefix() {
        // Given
        let signature = "int foo(int bar)";

        // When
        let name = extract_c_object_name(signature);

        // Then
        assert_eq!(name, "foo");
    }

    #[test]
    fn test_extract_c_object_name_strips_pointer_sigil_glued_to_name() {
        // Given
        let signature = "char *foo(void)";

        // When
        let name = extract_c_object_name(signature);

        // Then
        assert_eq!(name, "foo");
    }

    #[test]
    fn test_extract_c_object_name_strips_double_pointer_sigil() {
        // Given
        let signature = "int **foo(void)";

        // When
        let name = extract_c_object_name(signature);

        // Then
        assert_eq!(name, "foo");
    }

    #[test]
    fn test_extract_c_object_name_unaffected_when_sigil_glued_to_type() {
        // Given
        let signature = "PyObject* PyUnicode_FromStringAndSize(const char *str, Py_ssize_t size)";

        // When
        let name = extract_c_object_name(signature);

        // Then
        assert_eq!(name, "PyUnicode_FromStringAndSize");
    }

    #[test]
    fn test_extract_c_object_name_matches_cpython_unicode_fromstring() {
        // Given — the real-world signature that surfaced the broken-link bug
        let signature = "PyObject *PyUnicode_FromString(const char *str)";

        // When
        let name = extract_c_object_name(signature);

        // Then
        assert_eq!(name, "PyUnicode_FromString");
    }

    #[test]
    fn test_extract_c_object_name_bare_macro_name() {
        // Given — object-like macros have no parens and no return type
        let signature = "PY_SSIZE_T_MAX";

        // When
        let name = extract_c_object_name(signature);

        // Then
        assert_eq!(name, "PY_SSIZE_T_MAX");
    }

    #[test]
    fn test_extract_c_object_name_function_like_macro() {
        // Given — function-like macros have no return type to strip
        let signature = "MAX(a, b)";

        // When
        let name = extract_c_object_name(signature);

        // Then
        assert_eq!(name, "MAX");
    }

    #[test]
    fn test_build_domain_object_key_produces_expected_format() {
        // Given
        let object_type = ObjectType::Py(PyObjectType::Function);
        let name = "foo";

        // When
        let key = build_domain_object_key(object_type, name);

        // Then
        assert_eq!(key.as_str(), "py:function:foo");
    }

    #[test]
    fn test_build_domain_object_key_distinguishes_domains() {
        // Given
        let py_type = ObjectType::Py(PyObjectType::Function);
        let c_type = ObjectType::C(CObjectType::Function);
        let name = "foo";

        // When
        let py_key = build_domain_object_key(py_type, name);
        let c_key = build_domain_object_key(c_type, name);

        // Then
        assert_ne!(py_key, c_key);
    }

    #[test]
    fn test_build_domain_object_key_for_module() {
        // Given
        let object_type = ObjectType::Py(PyObjectType::Module);
        let name = "mypackage.mymodule";

        // When
        let key = build_domain_object_key(object_type, name);

        // Then
        assert_eq!(key.as_str(), "py:module:mypackage.mymodule");
    }

    #[test]
    fn test_build_domain_object_key_for_data_is_shared_by_data_and_const_roles() {
        // Given
        let object_type = ObjectType::Py(PyObjectType::Data);
        let name = "DEFAULT_TIMEOUT";

        // When — both `:py:data:` and `:py:const:` resolve to the same
        // `ObjectType`, so both must build this same key.
        let key = build_domain_object_key(object_type, name);

        // Then — `TargetName` normalizes to lowercase.
        assert_eq!(key.as_str(), "py:data:default_timeout");
    }

    #[test]
    fn test_effective_qualifier_returns_none_when_neither_set() {
        // Given / When / Then
        assert_eq!(effective_qualifier(None, None, Domain::Py), None);
    }

    #[test]
    fn test_effective_qualifier_falls_back_to_current_module_for_py_domain() {
        // Given / When / Then
        assert_eq!(
            effective_qualifier(None, Some("types"), Domain::Py),
            Some("types")
        );
    }

    #[test]
    fn test_effective_qualifier_prefers_class_qualifier_over_current_module() {
        // Given — a method nested inside a class that itself lives in a
        // module: the class's own (already module-qualified) name wins.
        // When / Then
        assert_eq!(
            effective_qualifier(Some("types.Greeter"), Some("types"), Domain::Py),
            Some("types.Greeter")
        );
    }

    #[test]
    fn test_effective_qualifier_never_applies_current_module_to_c_domain() {
        // Given — real Sphinx's module context is `py`-domain-only; a
        // `c:function` written as a later sibling after `py:module:: types`
        // must not get "types." prepended (the bug this guards against:
        // CPython's `types.rst` documents a `c:function` right after
        // `.. module:: types`, and it must not be swept into that scope).
        // When / Then
        assert_eq!(effective_qualifier(None, Some("types"), Domain::C), None);
    }

    #[test]
    fn test_qualify_name_returns_bare_name_when_no_qualifier() {
        // Given / When / Then
        assert_eq!(qualify_name(None, "greet"), "greet");
    }

    #[test]
    fn test_qualify_name_prefixes_with_qualifier() {
        // Given / When / Then
        assert_eq!(qualify_name(Some("Greeter"), "greet"), "Greeter.greet");
    }

    #[test]
    fn test_qualify_name_composes_nested_qualifiers() {
        // Given — a two-level nested-class qualifier, built incrementally
        let outer_qualified = qualify_name(None, "Outer");
        let inner_qualified = qualify_name(Some(&outer_qualified), "Inner");

        // When
        let method_qualified = qualify_name(Some(&inner_qualified), "method");

        // Then
        assert_eq!(method_qualified, "Outer.Inner.method");
    }

    #[test]
    fn test_qualify_name_returns_name_unchanged_when_already_fully_qualified() {
        // Given — CPython's `Doc/library/exceptions.rst` nests
        // `.. attribute:: StopIteration.value` inside
        // `.. exception:: StopIteration`, writing the attribute's name
        // already fully qualified rather than bare (`value`).
        // When / Then — must not double-prepend to
        // "StopIteration.StopIteration.value".
        assert_eq!(
            qualify_name(Some("StopIteration"), "StopIteration.value"),
            "StopIteration.value"
        );
    }

    #[test]
    fn test_qualify_name_composes_nested_qualifiers_when_innermost_name_is_already_qualified() {
        // Given — a two-level nested-class qualifier, where the innermost
        // name is already written fully qualified (like the
        // `StopIteration.value` case, but two levels deep).
        let outer_qualified = qualify_name(None, "Outer");
        let inner_qualified = qualify_name(Some(&outer_qualified), "Inner");

        // When
        let method_qualified = qualify_name(Some(&inner_qualified), "Outer.Inner.method");

        // Then — not doubled to "Outer.Inner.Outer.Inner.method"
        assert_eq!(method_qualified, "Outer.Inner.method");
    }

    #[test]
    fn test_build_domain_object_key_for_attribute() {
        // Given
        let object_type = ObjectType::Py(PyObjectType::Attribute);
        let name = "Greeter.name";

        // When
        let key = build_domain_object_key(object_type, name);

        // Then
        assert_eq!(key.as_str(), "py:attribute:greeter.name");
    }

    #[test]
    fn test_domain_object_body_object_type_matches_variant() {
        // Given / When / Then
        assert_eq!(
            DomainObjectBody::PyFunction {
                signature: "greet(name)".to_string(),
                body: vec![],
            }
            .object_type(),
            ObjectType::Py(PyObjectType::Function)
        );
        assert_eq!(
            DomainObjectBody::PyModule {
                name: "greetings".to_string(),
                platform: None,
                synopsis: None,
                deprecated: false,
                body: vec![],
            }
            .object_type(),
            ObjectType::Py(PyObjectType::Module)
        );
        assert_eq!(
            DomainObjectBody::PyData {
                name: "DEFAULT_TIMEOUT".to_string(),
                type_: None,
                value: None,
                body: vec![],
            }
            .object_type(),
            ObjectType::Py(PyObjectType::Data)
        );
        assert_eq!(
            DomainObjectBody::PyAttribute {
                name: "Greeter.name".to_string(),
                type_: None,
                value: None,
                canonical: None,
                body: vec![],
            }
            .object_type(),
            ObjectType::Py(PyObjectType::Attribute)
        );
        assert_eq!(
            DomainObjectBody::CFunction {
                signature: "int add(int a, int b)".to_string(),
                body: vec![],
            }
            .object_type(),
            ObjectType::C(CObjectType::Function)
        );
        assert_eq!(
            DomainObjectBody::CMacro {
                signature: "MAX(a, b)".to_string(),
                body: vec![],
            }
            .object_type(),
            ObjectType::C(CObjectType::Macro)
        );
        assert_eq!(
            DomainObjectBody::PyMethod {
                signature: "greet(self, name)".to_string(),
                is_classmethod: false,
                is_staticmethod: false,
                is_abstractmethod: false,
                is_async: false,
                body: vec![],
            }
            .object_type(),
            ObjectType::Py(PyObjectType::Method)
        );
        assert_eq!(
            DomainObjectBody::PyClass {
                signature: "Greeter".to_string(),
                is_final: false,
                body: vec![],
            }
            .object_type(),
            ObjectType::Py(PyObjectType::Class)
        );
        assert_eq!(
            DomainObjectBody::PyException {
                signature: "GreeterError".to_string(),
                is_final: false,
                body: vec![],
            }
            .object_type(),
            ObjectType::Py(PyObjectType::Exception)
        );
    }

    #[test]
    fn test_domain_object_body_name_extracts_from_signature_for_functions() {
        // Given
        let function = DomainObjectBody::PyFunction {
            signature: "greet(name)".to_string(),
            body: vec![],
        };

        // When / Then
        assert_eq!(function.name(), "greet");
    }

    #[test]
    fn test_domain_object_body_name_extracts_from_signature_for_macros() {
        // Given
        let macro_ = DomainObjectBody::CMacro {
            signature: "MAX(a, b)".to_string(),
            body: vec![],
        };

        // When / Then
        assert_eq!(macro_.name(), "MAX");
    }

    #[test]
    fn test_domain_object_body_name_uses_bare_signature_for_object_like_macros() {
        // Given
        let macro_ = DomainObjectBody::CMacro {
            signature: "PY_SSIZE_T_MAX".to_string(),
            body: vec![],
        };

        // When / Then
        assert_eq!(macro_.name(), "PY_SSIZE_T_MAX");
    }

    #[test]
    fn test_domain_object_body_name_extracts_from_signature_for_methods() {
        // Given
        let method = DomainObjectBody::PyMethod {
            signature: "Greeter.greet(self, name)".to_string(),
            is_classmethod: false,
            is_staticmethod: false,
            is_abstractmethod: false,
            is_async: false,
            body: vec![],
        };

        // When / Then
        assert_eq!(method.name(), "Greeter.greet");
    }

    #[test]
    fn test_domain_object_body_signature_text_shows_full_signature_for_methods() {
        // Given
        let method = DomainObjectBody::PyMethod {
            signature: "greet(self, name)".to_string(),
            is_classmethod: true,
            is_staticmethod: false,
            is_abstractmethod: false,
            is_async: false,
            body: vec![],
        };

        // When / Then
        assert_eq!(method.signature_text(), "greet(self, name)");
    }

    #[test]
    fn test_domain_object_body_name_extracts_from_signature_for_classes() {
        // Given
        let class = DomainObjectBody::PyClass {
            signature: "Greeter".to_string(),
            is_final: false,
            body: vec![],
        };

        // When / Then
        assert_eq!(class.name(), "Greeter");
    }

    #[test]
    fn test_domain_object_body_name_ignores_base_class_list() {
        // Given — base classes shouldn't leak into the referenceable name
        let class = DomainObjectBody::PyClass {
            signature: "Greeter(Base)".to_string(),
            is_final: false,
            body: vec![],
        };

        // When / Then
        assert_eq!(class.name(), "Greeter");
    }

    #[test]
    fn test_domain_object_body_signature_text_shows_full_signature_for_classes() {
        // Given
        let class = DomainObjectBody::PyClass {
            signature: "Greeter(Base)".to_string(),
            is_final: true,
            body: vec![],
        };

        // When / Then
        assert_eq!(class.signature_text(), "Greeter(Base)");
    }

    #[test]
    fn test_domain_object_body_name_extracts_from_signature_for_exceptions() {
        // Given
        let exception = DomainObjectBody::PyException {
            signature: "GreeterError".to_string(),
            is_final: false,
            body: vec![],
        };

        // When / Then
        assert_eq!(exception.name(), "GreeterError");
    }

    #[test]
    fn test_domain_object_body_name_ignores_base_class_list_for_exceptions() {
        // Given — base classes shouldn't leak into the referenceable name
        let exception = DomainObjectBody::PyException {
            signature: "InvalidNameError(GreeterError)".to_string(),
            is_final: false,
            body: vec![],
        };

        // When / Then
        assert_eq!(exception.name(), "InvalidNameError");
    }

    #[test]
    fn test_domain_object_body_signature_text_shows_full_signature_for_exceptions() {
        // Given
        let exception = DomainObjectBody::PyException {
            signature: "InvalidNameError(GreeterError)".to_string(),
            is_final: true,
            body: vec![],
        };

        // When / Then
        assert_eq!(exception.signature_text(), "InvalidNameError(GreeterError)");
    }

    #[test]
    fn test_domain_object_body_name_uses_bare_name_for_modules() {
        // Given
        let module = DomainObjectBody::PyModule {
            name: "mypackage.mymodule".to_string(),
            platform: None,
            synopsis: None,
            deprecated: false,
            body: vec![],
        };

        // When / Then
        assert_eq!(module.name(), "mypackage.mymodule");
    }

    #[test]
    fn test_domain_object_body_name_uses_bare_name_for_data() {
        // Given
        let data = DomainObjectBody::PyData {
            name: "DEFAULT_TIMEOUT".to_string(),
            type_: None,
            value: None,
            body: vec![],
        };

        // When / Then
        assert_eq!(data.name(), "DEFAULT_TIMEOUT");
    }

    #[test]
    fn test_domain_object_body_name_uses_bare_name_for_attributes() {
        // Given
        let attribute = DomainObjectBody::PyAttribute {
            name: "Greeter.name".to_string(),
            type_: None,
            value: None,
            canonical: None,
            body: vec![],
        };

        // When / Then
        assert_eq!(attribute.name(), "Greeter.name");
    }

    #[test]
    fn test_domain_object_body_signature_text_shows_full_signature_for_functions() {
        // Given
        let function = DomainObjectBody::CFunction {
            signature: "int add(int a, int b)".to_string(),
            body: vec![],
        };

        // When / Then
        assert_eq!(function.signature_text(), "int add(int a, int b)");
    }

    #[test]
    fn test_domain_object_body_signature_text_shows_full_signature_for_macros() {
        // Given
        let macro_ = DomainObjectBody::CMacro {
            signature: "MAX(a, b)".to_string(),
            body: vec![],
        };

        // When / Then
        assert_eq!(macro_.signature_text(), "MAX(a, b)");
    }

    #[test]
    fn test_domain_object_body_signature_text_shows_bare_name_for_modules() {
        // Given
        let module = DomainObjectBody::PyModule {
            name: "greetings".to_string(),
            platform: None,
            synopsis: None,
            deprecated: false,
            body: vec![],
        };

        // When / Then
        assert_eq!(module.signature_text(), "greetings");
    }

    #[test]
    fn test_domain_object_body_signature_text_shows_bare_name_for_data() {
        // Given
        let data = DomainObjectBody::PyData {
            name: "DEFAULT_TIMEOUT".to_string(),
            type_: None,
            value: None,
            body: vec![],
        };

        // When / Then
        assert_eq!(data.signature_text(), "DEFAULT_TIMEOUT");
    }

    #[test]
    fn test_domain_object_body_signature_text_shows_bare_name_for_attributes() {
        // Given
        let attribute = DomainObjectBody::PyAttribute {
            name: "Greeter.name".to_string(),
            type_: None,
            value: None,
            canonical: None,
            body: vec![],
        };

        // When / Then
        assert_eq!(attribute.signature_text(), "Greeter.name");
    }

    #[test]
    fn test_domain_object_body_body_returns_shared_body_for_every_variant() {
        // Given
        let paragraph = Node::Paragraph(vec![InlineNode::Text("hello".to_string())]);
        let function = DomainObjectBody::PyFunction {
            signature: "greet(name)".to_string(),
            body: vec![paragraph.clone()],
        };
        let module = DomainObjectBody::PyModule {
            name: "greetings".to_string(),
            platform: None,
            synopsis: None,
            deprecated: false,
            body: vec![paragraph.clone()],
        };
        let data = DomainObjectBody::PyData {
            name: "DEFAULT_TIMEOUT".to_string(),
            type_: None,
            value: None,
            body: vec![paragraph.clone()],
        };
        let attribute = DomainObjectBody::PyAttribute {
            name: "Greeter.name".to_string(),
            type_: None,
            value: None,
            canonical: None,
            body: vec![paragraph.clone()],
        };
        let c_function = DomainObjectBody::CFunction {
            signature: "int add(int a, int b)".to_string(),
            body: vec![paragraph.clone()],
        };
        let c_macro = DomainObjectBody::CMacro {
            signature: "MAX(a, b)".to_string(),
            body: vec![paragraph.clone()],
        };
        let method = DomainObjectBody::PyMethod {
            signature: "greet(self, name)".to_string(),
            is_classmethod: false,
            is_staticmethod: false,
            is_abstractmethod: false,
            is_async: false,
            body: vec![paragraph.clone()],
        };
        let class = DomainObjectBody::PyClass {
            signature: "Greeter".to_string(),
            is_final: false,
            body: vec![paragraph.clone()],
        };
        let exception = DomainObjectBody::PyException {
            signature: "GreeterError".to_string(),
            is_final: false,
            body: vec![paragraph.clone()],
        };

        // When / Then
        assert_eq!(function.body(), std::slice::from_ref(&paragraph));
        assert_eq!(module.body(), std::slice::from_ref(&paragraph));
        assert_eq!(data.body(), std::slice::from_ref(&paragraph));
        assert_eq!(attribute.body(), std::slice::from_ref(&paragraph));
        assert_eq!(c_function.body(), std::slice::from_ref(&paragraph));
        assert_eq!(c_macro.body(), std::slice::from_ref(&paragraph));
        assert_eq!(method.body(), std::slice::from_ref(&paragraph));
        assert_eq!(class.body(), std::slice::from_ref(&paragraph));
        assert_eq!(exception.body(), std::slice::from_ref(&paragraph));
    }

    #[test]
    fn test_body_mut_allows_in_place_rewrite() {
        // Given
        let mut function = DomainObjectBody::PyFunction {
            signature: "foo()".to_string(),
            body: vec![Node::Comment],
        };

        // When
        function.body_mut().push(Node::Comment);

        // Then
        assert_eq!(function.body(), &[Node::Comment, Node::Comment]);
    }
}

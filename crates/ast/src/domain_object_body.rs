use serde::{Deserialize, Serialize};

use crate::c_object_type::CObjectType;
use crate::node::Node;
use crate::object_type::ObjectType;
use crate::py_object_type::PyObjectType;
use crate::target_name::TargetName;

/// Extracts the referenceable name from a domain object signature.
///
/// Takes the text before the first `(` (or the whole string if there is
/// none), then its last whitespace-separated token — e.g. `"foo(bar)"` ->
/// `"foo"`, `"int foo(int bar)"` -> `"foo"`. A pointer return type like
/// `"char *foo(void)"` naively yields `"*foo"`, since real C declarator
/// parsing is out of scope (tracked in `spec_gaps.md`).
#[must_use]
pub fn extract_object_name(signature: &str) -> String {
    let before_parens = signature.split('(').next().unwrap_or(signature).trim();
    before_parens
        .split_whitespace()
        .next_back()
        .unwrap_or(before_parens)
        .to_string()
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

/// Prefixes `name` with `qualifier` (e.g. an enclosing `py:class`'s own
/// qualified name), joined with `.`, or returns `name` unchanged if there is
/// no enclosing qualifier. Shared by the analyzer (when indexing a nested
/// domain object) and the renderer (when computing its anchor `id`), so both
/// always agree on the qualified name for the same nested object.
#[must_use]
pub fn qualify_name(qualifier: Option<&str>, name: &str) -> String {
    match qualifier {
        Some(prefix) => format!("{prefix}.{name}"),
        None => name.to_string(),
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
    CFunction {
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
}

impl DomainObjectBody {
    /// The [`ObjectType`] this definition belongs to.
    #[must_use]
    pub const fn object_type(&self) -> ObjectType {
        match self {
            Self::PyFunction { .. } => ObjectType::Py(PyObjectType::Function),
            Self::PyModule { .. } => ObjectType::Py(PyObjectType::Module),
            Self::PyData { .. } => ObjectType::Py(PyObjectType::Data),
            Self::CFunction { .. } => ObjectType::C(CObjectType::Function),
            Self::PyMethod { .. } => ObjectType::Py(PyObjectType::Method),
            Self::PyClass { .. } => ObjectType::Py(PyObjectType::Class),
        }
    }

    /// The referenceable name used to build the cross-reference key
    /// ([`build_domain_object_key`]) — extracted from the signature for
    /// function-like objects, or the dotted name directly for modules.
    #[must_use]
    pub fn name(&self) -> String {
        match self {
            Self::PyFunction { signature, .. }
            | Self::CFunction { signature, .. }
            | Self::PyMethod { signature, .. }
            | Self::PyClass { signature, .. } => extract_object_name(signature),
            Self::PyModule { name, .. } | Self::PyData { name, .. } => name.clone(),
        }
    }

    /// The raw text shown in the rendered `<dt>` — the full signature for
    /// function-like objects, or the bare dotted name for modules/data.
    #[must_use]
    pub fn signature_text(&self) -> &str {
        match self {
            Self::PyFunction { signature, .. }
            | Self::CFunction { signature, .. }
            | Self::PyMethod { signature, .. }
            | Self::PyClass { signature, .. } => signature,
            Self::PyModule { name, .. } | Self::PyData { name, .. } => name,
        }
    }

    /// The parsed docstring body shared by every object type.
    #[must_use]
    pub fn body(&self) -> &[Node] {
        match self {
            Self::PyFunction { body, .. }
            | Self::PyModule { body, .. }
            | Self::PyData { body, .. }
            | Self::CFunction { body, .. }
            | Self::PyMethod { body, .. }
            | Self::PyClass { body, .. } => body,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inline_node::InlineNode;

    #[test]
    fn test_extract_object_name_simple_call() {
        // Given
        let signature = "foo(bar)";

        // When
        let name = extract_object_name(signature);

        // Then
        assert_eq!(name, "foo");
    }

    #[test]
    fn test_extract_object_name_no_parens() {
        // Given
        let signature = "foo";

        // When
        let name = extract_object_name(signature);

        // Then
        assert_eq!(name, "foo");
    }

    #[test]
    fn test_extract_object_name_no_args() {
        // Given
        let signature = "foo()";

        // When
        let name = extract_object_name(signature);

        // Then
        assert_eq!(name, "foo");
    }

    #[test]
    fn test_extract_object_name_c_style_return_type_prefix() {
        // Given
        let signature = "int foo(int bar)";

        // When
        let name = extract_object_name(signature);

        // Then
        assert_eq!(name, "foo");
    }

    #[test]
    fn test_extract_object_name_extra_whitespace() {
        // Given
        let signature = "  foo   (bar)";

        // When
        let name = extract_object_name(signature);

        // Then
        assert_eq!(name, "foo");
    }

    #[test]
    fn test_extract_object_name_empty_string() {
        // Given
        let signature = "";

        // When
        let name = extract_object_name(signature);

        // Then
        assert_eq!(name, "");
    }

    #[test]
    fn test_extract_object_name_pointer_return_type_is_naive() {
        // Given — documented limitation: no real C declarator parsing
        let signature = "char *foo(void)";

        // When
        let name = extract_object_name(signature);

        // Then
        assert_eq!(name, "*foo");
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
            DomainObjectBody::CFunction {
                signature: "int add(int a, int b)".to_string(),
                body: vec![],
            }
            .object_type(),
            ObjectType::C(CObjectType::Function)
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
        let c_function = DomainObjectBody::CFunction {
            signature: "int add(int a, int b)".to_string(),
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

        // When / Then
        assert_eq!(function.body(), std::slice::from_ref(&paragraph));
        assert_eq!(module.body(), std::slice::from_ref(&paragraph));
        assert_eq!(data.body(), std::slice::from_ref(&paragraph));
        assert_eq!(c_function.body(), std::slice::from_ref(&paragraph));
        assert_eq!(method.body(), std::slice::from_ref(&paragraph));
        assert_eq!(class.body(), std::slice::from_ref(&paragraph));
    }
}

use serde::{Deserialize, Serialize};

use crate::c_object_type::CObjectType;
use crate::node::Node;
use crate::non_empty_vector::NonEmptyVector;
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

/// The body of a domain object *definition* directive (e.g. `.. py:function::`,
/// `.. py:module::`, `.. c:function::`) — one variant per concrete object
/// type, each carrying exactly the fields/options meaningful to it.
///
/// This is deliberately separate from [`ObjectType`]: `ObjectType` is a
/// lightweight tag shared with cross-reference roles (`InlineNode::
/// DomainObjectReference`), which never carry options — only definitions do.
///
/// Every variant but `PyModule` holds `signatures`, not a single name: one
/// directive may declare several argument lines, each an independently
/// referenceable alias for the same documented object, all sharing one body.
/// `PyModule` is the exception because real Sphinx's `module` directive takes
/// exactly one argument.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DomainObjectBody {
    PyFunction {
        signatures: NonEmptyVector<String>,
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
        signatures: NonEmptyVector<String>,
        /// The data item's type annotation (e.g. `"int"`).
        type_: Option<String>,
        /// The data item's value (e.g. `"30"`).
        value: Option<String>,
        body: Vec<Node>,
    },
    PyAttribute {
        signatures: NonEmptyVector<String>,
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
        signatures: NonEmptyVector<String>,
        body: Vec<Node>,
    },
    CMacro {
        signatures: NonEmptyVector<String>,
        body: Vec<Node>,
    },
    PyMethod {
        signatures: NonEmptyVector<String>,
        is_classmethod: bool,
        is_staticmethod: bool,
        is_abstractmethod: bool,
        is_async: bool,
        body: Vec<Node>,
    },
    PyClass {
        signatures: NonEmptyVector<String>,
        is_final: bool,
        body: Vec<Node>,
    },
    PyException {
        signatures: NonEmptyVector<String>,
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

    /// The referenceable names used to build the cross-reference keys
    /// ([`build_domain_object_key`]) — one per declared signature, extracted
    /// from the signature for function-like objects, or taken directly for
    /// modules/data/attributes, whose signatures are already bare names.
    ///
    /// The first entry is the *primary* name: the only one qualified against
    /// the enclosing scope for the purpose of lending class context to the
    /// body (see [`Self::deduce_local_scope`]), mirroring real Sphinx, whose
    /// `before_content()` acts on the first parsed signature. Every entry,
    /// primary or not, is registered as an independently resolvable target.
    #[must_use]
    pub fn names(&self) -> NonEmptyVector<String> {
        match self {
            Self::PyFunction { signatures, .. }
            | Self::PyMethod { signatures, .. }
            | Self::PyClass { signatures, .. }
            | Self::PyException { signatures, .. } => {
                signatures.map(|signature| extract_python_object_name(signature))
            }
            Self::CFunction { signatures, .. } | Self::CMacro { signatures, .. } => {
                signatures.map(|signature| extract_c_object_name(signature))
            }
            Self::PyData { signatures, .. } | Self::PyAttribute { signatures, .. } => {
                signatures.map(String::clone)
            }
            Self::PyModule { name, .. } => NonEmptyVector::single(name.clone()),
        }
    }

    /// The raw texts shown in the rendered `<dt>`s — one per declared
    /// signature: the full signature for function-like objects, or the bare
    /// dotted name for modules/data/attributes.
    ///
    /// Index-parallel to [`Self::names`] and equally non-empty; returns a
    /// plain slice rather than a [`NonEmptyVector`] only because `PyModule`
    /// stores a single `String` with no such vector to borrow.
    #[must_use]
    pub fn signature_texts(&self) -> &[String] {
        match self {
            Self::PyFunction { signatures, .. }
            | Self::CFunction { signatures, .. }
            | Self::CMacro { signatures, .. }
            | Self::PyMethod { signatures, .. }
            | Self::PyClass { signatures, .. }
            | Self::PyException { signatures, .. }
            | Self::PyData { signatures, .. }
            | Self::PyAttribute { signatures, .. } => signatures.as_slice(),
            Self::PyModule { name, .. } => std::slice::from_ref(name),
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

    /// The class segments, if any, that this object's own nested body
    /// content should have pushed onto the enclosing `PythonScope` (in the
    /// `rusty_sphinx_scope` crate, which depends on this one, not the other
    /// way around, so it can't be linked from here) — the "class context" a
    /// bare cross-reference written inside that body resolves against
    /// first, and the qualifier a domain object *defined* inside it is
    /// indexed under. `new_segments` is this object's own contribution as
    /// returned by `PythonScope::qualify` — its (possibly dotted) name with
    /// any repeat of the *existing* class scope already absorbed.
    ///
    /// Two ways an object establishes one:
    /// - `py:class`/`py:exception` bodies introduce every one of their own
    ///   new segments (real lexical nesting: `.. method:: find_spec` written
    ///   inside `.. class:: zipimporter` is
    ///   `zipimport.zipimporter.find_spec`).
    /// - Any other `py` object *written with a dotted signature* lends all
    ///   but the last of its new segments (`.. method:: ZipFile.read` lends
    ///   `ZipFile`), mirroring real Sphinx's `PyObject.before_content()`/
    ///   `after_content()`, which sets the `py:class` `ref_context` from the
    ///   signature's name-prefix for the duration of that directive's body,
    ///   then restores it. This is what lets a bare ``:meth:`read` `` written
    ///   inside `.. method:: ZipFile.open`'s body resolve against
    ///   `ZipFile.read` with no global search: `open`'s own signature
    ///   already carries the scope. Real `CPython` docs (e.g. `zipfile.rst`)
    ///   document a class's methods flat like this rather than nested.
    ///
    /// `py:module` establishes no *class* scope: real Sphinx's `module`
    /// directive is not a `PyObject` and never sets `py:class` from its own
    /// name — it sets only the persistent, document-order module context
    /// `PythonScope::set_module` already models, so a dotted module
    /// name (`xml.etree.ElementTree`) must never be mistaken for a class
    /// prefix. `c` domain objects establish none either.
    ///
    /// Shared by the analyzer (`index_domain_object`) and the renderer
    /// (`render_domain_object`), so both always agree on the scope a given
    /// body introduces. Matched exhaustively rather than with a wildcard, so
    /// a new object type can't be added without deciding what it scopes.
    #[must_use]
    pub fn deduce_local_scope(&self, new_segments: &[String]) -> Vec<String> {
        match self {
            Self::PyClass { .. } | Self::PyException { .. } => new_segments.to_vec(),
            Self::PyFunction { .. }
            | Self::PyMethod { .. }
            | Self::PyData { .. }
            | Self::PyAttribute { .. } => new_segments
                .split_last()
                .map(|(_, rest)| rest.to_vec())
                .unwrap_or_default(),
            Self::PyModule { .. } | Self::CFunction { .. } | Self::CMacro { .. } => Vec::new(),
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
    fn test_deduce_local_scope_lends_all_new_segments_for_classes() {
        // Given
        let class = DomainObjectBody::PyClass {
            signatures: NonEmptyVector::single("zipimporter(archivepath)".to_string()),
            is_final: false,
            body: vec![],
        };

        // When
        let scope = class.deduce_local_scope(&["zipimporter".to_string()]);

        // Then
        assert_eq!(scope, vec!["zipimporter".to_string()]);
    }

    #[test]
    fn test_deduce_local_scope_lends_all_new_segments_for_exceptions() {
        // Given — exceptions are classes in Python, so they scope their body
        // the same way.
        let exception = DomainObjectBody::PyException {
            signatures: NonEmptyVector::single("ZipImportError".to_string()),
            is_final: false,
            body: vec![],
        };

        // When
        let scope = exception.deduce_local_scope(&["ZipImportError".to_string()]);

        // Then
        assert_eq!(scope, vec!["ZipImportError".to_string()]);
    }

    #[test]
    fn test_deduce_local_scope_lends_all_but_last_new_segment_for_dotted_method() {
        // Given — the real-world CPython shape that surfaced the
        // "broken domain object 'read'" warning: `zipfile.rst` documents
        // `ZipFile`'s methods flat, with dotted signatures, so the method's
        // own name carries the class scope its body should resolve against.
        let method = DomainObjectBody::PyMethod {
            signatures: NonEmptyVector::single("ZipFile.open(name, mode='r')".to_string()),
            is_classmethod: false,
            is_staticmethod: false,
            is_abstractmethod: false,
            is_async: false,
            body: vec![],
        };

        // When
        let scope = method.deduce_local_scope(&["ZipFile".to_string(), "open".to_string()]);

        // Then — the last component is dropped, not the whole dotted path.
        assert_eq!(scope, vec!["ZipFile".to_string()]);
    }

    #[test]
    fn test_deduce_local_scope_lends_all_but_last_new_segment_for_dotted_function() {
        // Given
        let function = DomainObjectBody::PyFunction {
            signatures: NonEmptyVector::single("path.join(a, *p)".to_string()),
            body: vec![],
        };

        // When
        let scope = function.deduce_local_scope(&["path".to_string(), "join".to_string()]);

        // Then
        assert_eq!(scope, vec!["path".to_string()]);
    }

    #[test]
    fn test_deduce_local_scope_lends_all_but_last_new_segment_for_dotted_attribute() {
        // Given
        let attribute = DomainObjectBody::PyAttribute {
            signatures: NonEmptyVector::single("ZipInfo.filename".to_string()),
            type_: None,
            value: None,
            canonical: None,
            body: vec![],
        };

        // When
        let attribute_scope =
            attribute.deduce_local_scope(&["ZipInfo".to_string(), "filename".to_string()]);

        // Then
        assert_eq!(attribute_scope, vec!["ZipInfo".to_string()]);
    }

    #[test]
    fn test_deduce_local_scope_lends_all_but_last_new_segment_for_dotted_data() {
        // Given
        let data = DomainObjectBody::PyData {
            signatures: NonEmptyVector::single("ZipFile.DEFAULT_TIMEOUT".to_string()),
            type_: None,
            value: None,
            body: vec![],
        };

        // When
        let scope =
            data.deduce_local_scope(&["ZipFile".to_string(), "DEFAULT_TIMEOUT".to_string()]);

        // Then
        assert_eq!(scope, vec!["ZipFile".to_string()]);
    }

    #[test]
    fn test_deduce_local_scope_returns_empty_for_undotted_function() {
        // Given — an unqualified, module-less function has no prefix to lend.
        let function = DomainObjectBody::PyFunction {
            signatures: NonEmptyVector::single("greet(name)".to_string()),
            body: vec![],
        };

        // When
        let scope = function.deduce_local_scope(&["greet".to_string()]);

        // Then
        assert!(scope.is_empty());
    }

    #[test]
    fn test_deduce_local_scope_returns_empty_for_undotted_method() {
        // Given
        let method = DomainObjectBody::PyMethod {
            signatures: NonEmptyVector::single("find_spec(fullname)".to_string()),
            is_classmethod: false,
            is_staticmethod: false,
            is_abstractmethod: false,
            is_async: false,
            body: vec![],
        };

        // When
        let scope = method.deduce_local_scope(&["find_spec".to_string()]);

        // Then
        assert!(scope.is_empty());
    }

    #[test]
    fn test_deduce_local_scope_returns_empty_for_modules() {
        // Given — real Sphinx's `module` directive is not a `PyObject` and
        // never sets `py:class` from its own name — it only sets the
        // persistent, document-order module context
        // (`PythonScope::set_module`), so a dotted module name
        // (`xml.etree.ElementTree`) must never be mistaken for a class
        // prefix.
        let module = DomainObjectBody::PyModule {
            name: "xml.etree.ElementTree".to_string(),
            platform: None,
            synopsis: None,
            deprecated: false,
            body: vec![],
        };

        // When
        let scope = module.deduce_local_scope(&[
            "xml".to_string(),
            "etree".to_string(),
            "ElementTree".to_string(),
        ]);

        // Then — must not lend "xml.etree" to everything in its body.
        assert!(scope.is_empty());
    }

    #[test]
    fn test_deduce_local_scope_returns_empty_for_c_domain_objects() {
        // Given — the `py:class` context is a py-domain-only concept.
        let function = DomainObjectBody::CFunction {
            signatures: NonEmptyVector::single(
                "int PyList_Append(PyObject *list, PyObject *item)".to_string(),
            ),
            body: vec![],
        };
        let macro_ = DomainObjectBody::CMacro {
            signatures: NonEmptyVector::single("PY_SSIZE_T_MAX".to_string()),
            body: vec![],
        };

        // When / Then
        assert!(
            function
                .deduce_local_scope(&["PyList_Append".to_string()])
                .is_empty()
        );
        assert!(
            macro_
                .deduce_local_scope(&["PY_SSIZE_T_MAX".to_string()])
                .is_empty()
        );
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
                signatures: NonEmptyVector::single("greet(name)".to_string()),
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
                signatures: NonEmptyVector::single("DEFAULT_TIMEOUT".to_string()),
                type_: None,
                value: None,
                body: vec![],
            }
            .object_type(),
            ObjectType::Py(PyObjectType::Data)
        );
        assert_eq!(
            DomainObjectBody::PyAttribute {
                signatures: NonEmptyVector::single("Greeter.name".to_string()),
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
                signatures: NonEmptyVector::single("int add(int a, int b)".to_string()),
                body: vec![],
            }
            .object_type(),
            ObjectType::C(CObjectType::Function)
        );
        assert_eq!(
            DomainObjectBody::CMacro {
                signatures: NonEmptyVector::single("MAX(a, b)".to_string()),
                body: vec![],
            }
            .object_type(),
            ObjectType::C(CObjectType::Macro)
        );
        assert_eq!(
            DomainObjectBody::PyMethod {
                signatures: NonEmptyVector::single("greet(self, name)".to_string()),
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
                signatures: NonEmptyVector::single("Greeter".to_string()),
                is_final: false,
                body: vec![],
            }
            .object_type(),
            ObjectType::Py(PyObjectType::Class)
        );
        assert_eq!(
            DomainObjectBody::PyException {
                signatures: NonEmptyVector::single("GreeterError".to_string()),
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
            signatures: NonEmptyVector::single("greet(name)".to_string()),
            body: vec![],
        };

        // When / Then
        assert_eq!(function.names().as_slice(), ["greet"]);
    }

    #[test]
    fn test_domain_object_body_name_extracts_from_signature_for_macros() {
        // Given
        let macro_ = DomainObjectBody::CMacro {
            signatures: NonEmptyVector::single("MAX(a, b)".to_string()),
            body: vec![],
        };

        // When / Then
        assert_eq!(macro_.names().as_slice(), ["MAX"]);
    }

    #[test]
    fn test_domain_object_body_name_uses_bare_signature_for_object_like_macros() {
        // Given
        let macro_ = DomainObjectBody::CMacro {
            signatures: NonEmptyVector::single("PY_SSIZE_T_MAX".to_string()),
            body: vec![],
        };

        // When / Then
        assert_eq!(macro_.names().as_slice(), ["PY_SSIZE_T_MAX"]);
    }

    #[test]
    fn test_domain_object_body_name_extracts_from_signature_for_methods() {
        // Given
        let method = DomainObjectBody::PyMethod {
            signatures: NonEmptyVector::single("Greeter.greet(self, name)".to_string()),
            is_classmethod: false,
            is_staticmethod: false,
            is_abstractmethod: false,
            is_async: false,
            body: vec![],
        };

        // When / Then
        assert_eq!(method.names().as_slice(), ["Greeter.greet"]);
    }

    #[test]
    fn test_domain_object_body_signature_text_shows_full_signature_for_methods() {
        // Given
        let method = DomainObjectBody::PyMethod {
            signatures: NonEmptyVector::single("greet(self, name)".to_string()),
            is_classmethod: true,
            is_staticmethod: false,
            is_abstractmethod: false,
            is_async: false,
            body: vec![],
        };

        // When / Then
        assert_eq!(method.signature_texts(), ["greet(self, name)"]);
    }

    #[test]
    fn test_domain_object_body_name_extracts_from_signature_for_classes() {
        // Given
        let class = DomainObjectBody::PyClass {
            signatures: NonEmptyVector::single("Greeter".to_string()),
            is_final: false,
            body: vec![],
        };

        // When / Then
        assert_eq!(class.names().as_slice(), ["Greeter"]);
    }

    #[test]
    fn test_domain_object_body_name_ignores_base_class_list() {
        // Given — base classes shouldn't leak into the referenceable name
        let class = DomainObjectBody::PyClass {
            signatures: NonEmptyVector::single("Greeter(Base)".to_string()),
            is_final: false,
            body: vec![],
        };

        // When / Then
        assert_eq!(class.names().as_slice(), ["Greeter"]);
    }

    #[test]
    fn test_domain_object_body_signature_text_shows_full_signature_for_classes() {
        // Given
        let class = DomainObjectBody::PyClass {
            signatures: NonEmptyVector::single("Greeter(Base)".to_string()),
            is_final: true,
            body: vec![],
        };

        // When / Then
        assert_eq!(class.signature_texts(), ["Greeter(Base)"]);
    }

    #[test]
    fn test_domain_object_body_name_extracts_from_signature_for_exceptions() {
        // Given
        let exception = DomainObjectBody::PyException {
            signatures: NonEmptyVector::single("GreeterError".to_string()),
            is_final: false,
            body: vec![],
        };

        // When / Then
        assert_eq!(exception.names().as_slice(), ["GreeterError"]);
    }

    #[test]
    fn test_domain_object_body_name_ignores_base_class_list_for_exceptions() {
        // Given — base classes shouldn't leak into the referenceable name
        let exception = DomainObjectBody::PyException {
            signatures: NonEmptyVector::single("InvalidNameError(GreeterError)".to_string()),
            is_final: false,
            body: vec![],
        };

        // When / Then
        assert_eq!(exception.names().as_slice(), ["InvalidNameError"]);
    }

    #[test]
    fn test_domain_object_body_signature_text_shows_full_signature_for_exceptions() {
        // Given
        let exception = DomainObjectBody::PyException {
            signatures: NonEmptyVector::single("InvalidNameError(GreeterError)".to_string()),
            is_final: true,
            body: vec![],
        };

        // When / Then
        assert_eq!(
            exception.signature_texts(),
            ["InvalidNameError(GreeterError)"]
        );
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
        assert_eq!(module.names().as_slice(), ["mypackage.mymodule"]);
    }

    #[test]
    fn test_domain_object_body_name_uses_bare_name_for_data() {
        // Given
        let data = DomainObjectBody::PyData {
            signatures: NonEmptyVector::single("DEFAULT_TIMEOUT".to_string()),
            type_: None,
            value: None,
            body: vec![],
        };

        // When / Then
        assert_eq!(data.names().as_slice(), ["DEFAULT_TIMEOUT"]);
    }

    #[test]
    fn test_domain_object_body_name_uses_bare_name_for_attributes() {
        // Given
        let attribute = DomainObjectBody::PyAttribute {
            signatures: NonEmptyVector::single("Greeter.name".to_string()),
            type_: None,
            value: None,
            canonical: None,
            body: vec![],
        };

        // When / Then
        assert_eq!(attribute.names().as_slice(), ["Greeter.name"]);
    }

    #[test]
    fn test_domain_object_body_signature_text_shows_full_signature_for_functions() {
        // Given
        let function = DomainObjectBody::CFunction {
            signatures: NonEmptyVector::single("int add(int a, int b)".to_string()),
            body: vec![],
        };

        // When / Then
        assert_eq!(function.signature_texts(), ["int add(int a, int b)"]);
    }

    #[test]
    fn test_domain_object_body_signature_text_shows_full_signature_for_macros() {
        // Given
        let macro_ = DomainObjectBody::CMacro {
            signatures: NonEmptyVector::single("MAX(a, b)".to_string()),
            body: vec![],
        };

        // When / Then
        assert_eq!(macro_.signature_texts(), ["MAX(a, b)"]);
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
        assert_eq!(module.signature_texts(), ["greetings"]);
    }

    #[test]
    fn test_domain_object_body_signature_text_shows_bare_name_for_data() {
        // Given
        let data = DomainObjectBody::PyData {
            signatures: NonEmptyVector::single("DEFAULT_TIMEOUT".to_string()),
            type_: None,
            value: None,
            body: vec![],
        };

        // When / Then
        assert_eq!(data.signature_texts(), ["DEFAULT_TIMEOUT"]);
    }

    #[test]
    fn test_domain_object_body_signature_text_shows_bare_name_for_attributes() {
        // Given
        let attribute = DomainObjectBody::PyAttribute {
            signatures: NonEmptyVector::single("Greeter.name".to_string()),
            type_: None,
            value: None,
            canonical: None,
            body: vec![],
        };

        // When / Then
        assert_eq!(attribute.signature_texts(), ["Greeter.name"]);
    }

    #[test]
    fn test_names_extracts_from_every_signature_not_just_the_first() {
        // Given — a multi-signature function: every entry needs the same
        // name extraction applied, not only the primary.
        let function = DomainObjectBody::PyFunction {
            signatures: NonEmptyVector::new(
                "spawnl(mode, file, *args)".to_string(),
                vec!["spawnle(mode, file, *args, env)".to_string()],
            ),
            body: vec![],
        };

        // When
        let names = function.names();

        // Then
        assert_eq!(names.as_slice(), ["spawnl", "spawnle"]);
    }

    #[test]
    fn test_names_strips_base_class_lists_from_every_signature() {
        // Given
        let class = DomainObjectBody::PyClass {
            signatures: NonEmptyVector::new(
                "Greeter(Base)".to_string(),
                vec!["PoliteGreeter(Greeter)".to_string()],
            ),
            is_final: false,
            body: vec![],
        };

        // When
        let names = class.names();

        // Then
        assert_eq!(names.as_slice(), ["Greeter", "PoliteGreeter"]);
    }

    #[test]
    fn test_names_strips_c_return_types_from_every_signature() {
        // Given
        let function = DomainObjectBody::CFunction {
            signatures: NonEmptyVector::new(
                "int add(int a, int b)".to_string(),
                vec!["PyObject *PyUnicode_FromString(const char *str)".to_string()],
            ),
            body: vec![],
        };

        // When
        let names = function.names();

        // Then
        assert_eq!(names.as_slice(), ["add", "PyUnicode_FromString"]);
    }

    #[test]
    fn test_names_uses_bare_signatures_verbatim_for_data() {
        // Given — the confirmed `library/socket.rst` shape: `py:data`
        // signatures are already bare names, so nothing is extracted.
        let data = DomainObjectBody::PyData {
            signatures: NonEmptyVector::new(
                "AF_UNIX".to_string(),
                vec!["AF_INET".to_string(), "AF_INET6".to_string()],
            ),
            type_: None,
            value: None,
            body: vec![],
        };

        // When
        let names = data.names();

        // Then
        assert_eq!(names.as_slice(), ["AF_UNIX", "AF_INET", "AF_INET6"]);
    }

    #[test]
    fn test_signature_texts_returns_every_signature_unextracted() {
        // Given — the `<dt>` display text keeps the full signature, unlike
        // `names()`.
        let function = DomainObjectBody::PyFunction {
            signatures: NonEmptyVector::new(
                "spawnl(mode, file, *args)".to_string(),
                vec!["spawnle(mode, file, *args, env)".to_string()],
            ),
            body: vec![],
        };

        // When
        let texts = function.signature_texts();

        // Then
        assert_eq!(
            texts,
            [
                "spawnl(mode, file, *args)",
                "spawnle(mode, file, *args, env)"
            ]
        );
    }

    #[test]
    fn test_modules_always_have_exactly_one_name_and_signature_text() {
        // Given — real Sphinx's `module` directive takes exactly one
        // argument, so `PyModule` can never be multi-signature.
        let module = DomainObjectBody::PyModule {
            name: "xml.etree.ElementTree".to_string(),
            platform: None,
            synopsis: None,
            deprecated: false,
            body: vec![],
        };

        // When / Then
        assert_eq!(module.names().as_slice(), ["xml.etree.ElementTree"]);
        assert_eq!(module.signature_texts(), ["xml.etree.ElementTree"]);
    }

    #[test]
    fn test_names_and_signature_texts_stay_index_parallel() {
        // Given — the renderer zips the two to pair each anchor with its
        // display text, so they must have matching lengths and order.
        let method = DomainObjectBody::PyMethod {
            signatures: NonEmptyVector::new(
                "ZipFile.open(name)".to_string(),
                vec!["ZipFile.read(name)".to_string()],
            ),
            is_classmethod: false,
            is_staticmethod: false,
            is_abstractmethod: false,
            is_async: false,
            body: vec![],
        };

        // When
        let names = method.names();
        let texts = method.signature_texts();

        // Then
        assert_eq!(names.as_slice().len(), texts.len());
        assert_eq!(names.as_slice(), ["ZipFile.open", "ZipFile.read"]);
        assert_eq!(texts, ["ZipFile.open(name)", "ZipFile.read(name)"]);
    }

    #[test]
    fn test_domain_object_body_body_returns_shared_body_for_every_variant() {
        // Given
        let paragraph = Node::Paragraph(vec![InlineNode::Text("hello".to_string())]);
        let function = DomainObjectBody::PyFunction {
            signatures: NonEmptyVector::single("greet(name)".to_string()),
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
            signatures: NonEmptyVector::single("DEFAULT_TIMEOUT".to_string()),
            type_: None,
            value: None,
            body: vec![paragraph.clone()],
        };
        let attribute = DomainObjectBody::PyAttribute {
            signatures: NonEmptyVector::single("Greeter.name".to_string()),
            type_: None,
            value: None,
            canonical: None,
            body: vec![paragraph.clone()],
        };
        let c_function = DomainObjectBody::CFunction {
            signatures: NonEmptyVector::single("int add(int a, int b)".to_string()),
            body: vec![paragraph.clone()],
        };
        let c_macro = DomainObjectBody::CMacro {
            signatures: NonEmptyVector::single("MAX(a, b)".to_string()),
            body: vec![paragraph.clone()],
        };
        let method = DomainObjectBody::PyMethod {
            signatures: NonEmptyVector::single("greet(self, name)".to_string()),
            is_classmethod: false,
            is_staticmethod: false,
            is_abstractmethod: false,
            is_async: false,
            body: vec![paragraph.clone()],
        };
        let class = DomainObjectBody::PyClass {
            signatures: NonEmptyVector::single("Greeter".to_string()),
            is_final: false,
            body: vec![paragraph.clone()],
        };
        let exception = DomainObjectBody::PyException {
            signatures: NonEmptyVector::single("GreeterError".to_string()),
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
            signatures: NonEmptyVector::single("foo()".to_string()),
            body: vec![Node::Comment],
        };

        // When
        function.body_mut().push(Node::Comment);

        // Then
        assert_eq!(function.body(), &[Node::Comment, Node::Comment]);
    }
}

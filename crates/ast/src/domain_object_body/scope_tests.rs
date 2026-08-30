use super::*;
use crate::object_naming::build_domain_object_key;

#[test]
fn test_deduce_local_scope_lends_all_new_segments_for_classes() {
    // Given
    let class = DomainObjectBody::PyClass {
        module: None,
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
        module: None,
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
        module: None,
        is_decorator: false,
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
        module: None,
        is_decorator: false,
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
        module: None,
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
        module: None,
        signatures: NonEmptyVector::single("ZipFile.DEFAULT_TIMEOUT".to_string()),
        type_: None,
        value: None,
        body: vec![],
    };

    // When
    let scope = data.deduce_local_scope(&["ZipFile".to_string(), "DEFAULT_TIMEOUT".to_string()]);

    // Then
    assert_eq!(scope, vec!["ZipFile".to_string()]);
}

#[test]
fn test_deduce_local_scope_returns_empty_for_undotted_function() {
    // Given — an unqualified, module-less function has no prefix to lend.
    let function = DomainObjectBody::PyFunction {
        module: None,
        is_decorator: false,
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
        module: None,
        is_decorator: false,
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
            "int PyList_Append(PyObject *list, PyObject *item)".into(),
        ),
        body: vec![],
    };
    let macro_ = DomainObjectBody::CMacro {
        signatures: NonEmptyVector::single("PY_SSIZE_T_MAX".into()),
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

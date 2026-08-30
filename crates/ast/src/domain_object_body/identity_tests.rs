use super::*;

#[test]
fn test_domain_object_body_object_type_matches_variant() {
    // Given / When / Then
    assert_eq!(
        DomainObjectBody::PyFunction {
            module: None,
            is_decorator: false,
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
            module: None,
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
            module: None,
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
            signatures: NonEmptyVector::single("int add(int a, int b)".into()),
            body: vec![],
        }
        .object_type(),
        ObjectType::C(CObjectType::Function)
    );
    assert_eq!(
        DomainObjectBody::CMacro {
            signatures: NonEmptyVector::single("MAX(a, b)".into()),
            body: vec![],
        }
        .object_type(),
        ObjectType::C(CObjectType::Macro)
    );
    assert_eq!(
        DomainObjectBody::PyMethod {
            module: None,
            is_decorator: false,
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
            module: None,
            signatures: NonEmptyVector::single("Greeter".to_string()),
            is_final: false,
            body: vec![],
        }
        .object_type(),
        ObjectType::Py(PyObjectType::Class)
    );
    assert_eq!(
        DomainObjectBody::PyException {
            module: None,
            signatures: NonEmptyVector::single("GreeterError".to_string()),
            is_final: false,
            body: vec![],
        }
        .object_type(),
        ObjectType::Py(PyObjectType::Exception)
    );
}

#[test]
fn test_domain_object_body_object_type_is_function_for_decorator() {
    // Given — a `.. decorator::`-derived `PyFunction`: real Sphinx
    // registers it under the exact same object type as a plain
    // `py:function` (`PyDecoratorFunction.run()` forces
    // `self.name = 'py:function'`), so `is_decorator` must not change
    // `object_type()`.
    let decorator = DomainObjectBody::PyFunction {
        module: None,
        signatures: NonEmptyVector::single("classmethod".to_string()),
        is_decorator: true,
        body: vec![],
    };

    // When / Then
    assert_eq!(
        decorator.object_type(),
        ObjectType::Py(PyObjectType::Function)
    );
}

#[test]
fn test_domain_object_body_object_type_is_method_for_decoratormethod() {
    // Given
    let decorator_method = DomainObjectBody::PyMethod {
        module: None,
        signatures: NonEmptyVector::single("register(cls)".to_string()),
        is_classmethod: false,
        is_staticmethod: false,
        is_abstractmethod: false,
        is_async: false,
        is_decorator: true,
        body: vec![],
    };

    // When / Then
    assert_eq!(
        decorator_method.object_type(),
        ObjectType::Py(PyObjectType::Method)
    );
}

#[test]
fn test_domain_object_body_name_extracts_from_signature_for_functions() {
    // Given
    let function = DomainObjectBody::PyFunction {
        module: None,
        is_decorator: false,
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
        signatures: NonEmptyVector::single("MAX(a, b)".into()),
        body: vec![],
    };

    // When / Then
    assert_eq!(macro_.names().as_slice(), ["MAX"]);
}

#[test]
fn test_domain_object_body_name_uses_bare_signature_for_object_like_macros() {
    // Given
    let macro_ = DomainObjectBody::CMacro {
        signatures: NonEmptyVector::single("PY_SSIZE_T_MAX".into()),
        body: vec![],
    };

    // When / Then
    assert_eq!(macro_.names().as_slice(), ["PY_SSIZE_T_MAX"]);
}

#[test]
fn test_domain_object_body_name_extracts_from_signature_for_methods() {
    // Given
    let method = DomainObjectBody::PyMethod {
        module: None,
        is_decorator: false,
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
        module: None,
        is_decorator: false,
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
        module: None,
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
        module: None,
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
        module: None,
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
        module: None,
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
        module: None,
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
        module: None,
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
        module: None,
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
        module: None,
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
        signatures: NonEmptyVector::single("int add(int a, int b)".into()),
        body: vec![],
    };

    // When / Then
    assert_eq!(function.signature_texts(), ["int add(int a, int b)"]);
}

#[test]
fn test_domain_object_body_signature_text_shows_full_signature_for_macros() {
    // Given
    let macro_ = DomainObjectBody::CMacro {
        signatures: NonEmptyVector::single("MAX(a, b)".into()),
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
        module: None,
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
        module: None,
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
        module: None,
        is_decorator: false,
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
        module: None,
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
            "int add(int a, int b)".into(),
            vec!["PyObject *PyUnicode_FromString(const char *str)".into()],
        ),
        body: vec![],
    };

    // When
    let names = function.names();

    // Then
    assert_eq!(names.as_slice(), ["add", "PyUnicode_FromString"]);
}

#[test]
fn test_names_reads_the_name_parsed_from_a_function_pointer_typedef() {
    // Given — `Doc/c-api/init.rst`'s `Py_tracefunc`, whose name sits
    // inside the `(*…)` group. Reaching it needs the declaration parser;
    // the older "text before the first parenthesis" heuristic reads the
    // return type instead.
    let type_ = DomainObjectBody::CType {
        signatures: NonEmptyVector::single(
            "int (*Py_tracefunc)(PyObject *obj, PyFrameObject *frame, int what, PyObject *arg)"
                .into(),
        ),
        no_index: false,
        no_index_entry: false,
        no_contents_entry: false,
        body: vec![],
    };

    // When
    let names = type_.names();

    // Then
    assert_eq!(names.as_slice(), ["Py_tracefunc"]);
}

#[test]
fn test_names_reads_the_parsed_name_for_every_c_object_type() {
    // Given — one of each `c` variant, all carrying a signature whose
    // name only a real declarator parse recovers.
    let function = DomainObjectBody::CFunction {
        signatures: NonEmptyVector::single("PyObject *(*getattrofunc)(PyObject *)".into()),
        body: vec![],
    };
    let macro_ = DomainObjectBody::CMacro {
        signatures: NonEmptyVector::single("void (*freefunc)(void *)".into()),
        body: vec![],
    };
    let struct_ = DomainObjectBody::CStruct {
        signatures: NonEmptyVector::single("int (*inquiry)(PyObject *)".into()),
        no_index: false,
        no_index_entry: false,
        no_contents_entry: false,
        body: vec![],
    };
    let union_ = DomainObjectBody::CUnion {
        signatures: NonEmptyVector::single("Py_ssize_t (*lenfunc)(PyObject *)".into()),
        no_index: false,
        no_index_entry: false,
        no_contents_entry: false,
        body: vec![],
    };
    let member = DomainObjectBody::CMember {
        signatures: NonEmptyVector::single("int (*visitproc)(PyObject *o, void *arg)".into()),
        no_index: false,
        no_index_entry: false,
        no_contents_entry: false,
        body: vec![],
    };
    let type_ = DomainObjectBody::CType {
        signatures: NonEmptyVector::single("PyObject *(*unaryfunc)(PyObject *)".into()),
        no_index: false,
        no_index_entry: false,
        no_contents_entry: false,
        body: vec![],
    };

    // When / Then
    assert_eq!(function.names().as_slice(), ["getattrofunc"]);
    assert_eq!(macro_.names().as_slice(), ["freefunc"]);
    assert_eq!(struct_.names().as_slice(), ["inquiry"]);
    assert_eq!(union_.names().as_slice(), ["lenfunc"]);
    assert_eq!(member.names().as_slice(), ["visitproc"]);
    assert_eq!(type_.names().as_slice(), ["unaryfunc"]);
}

#[test]
fn test_signature_texts_returns_c_signatures_as_written() {
    // Given — the rendered `<dt>` shows the whole declaration, so the
    // stored text must survive the name extraction untouched.
    let type_ = DomainObjectBody::CType {
        signatures: NonEmptyVector::new(
            "int (*Py_tracefunc)(PyObject *obj, int what)".into(),
            vec!["unsigned long ulong".into()],
        ),
        no_index: false,
        no_index_entry: false,
        no_contents_entry: false,
        body: vec![],
    };

    // When
    let texts = type_.signature_texts();

    // Then
    assert_eq!(
        texts,
        [
            "int (*Py_tracefunc)(PyObject *obj, int what)",
            "unsigned long ulong"
        ]
    );
}

#[test]
fn test_names_and_signature_texts_stay_index_parallel_for_c_objects() {
    // Given — a multi-signature `c:type`, where each name is derived
    // independently of its neighbours.
    let type_ = DomainObjectBody::CType {
        signatures: NonEmptyVector::new(
            "int (*Py_tracefunc)(PyObject *obj)".into(),
            vec!["unsigned long ulong".into(), "FILE".into()],
        ),
        no_index: false,
        no_index_entry: false,
        no_contents_entry: false,
        body: vec![],
    };

    // When
    let names = type_.names();
    let texts = type_.signature_texts();

    // Then
    assert_eq!(names.as_slice().len(), texts.len());
    assert_eq!(names.as_slice(), ["Py_tracefunc", "ulong", "FILE"]);
}

#[test]
fn test_names_uses_bare_signatures_verbatim_for_data() {
    // Given — the confirmed `library/socket.rst` shape: `py:data`
    // signatures are already bare names, so nothing is extracted.
    let data = DomainObjectBody::PyData {
        module: None,
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
        module: None,
        is_decorator: false,
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
        module: None,
        is_decorator: false,
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

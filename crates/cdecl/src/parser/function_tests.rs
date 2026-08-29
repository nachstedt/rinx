use super::*;

/// The declared name, for the many tests that only care about that.
fn name_of(signature: &str) -> String {
    declared_name(signature).unwrap_or_else(|error| panic!("{signature:?}: {error}"))
}

// ── Functions ─────────────────────────────────────────────────────────

#[test]
fn test_parses_a_function_with_no_parameters() {
    // Given
    let signature = "int f()";

    // When
    let declaration = parse_declaration(signature).expect("should parse");

    // Then
    let Declarator::Function { parameters, .. } = &declaration.declarator else {
        panic!("expected a function declarator");
    };
    assert!(parameters.is_empty());
    assert_eq!(declaration.name(), Some("f"));
}

#[test]
fn test_normalises_a_void_parameter_list_to_no_parameters() {
    // Given — `(void)` means "takes nothing", not "takes one void".
    let signature = "int f(void)";

    // When
    let declaration = parse_declaration(signature).expect("should parse");

    // Then
    let Declarator::Function { parameters, .. } = &declaration.declarator else {
        panic!("expected a function declarator");
    };
    assert!(parameters.is_empty());
}

#[test]
fn test_parses_a_function_with_named_parameters() {
    // Given
    let signature = "int f(int a, int b)";

    // When
    let declaration = parse_declaration(signature).expect("should parse");

    // Then
    let Declarator::Function { parameters, .. } = &declaration.declarator else {
        panic!("expected a function declarator");
    };
    assert_eq!(parameters.len(), 2);
    assert_eq!(parameters[0].name(), Some("a"));
    assert_eq!(parameters[1].name(), Some("b"));
}

#[test]
fn test_parses_a_varargs_function() {
    // Given
    let signature = "int f(const char *fmt, ...)";

    // When
    let declaration = parse_declaration(signature).expect("should parse");

    // Then
    let Declarator::Function {
        parameters,
        varargs,
        ..
    } = &declaration.declarator
    else {
        panic!("expected a function declarator");
    };
    assert!(varargs);
    assert_eq!(parameters.len(), 1);
    assert_eq!(parameters[0].name(), Some("fmt"));
}

#[test]
fn test_parses_an_unnamed_parameter_as_abstract() {
    // Given — parameter types with no names are legal and common.
    let signature = "int f(PyObject *)";

    // When
    let declaration = parse_declaration(signature).expect("should parse");

    // Then
    let Declarator::Function { parameters, .. } = &declaration.declarator else {
        panic!("expected a function declarator");
    };
    assert_eq!(parameters.len(), 1);
    assert_eq!(parameters[0].name(), None);
}

#[test]
fn test_parses_a_pointer_return_type() {
    // Given — the `*` binds to the return type, and the heuristic this
    // crate replaces already handled this one.
    let signature = "PyObject *PyUnicode_FromString(const char *str)";

    // When / Then
    assert_eq!(name_of(signature), "PyUnicode_FromString");
}

// ── Grouped declarators ───────────────────────────────────────────────

#[test]
fn test_parses_a_function_pointer_typedef() {
    // Given — the shape that motivated this crate: the name is inside the
    // `(*…)` group, not before the first parenthesis.
    let signature = "int (*type_name)(int arg1, int arg2)";

    // When / Then
    assert_eq!(name_of(signature), "type_name");
}

#[test]
fn test_parses_a_function_pointer_with_a_pointer_return_type() {
    // Given — `c-api/typeobj.rst`'s slot typedefs take this shape.
    let signature = "PyObject *(*unaryfunc)(PyObject *)";

    // When / Then
    assert_eq!(name_of(signature), "unaryfunc");
}

#[test]
fn test_parses_a_pointer_to_function_pointer() {
    // Given
    let signature = "int (**fp)(void)";

    // When / Then
    assert_eq!(name_of(signature), "fp");
}

#[test]
fn test_parses_a_pointer_to_array() {
    // Given
    let signature = "int (*p)[10]";

    // When / Then
    assert_eq!(name_of(signature), "p");
}

#[test]
fn test_parses_a_function_returning_a_function_pointer() {
    // Given — the textbook worst case: `signal` takes a function pointer
    // and returns one.
    let signature = "void (*signal(int, void (*)(int)))(int)";

    // When / Then
    assert_eq!(name_of(signature), "signal");
}

#[test]
fn test_parses_a_pointer_to_array_of_function_pointers() {
    // Given
    let signature = "int (*(*x)[5])(void)";

    // When / Then
    assert_eq!(name_of(signature), "x");
}

#[test]
fn test_builds_a_function_pointer_tree_outermost_first() {
    // Given — structure, not just the name: `int (*f)(int)` is a pointer
    // to a function, so the function suffix must wrap the pointer.
    let signature = "int (*f)(int)";

    // When
    let declaration = parse_declaration(signature).expect("should parse");

    // Then
    let Declarator::Function { inner, .. } = &declaration.declarator else {
        panic!("expected the outermost declarator to be a function");
    };
    let Declarator::Pointer { inner, .. } = inner.as_ref() else {
        panic!("expected a pointer inside the function");
    };
    assert_eq!(inner.as_ref(), &Declarator::Name("f".to_string()));
}

// ── Documentation-specific trailing forms ─────────────────────────────

#[test]
fn test_parses_a_member_bitfield_width() {
    // Given
    let signature = "unsigned int flags : 3";

    // When
    let declaration = parse_declaration(signature).expect("should parse");

    // Then
    assert_eq!(declaration.bitfield_width.as_deref(), Some("3"));
    assert_eq!(declaration.name(), Some("flags"));
}

#[test]
fn test_parses_a_numeric_initialiser() {
    // Given
    let signature = "int x = 5";

    // When
    let declaration = parse_declaration(signature).expect("should parse");

    // Then
    assert_eq!(declaration.initializer.as_deref(), Some("5"));
    assert_eq!(declaration.name(), Some("x"));
}

#[test]
fn test_parses_a_pointer_initialiser() {
    // Given
    let signature = "PyObject *o = NULL";

    // When
    let declaration = parse_declaration(signature).expect("should parse");

    // Then
    assert_eq!(declaration.initializer.as_deref(), Some("NULL"));
    assert_eq!(declaration.name(), Some("o"));
}

#[test]
fn test_skips_a_leading_attribute() {
    // Given
    let signature = "__attribute__((noreturn)) void f(void)";

    // When / Then
    assert_eq!(name_of(signature), "f");
}

#[test]
fn test_parses_a_function_like_macro_signature() {
    // Given — `.. c:macro:: MAX(a, b)`: no return type, so `MAX` is the
    // name and the parenthesis opens its parameter list.
    let signature = "MAX(a, b)";

    // When / Then
    assert_eq!(name_of(signature), "MAX");
}

#[test]
fn test_parses_an_object_like_macro_signature() {
    // Given
    let signature = "PY_SSIZE_T_MAX";

    // When / Then
    assert_eq!(name_of(signature), "PY_SSIZE_T_MAX");
}

// ── Failure and robustness ────────────────────────────────────────────

#[test]
fn test_rejects_empty_input() {
    // Given
    let signature = "";

    // When
    let result = parse_declaration(signature);

    // Then
    assert_eq!(
        result.unwrap_err().message,
        "empty declaration".to_string(),
        "empty input has nothing to fall back on"
    );
}

#[test]
fn test_rejects_whitespace_only_input() {
    // Given
    let signature = "   \t  ";

    // When / Then
    assert!(parse_declaration(signature).is_err());
}

#[test]
fn test_rejects_a_declaration_with_no_declared_name() {
    // Given — parses fine as a type, but declares nothing.
    let signature = "int";

    // When
    let result = declared_name(signature);

    // Then
    assert_eq!(result.unwrap_err().message, "declaration declares no name");
}

#[test]
fn test_rejects_an_unbalanced_parenthesis() {
    // Given
    let signature = "int (*x(";

    // When / Then
    assert!(parse_declaration(signature).is_err());
}

#[test]
fn test_rejects_an_unterminated_array() {
    // Given
    let signature = "int x[";

    // When / Then
    assert!(parse_declaration(signature).is_err());
}

#[test]
fn test_rejects_a_stray_closing_parenthesis() {
    // Given
    let signature = ")";

    // When
    let error = parse_declaration(signature).unwrap_err();

    // Then
    assert_eq!(error.offset, 0);
}

#[test]
fn test_rejects_trailing_junk_after_a_complete_declaration() {
    // Given
    let signature = "int x ??";

    // When
    let error = parse_declaration(signature).unwrap_err();

    // Then
    assert!(error.message.starts_with("unexpected"));
    assert_eq!(error.offset, 6);
}

#[test]
fn test_rejects_prose_that_is_not_a_declaration() {
    // Given
    let signature = ">>> not a declaration <<<";

    // When / Then
    assert!(parse_declaration(signature).is_err());
}

#[test]
fn test_rejects_declarators_nested_beyond_the_depth_bound() {
    // Given — deeper than `MAX_DECLARATOR_DEPTH` groups.
    let signature = format!("int {}x{}", "(*".repeat(200), ")".repeat(200));

    // When
    let error = parse_declaration(&signature).unwrap_err();

    // Then — a bounded error rather than a blown stack.
    assert_eq!(error.message, "declarator nested too deeply");
}

#[test]
fn test_never_panics_on_truncations_or_deletions_of_real_signatures() {
    // Given — every prefix and every single-character deletion of a set
    // of real signatures, which is where a hand-rolled parser is most
    // likely to walk off the end of its token stream.
    let signatures = [
        "int (*Py_tracefunc)(PyObject *obj, PyFrameObject *frame, int what, PyObject *arg)",
        "PyObject *(*unaryfunc)(PyObject *)",
        "void (*signal(int, void (*)(int)))(int)",
        "unsigned int flags : 3",
        "int x[static 4] = {0}",
        "__attribute__((noreturn)) void f(void)",
    ];

    // When / Then — the assertion is simply that each call returns.
    for signature in signatures {
        for end in 0..=signature.len() {
            if signature.is_char_boundary(end) {
                let _ = parse_declaration(&signature[..end]);
            }
        }
        for skip in 0..signature.len() {
            let mutated: String = signature
                .chars()
                .enumerate()
                .filter(|&(index, _)| index != skip)
                .map(|(_, character)| character)
                .collect();
            let _ = parse_declaration(&mutated);
        }
    }
}

// ── Real CPython signatures ───────────────────────────────────────────

#[test]
fn test_parses_cpython_py_tracefunc_from_c_api_init() {
    // Given — `Doc/c-api/init.rst`; referenced 8 times from
    // `Doc/c-api/profiling.rst` and unresolvable until now.
    let signature =
        "int (*Py_tracefunc)(PyObject *obj, PyFrameObject *frame, int what, PyObject *arg)";

    // When / Then
    assert_eq!(name_of(signature), "Py_tracefunc");
}

#[test]
fn test_parses_cpython_slot_typedefs_from_c_api_typeobj() {
    // Given — `Doc/c-api/typeobj.rst` declares dozens of these.
    let cases = [
        ("PyObject *(*unaryfunc)(PyObject *)", "unaryfunc"),
        (
            "PyObject *(*binaryfunc)(PyObject *, PyObject *)",
            "binaryfunc",
        ),
        (
            "PyObject *(*ternaryfunc)(PyObject *, PyObject *, PyObject *)",
            "ternaryfunc",
        ),
        ("int (*inquiry)(PyObject *)", "inquiry"),
        ("Py_ssize_t (*lenfunc)(PyObject *)", "lenfunc"),
        ("int (*visitproc)(PyObject *object, void *arg)", "visitproc"),
        (
            "int (*traverseproc)(PyObject *self, visitproc visit, void *arg)",
            "traverseproc",
        ),
        ("Py_hash_t (*hashfunc)(PyObject *)", "hashfunc"),
        (
            "PyObject *(*richcmpfunc)(PyObject *, PyObject *, int)",
            "richcmpfunc",
        ),
        ("PyObject *(*getiterfunc)(PyObject *)", "getiterfunc"),
        (
            "PyObject *(*descrgetfunc)(PyObject *, PyObject *, PyObject *)",
            "descrgetfunc",
        ),
        (
            "int (*initproc)(PyObject *, PyObject *, PyObject *)",
            "initproc",
        ),
        (
            "PyObject *(*newfunc)(PyTypeObject *, PyObject *, PyObject *)",
            "newfunc",
        ),
        ("void (*destructor)(PyObject *)", "destructor"),
    ];

    // When / Then
    for (signature, expected) in cases {
        assert_eq!(name_of(signature), expected, "for {signature:?}");
    }
}

#[test]
fn test_parses_cpython_member_and_macro_signatures() {
    // Given — the non-function C object types.
    let cases = [
        ("Py_ssize_t ob_refcnt", "ob_refcnt"),
        ("PyTypeObject *ob_type", "ob_type"),
        ("PY_SSIZE_T_MAX", "PY_SSIZE_T_MAX"),
        ("Py_RETURN_NONE", "Py_RETURN_NONE"),
    ];

    // When / Then
    for (signature, expected) in cases {
        assert_eq!(name_of(signature), expected, "for {signature:?}");
    }
}

// ── Helpers ───────────────────────────────────────────────────────────

#[test]
fn test_is_void_parameter_list_recognises_only_a_lone_unnamed_void() {
    // Given
    let void_parameter = Declaration {
        specifiers: vec!["void".to_string()],
        declarator: Declarator::Abstract,
        initializer: None,
        bitfield_width: None,
    };
    let named_void_parameter = Declaration {
        declarator: Declarator::Name("v".to_string()),
        ..void_parameter.clone()
    };
    let int_parameter = Declaration {
        specifiers: vec!["int".to_string()],
        ..void_parameter.clone()
    };

    // When / Then
    assert!(is_void_parameter_list(std::slice::from_ref(
        &void_parameter
    )));
    assert!(!is_void_parameter_list(&[]));
    assert!(!is_void_parameter_list(&[named_void_parameter]));
    assert!(!is_void_parameter_list(&[int_parameter]));
    assert!(!is_void_parameter_list(&[
        void_parameter.clone(),
        void_parameter
    ]));
}

#[test]
fn test_parse_error_displays_message_and_offset() {
    // Given
    let error = ParseError {
        message: "expected ')'".to_string(),
        offset: 7,
    };

    // When / Then
    assert_eq!(error.to_string(), "expected ')' at offset 7");
}

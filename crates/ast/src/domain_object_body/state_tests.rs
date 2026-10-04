use super::*;
use crate::inline_node::InlineNode;

#[test]
fn test_domain_object_body_body_returns_shared_body_for_every_variant() {
    // Given
    let paragraph = Node::Paragraph(vec![InlineNode::Text("hello".to_string())]);
    let function = DomainObjectBody::PyFunction {
        is_async: false,
        flags: crate::DescriptionFlags::default(),
        module: None,
        is_decorator: false,
        signatures: NonEmptyVector::single("greet(name)".to_string()),
        body: vec![paragraph.clone()],
    };
    let module = DomainObjectBody::PyModule {
        name: "greetings".to_string(),
        options: crate::ModuleOptions::default(),
        body: vec![paragraph.clone()],
    };
    let data = DomainObjectBody::PyData {
        flags: crate::DescriptionFlags::default(),
        module: None,
        signatures: NonEmptyVector::single("DEFAULT_TIMEOUT".to_string()),
        type_: None,
        value: None,
        body: vec![paragraph.clone()],
    };
    let attribute = DomainObjectBody::PyAttribute {
        flags: crate::DescriptionFlags::default(),
        module: None,
        signatures: NonEmptyVector::single("Greeter.name".to_string()),
        type_: None,
        value: None,
        canonical: None,
        body: vec![paragraph.clone()],
    };
    let c_function = DomainObjectBody::CFunction {
        flags: crate::DescriptionFlags::default(),
        signatures: NonEmptyVector::single("int add(int a, int b)".into()),
        body: vec![paragraph.clone()],
    };
    let c_macro = DomainObjectBody::CMacro {
        flags: crate::DescriptionFlags::default(),
        signatures: NonEmptyVector::single("MAX(a, b)".into()),
        body: vec![paragraph.clone()],
    };
    let method = DomainObjectBody::PyMethod {
        flags: crate::DescriptionFlags::default(),
        module: None,
        is_decorator: false,
        signatures: NonEmptyVector::single("greet(self, name)".to_string()),
        is_classmethod: false,
        is_staticmethod: false,
        is_abstractmethod: false,
        is_async: false,
        body: vec![paragraph.clone()],
    };
    let class = DomainObjectBody::PyClass {
        flags: crate::DescriptionFlags::default(),
        module: None,
        signatures: NonEmptyVector::single("Greeter".to_string()),
        is_final: false,
        body: vec![paragraph.clone()],
    };
    let exception = DomainObjectBody::PyException {
        flags: crate::DescriptionFlags::default(),
        module: None,
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
        is_async: false,
        flags: crate::DescriptionFlags::default(),
        module: None,
        is_decorator: false,
        signatures: NonEmptyVector::single("foo()".to_string()),
        body: vec![Node::Comment],
    };

    // When
    function.body_mut().push(Node::Comment);

    // Then
    assert_eq!(function.body(), &[Node::Comment, Node::Comment]);
}

fn plain_c_member(signature: &str) -> DomainObjectBody {
    DomainObjectBody::CMember {
        signatures: NonEmptyVector::single(signature.into()),
        flags: crate::DescriptionFlags::default(),
        body: vec![],
    }
}

fn plain_c_struct(signature: &str) -> DomainObjectBody {
    DomainObjectBody::CStruct {
        signatures: NonEmptyVector::single(signature.into()),
        flags: crate::DescriptionFlags::default(),
        body: vec![],
    }
}

fn plain_c_union(signature: &str) -> DomainObjectBody {
    DomainObjectBody::CUnion {
        signatures: NonEmptyVector::single(signature.into()),
        flags: crate::DescriptionFlags::default(),
        body: vec![],
    }
}

fn plain_c_type(signature: &str) -> DomainObjectBody {
    DomainObjectBody::CType {
        signatures: NonEmptyVector::single(signature.into()),
        flags: crate::DescriptionFlags::default(),
        body: vec![],
    }
}

#[test]
fn test_domain_object_body_object_type_for_c_struct_union_member() {
    // Given / When / Then
    assert_eq!(
        plain_c_struct("Data").object_type(),
        ObjectType::C(CObjectType::Struct)
    );
    assert_eq!(
        plain_c_union("Number").object_type(),
        ObjectType::C(CObjectType::Union)
    );
    assert_eq!(
        plain_c_member("count").object_type(),
        ObjectType::C(CObjectType::Member)
    );
}

#[test]
fn test_domain_object_body_name_uses_bare_signature_for_c_struct() {
    // Given — struct/union tags have no parens, like object-like macros.
    let struct_ = plain_c_struct("Data");

    // When / Then
    assert_eq!(struct_.names().as_slice(), ["Data"]);
}

#[test]
fn test_domain_object_body_name_uses_bare_signature_for_c_union() {
    // Given
    let union_ = plain_c_union("Number");

    // When / Then
    assert_eq!(union_.names().as_slice(), ["Number"]);
}

#[test]
fn test_domain_object_body_name_extracts_dotted_flat_name_for_c_member() {
    // Given — the real CPython-docs shape: a member declared with its
    // enclosing struct's name baked into the signature, no `.. c:struct::`
    // wrapper at all.
    let member = plain_c_member("PyObject *PyTypeObject.tp_bases");

    // When / Then
    assert_eq!(member.names().as_slice(), ["PyTypeObject.tp_bases"]);
}

#[test]
fn test_domain_object_body_name_uses_bare_name_for_undotted_c_member() {
    // Given — a member written bare, e.g. nested inside `.. c:struct::`.
    let member = plain_c_member("int count");

    // When / Then
    assert_eq!(member.names().as_slice(), ["count"]);
}

#[test]
fn test_domain_object_body_signature_text_shows_full_signature_for_c_struct_union_member() {
    // Given
    let struct_ = plain_c_struct("Data");
    let union_ = plain_c_union("Number");
    let member = plain_c_member("PyObject *PyTypeObject.tp_bases");

    // When / Then
    assert_eq!(struct_.signature_texts(), ["Data"]);
    assert_eq!(union_.signature_texts(), ["Number"]);
    assert_eq!(
        member.signature_texts(),
        ["PyObject *PyTypeObject.tp_bases"]
    );
}

#[test]
fn test_deduce_local_scope_lends_all_new_segments_for_c_struct_and_union() {
    // Given — real lexical nesting, like `py:class`.
    let struct_ = plain_c_struct("Data");
    let union_ = plain_c_union("Number");

    // When / Then
    assert_eq!(
        struct_.deduce_local_scope(&["Data".to_string()]),
        vec!["Data".to_string()]
    );
    assert_eq!(
        union_.deduce_local_scope(&["Number".to_string()]),
        vec!["Number".to_string()]
    );
}

#[test]
fn test_deduce_local_scope_returns_empty_for_c_member() {
    // Given — nothing nests under a `c:member` in real Sphinx.
    let member = plain_c_member("count");

    // When
    let scope = member.deduce_local_scope(&["count".to_string()]);

    // Then
    assert!(scope.is_empty());
}

#[test]
fn test_domain_object_body_body_returns_shared_body_for_c_struct_union_member() {
    // Given
    let paragraph = Node::Paragraph(vec![InlineNode::Text("hello".to_string())]);
    let struct_ = DomainObjectBody::CStruct {
        signatures: NonEmptyVector::single("Data".into()),
        flags: crate::DescriptionFlags::default(),
        body: vec![paragraph.clone()],
    };
    let union_ = DomainObjectBody::CUnion {
        signatures: NonEmptyVector::single("Number".into()),
        flags: crate::DescriptionFlags::default(),
        body: vec![paragraph.clone()],
    };
    let member = DomainObjectBody::CMember {
        signatures: NonEmptyVector::single("count".into()),
        flags: crate::DescriptionFlags::default(),
        body: vec![paragraph.clone()],
    };

    // When / Then
    assert_eq!(struct_.body(), std::slice::from_ref(&paragraph));
    assert_eq!(union_.body(), std::slice::from_ref(&paragraph));
    assert_eq!(member.body(), std::slice::from_ref(&paragraph));
}

#[test]
fn test_no_index_is_false_by_default_for_pre_existing_variants() {
    // Given / When / Then
    assert!(
        !DomainObjectBody::CFunction {
            flags: crate::DescriptionFlags::default(),
            signatures: NonEmptyVector::single("int add(int a, int b)".into()),
            body: vec![],
        }
        .no_index()
    );
}

#[test]
fn test_no_index_reflects_flag_for_c_member() {
    // Given
    let mut member = plain_c_member("count");

    // When / Then
    assert!(!member.no_index());
    if let DomainObjectBody::CMember { flags, .. } = &mut member {
        flags.set(crate::DescriptionFlag::NoIndex);
    }
    assert!(member.no_index());
}

#[test]
fn test_no_index_entry_is_true_when_no_index_entry_flag_set() {
    // Given
    let member = DomainObjectBody::CMember {
        signatures: NonEmptyVector::single("count".into()),
        flags: crate::DescriptionFlags::of([crate::DescriptionFlag::NoIndexEntry]),
        body: vec![],
    };

    // When / Then
    assert!(member.no_index_entry());
}

#[test]
fn test_no_index_entry_is_implied_by_no_index() {
    // Given — real Sphinx's `no-index` implies `no-index-entry`.
    let member = DomainObjectBody::CMember {
        signatures: NonEmptyVector::single("count".into()),
        flags: crate::DescriptionFlags::of([crate::DescriptionFlag::NoIndex]),
        body: vec![],
    };

    // When / Then
    assert!(member.no_index_entry());
}

#[test]
fn test_domain_object_body_object_type_for_c_type() {
    // Given / When / Then
    assert_eq!(
        plain_c_type("PyMemAllocatorDomain").object_type(),
        ObjectType::C(CObjectType::Type)
    );
}

#[test]
fn test_domain_object_body_name_uses_bare_signature_for_c_type() {
    // Given — bare typedef alias, no parens.
    let type_ = plain_c_type("PyMemAllocatorDomain");

    // When / Then
    assert_eq!(type_.names().as_slice(), ["PyMemAllocatorDomain"]);
}

#[test]
fn test_domain_object_body_name_extracts_from_two_token_typedef_signature() {
    // Given — real Sphinx's `type name` typedef-alias form.
    let type_ = plain_c_type("unsigned long ulong");

    // When / Then
    assert_eq!(type_.names().as_slice(), ["ulong"]);
}

#[test]
fn test_domain_object_body_signature_text_shows_full_signature_for_c_type() {
    // Given
    let type_ = plain_c_type("unsigned long ulong");

    // When / Then
    assert_eq!(type_.signature_texts(), ["unsigned long ulong"]);
}

#[test]
fn test_deduce_local_scope_lends_all_new_segments_for_c_type() {
    // Given — nesting under `c:type` scope-qualifies exactly like
    // `c:struct`/`c:union`, confirmed against real Sphinx's C-domain
    // docs (nesting is generic to any declaration, not struct/union
    // specific).
    let type_ = plain_c_type("PyMemAllocatorDomain");

    // When
    let scope = type_.deduce_local_scope(&["PyMemAllocatorDomain".to_string()]);

    // Then
    assert_eq!(scope, vec!["PyMemAllocatorDomain".to_string()]);
}

#[test]
fn test_domain_object_body_body_returns_shared_body_for_c_type() {
    // Given
    let paragraph = Node::Paragraph(vec![InlineNode::Text("hello".to_string())]);
    let type_ = DomainObjectBody::CType {
        signatures: NonEmptyVector::single("PyMemAllocatorDomain".into()),
        flags: crate::DescriptionFlags::default(),
        body: vec![paragraph.clone()],
    };

    // When / Then
    assert_eq!(type_.body(), std::slice::from_ref(&paragraph));
}

#[test]
fn test_no_index_reflects_flag_for_c_type() {
    // Given
    let mut type_ = plain_c_type("PyMemAllocatorDomain");

    // When / Then
    assert!(!type_.no_index());
    if let DomainObjectBody::CType { flags, .. } = &mut type_ {
        flags.set(crate::DescriptionFlag::NoIndex);
    }
    assert!(type_.no_index());
}

#[test]
fn test_no_index_entry_is_true_when_no_index_entry_flag_set_for_c_type() {
    // Given
    let type_ = DomainObjectBody::CType {
        signatures: NonEmptyVector::single("PyMemAllocatorDomain".into()),
        flags: crate::DescriptionFlags::of([crate::DescriptionFlag::NoIndexEntry]),
        body: vec![],
    };

    // When / Then
    assert!(type_.no_index_entry());
}

#[test]
fn test_no_index_entry_is_implied_by_no_index_for_c_type() {
    // Given
    let type_ = DomainObjectBody::CType {
        signatures: NonEmptyVector::single("PyMemAllocatorDomain".into()),
        flags: crate::DescriptionFlags::of([crate::DescriptionFlag::NoIndex]),
        body: vec![],
    };

    // When / Then
    assert!(type_.no_index_entry());
}

#[test]
fn test_no_contents_entry_reflects_flag_for_c_type() {
    // Given
    let type_with_flag = DomainObjectBody::CType {
        signatures: NonEmptyVector::single("PyMemAllocatorDomain".into()),
        flags: crate::DescriptionFlags::of([crate::DescriptionFlag::NoContentsEntry]),
        body: vec![],
    };

    // When / Then
    assert!(type_with_flag.no_contents_entry());
    assert!(!plain_c_type("PyMemAllocatorDomain").no_contents_entry());
}

#[test]
fn test_no_contents_entry_reflects_flag_for_c_struct() {
    // Given
    let struct_with_flag = DomainObjectBody::CStruct {
        signatures: NonEmptyVector::single("Data".into()),
        flags: crate::DescriptionFlags::of([crate::DescriptionFlag::NoContentsEntry]),
        body: vec![],
    };

    // When / Then
    assert!(struct_with_flag.no_contents_entry());
    assert!(!plain_c_struct("Data").no_contents_entry());
}

#[test]
fn test_module_override_is_none_by_default_for_every_py_variant_that_carries_it() {
    // Given / When / Then
    assert_eq!(
        DomainObjectBody::PyFunction {
            is_async: false,
            flags: crate::DescriptionFlags::default(),
            module: None,
            is_decorator: false,
            signatures: NonEmptyVector::single("greet(name)".to_string()),
            body: vec![],
        }
        .module_override(),
        None
    );
    assert_eq!(
        DomainObjectBody::PyMethod {
            flags: crate::DescriptionFlags::default(),
            module: None,
            is_decorator: false,
            signatures: NonEmptyVector::single("greet(self, name)".to_string()),
            is_classmethod: false,
            is_staticmethod: false,
            is_abstractmethod: false,
            is_async: false,
            body: vec![],
        }
        .module_override(),
        None
    );
    assert_eq!(
        DomainObjectBody::PyClass {
            flags: crate::DescriptionFlags::default(),
            module: None,
            signatures: NonEmptyVector::single("Greeter".to_string()),
            is_final: false,
            body: vec![],
        }
        .module_override(),
        None
    );
    assert_eq!(
        DomainObjectBody::PyException {
            flags: crate::DescriptionFlags::default(),
            module: None,
            signatures: NonEmptyVector::single("GreeterError".to_string()),
            is_final: false,
            body: vec![],
        }
        .module_override(),
        None
    );
    assert_eq!(
        DomainObjectBody::PyData {
            flags: crate::DescriptionFlags::default(),
            module: None,
            signatures: NonEmptyVector::single("DEFAULT_TIMEOUT".to_string()),
            type_: None,
            value: None,
            body: vec![],
        }
        .module_override(),
        None
    );
    assert_eq!(
        DomainObjectBody::PyAttribute {
            flags: crate::DescriptionFlags::default(),
            module: None,
            signatures: NonEmptyVector::single("Greeter.name".to_string()),
            type_: None,
            value: None,
            canonical: None,
            body: vec![],
        }
        .module_override(),
        None
    );
}

#[test]
fn test_module_override_reflects_the_field_for_every_py_variant_that_carries_it() {
    // Given / When / Then
    assert_eq!(
        DomainObjectBody::PyFunction {
            is_async: false,
            flags: crate::DescriptionFlags::default(),
            module: Some("ctypes.util".to_string()),
            is_decorator: false,
            signatures: NonEmptyVector::single("find_library(name)".to_string()),
            body: vec![],
        }
        .module_override(),
        Some("ctypes.util")
    );
    assert_eq!(
        DomainObjectBody::PyMethod {
            flags: crate::DescriptionFlags::default(),
            module: Some("multiprocessing.managers".to_string()),
            is_decorator: false,
            signatures: NonEmptyVector::single("get_server()".to_string()),
            is_classmethod: false,
            is_staticmethod: false,
            is_abstractmethod: false,
            is_async: false,
            body: vec![],
        }
        .module_override(),
        Some("multiprocessing.managers")
    );
    assert_eq!(
        DomainObjectBody::PyClass {
            flags: crate::DescriptionFlags::default(),
            module: Some("multiprocessing.managers".to_string()),
            signatures: NonEmptyVector::single("SharedMemoryManager".to_string()),
            is_final: false,
            body: vec![],
        }
        .module_override(),
        Some("multiprocessing.managers")
    );
    assert_eq!(
        DomainObjectBody::PyException {
            flags: crate::DescriptionFlags::default(),
            module: Some("mymodule.other".to_string()),
            signatures: NonEmptyVector::single("GreeterError".to_string()),
            is_final: false,
            body: vec![],
        }
        .module_override(),
        Some("mymodule.other")
    );
    assert_eq!(
        DomainObjectBody::PyData {
            flags: crate::DescriptionFlags::default(),
            module: Some("ctypes.util".to_string()),
            signatures: NonEmptyVector::single("DEFAULT_TIMEOUT".to_string()),
            type_: None,
            value: None,
            body: vec![],
        }
        .module_override(),
        Some("ctypes.util")
    );
    assert_eq!(
        DomainObjectBody::PyAttribute {
            flags: crate::DescriptionFlags::default(),
            module: Some("mymodule.other".to_string()),
            signatures: NonEmptyVector::single("Greeter.name".to_string()),
            type_: None,
            value: None,
            canonical: None,
            body: vec![],
        }
        .module_override(),
        Some("mymodule.other")
    );
}

#[test]
fn test_module_override_is_always_none_for_py_module_and_every_c_variant() {
    // Given — `py:module` is not a `PyObject` and has no `:module:`
    // option (it *is* the module declaration); no `c`-domain object
    // carries the option either.
    let module = DomainObjectBody::PyModule {
        name: "greetings".to_string(),
        options: crate::ModuleOptions::default(),
        body: vec![],
    };

    // When / Then
    assert_eq!(module.module_override(), None);
    assert_eq!(
        DomainObjectBody::CFunction {
            flags: crate::DescriptionFlags::default(),
            signatures: NonEmptyVector::single("int add(int a, int b)".into()),
            body: vec![],
        }
        .module_override(),
        None
    );
    assert_eq!(
        DomainObjectBody::CMacro {
            flags: crate::DescriptionFlags::default(),
            signatures: NonEmptyVector::single("MAX(a, b)".into()),
            body: vec![],
        }
        .module_override(),
        None
    );
    assert_eq!(plain_c_struct("Data").module_override(), None);
    assert_eq!(plain_c_union("Number").module_override(), None);
    assert_eq!(plain_c_member("count").module_override(), None);
    assert_eq!(plain_c_type("PyMemAllocatorDomain").module_override(), None);
}

/// A `py:class` written with `flags`.
fn py_class_with(flags: crate::DescriptionFlags) -> DomainObjectBody {
    DomainObjectBody::PyClass {
        flags,
        module: None,
        signatures: NonEmptyVector::single("bytearray".to_string()),
        is_final: false,
        body: vec![],
    }
}

#[test]
fn test_no_index_reflects_the_flag_for_a_python_object() {
    // Given — CPython's `functions.rst` documents `bytearray` a second time
    // under `:noindex:`, leaving `stdtypes.rst`'s description the target.
    let indexed = py_class_with(crate::DescriptionFlags::default());
    let unindexed = py_class_with(crate::DescriptionFlags::of([
        crate::DescriptionFlag::NoIndex,
    ]));

    // When / Then
    assert!(!indexed.no_index());
    assert!(unindexed.no_index());
}

#[test]
fn test_no_index_implies_no_index_entry_for_a_python_object() {
    // Given
    let unindexed = py_class_with(crate::DescriptionFlags::of([
        crate::DescriptionFlag::NoIndex,
    ]));
    let unlisted = py_class_with(crate::DescriptionFlags::of([
        crate::DescriptionFlag::NoIndexEntry,
    ]));

    // When / Then
    assert!(unindexed.no_index_entry());
    assert!(unlisted.no_index_entry());
    assert!(!unlisted.no_index());
}

#[test]
fn test_no_index_reflects_the_flag_for_a_command_line_option() {
    // Given
    let option = DomainObjectBody::StdCmdoption {
        flags: crate::DescriptionFlags::of([crate::DescriptionFlag::NoIndex]),
        signatures: NonEmptyVector::single("-v".to_string()),
        body: vec![],
    };

    // When / Then
    assert!(option.no_index());
    assert!(option.no_index_entry());
}

#[test]
fn test_no_contents_entry_reflects_the_flag_for_a_python_object() {
    // Given
    let object = py_class_with(crate::DescriptionFlags::of([
        crate::DescriptionFlag::NoContentsEntry,
    ]));

    // When / Then
    assert!(object.no_contents_entry());
    assert!(!object.no_index());
}

#[test]
fn test_description_flags_is_none_only_for_a_module() {
    // Given
    let module = DomainObjectBody::PyModule {
        name: "os".to_string(),
        options: crate::ModuleOptions::default(),
        body: vec![],
    };
    let class = py_class_with(crate::DescriptionFlags::default());

    // When / Then
    assert_eq!(module.description_flags(), None);
    assert_eq!(
        class.description_flags(),
        Some(&crate::DescriptionFlags::default())
    );
}

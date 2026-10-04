//! Basic domain-object registration/index tests — including
//! `.. option::` flag handling, multi-signature objects, `:no-index:`,
//! and a definition nested inside various container constructs. Split
//! out of `super::domain_object_index` purely for its line count.

use super::super::document_index::analyze;
use super::*;
use rinx_ast::{
    Directive, Document, Domain, InlineNode, Node, NonEmptyVector, ObjectType, TableSource,
    TargetName,
};

/// Looks up a domain object by the pre-refactor flat `"domain:objtype:name"`
/// key shape (e.g. `"py:function:greet"`), so test expectations can stay
/// expressed as a single string instead of repeating two-level map
/// navigation at every call site below.
fn lookup_domain_object<'a>(index: &'a ProjectIndex, flat_key: &str) -> Option<&'a String> {
    let mut parts = flat_key.splitn(3, ':');
    let domain: Domain = parts.next()?.parse().ok()?;
    let objtype_str = parts.next()?;
    let name = parts.next()?;
    let object_type = ObjectType::from_directive_name(domain, objtype_str)?;
    index
        .domain_objects
        .get(&TargetName::new(name))?
        .get(&object_type)
}

#[test]
fn test_analyze_registers_domain_object() {
    // Given
    let doc = Document::new(
        "api.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            rinx_ast::DomainObjectBody::PyFunction {
                flags: rinx_ast::DescriptionFlags::default(),
                is_async: false,
                module: None,
                is_decorator: false,
                signatures: NonEmptyVector::single("greet(name)".to_string()),
                body: vec![],
            },
        ))],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert_eq!(index.domain_objects.len(), 1);
    assert_eq!(
        lookup_domain_object(&index, "py:function:greet"),
        Some(&"api.rst".to_string())
    );
}
#[test]
fn test_analyze_registers_module_domain_object() {
    // Given
    let doc = Document::new(
        "api.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            rinx_ast::DomainObjectBody::PyModule {
                name: "greetings".to_string(),
                options: rinx_ast::ModuleOptions::default(),
                body: vec![],
            },
        ))],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert_eq!(index.domain_objects.len(), 1);
    assert_eq!(
        lookup_domain_object(&index, "py:module:greetings"),
        Some(&"api.rst".to_string())
    );
}
/// A document holding nothing but one `.. py:module:: greetings` with
/// `options`.
fn module_document(options: rinx_ast::ModuleOptions) -> Document {
    Document::new(
        "api.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            rinx_ast::DomainObjectBody::PyModule {
                name: "greetings".to_string(),
                options,
                body: vec![],
            },
        ))],
    )
}
#[test]
fn test_analyze_records_a_modules_synopsis_platform_and_deprecation() {
    // Given
    let doc = module_document(rinx_ast::ModuleOptions {
        platform: Some("Unix".to_string()),
        synopsis: Some("Greeting utilities.".to_string()),
        flags: [rinx_ast::ModuleFlag::Deprecated].into(),
    });

    // When
    let index = analyze(&doc);

    // Then
    assert_eq!(
        index.modules.get(&TargetName::new("greetings")),
        Some(&rinx_index::ModuleEntry {
            doc_path: "api.rst".to_string(),
            synopsis: Some("Greeting utilities.".to_string()),
            platform: Some("Unix".to_string()),
            deprecated: true,
        })
    );
}
#[test]
fn test_analyze_records_a_module_without_an_index_entry() {
    // Given — `:no-index-entry:` drops only the general-index entry.
    let doc = module_document(rinx_ast::ModuleOptions {
        flags: [rinx_ast::ModuleFlag::NoIndexEntry].into(),
        ..rinx_ast::ModuleOptions::default()
    });

    // When
    let index = analyze(&doc);

    // Then
    assert!(index.modules.contains_key(&TargetName::new("greetings")));
    assert!(lookup_domain_object(&index, "py:module:greetings").is_some());
    assert!(index.genindex_entries.is_empty());
}
#[test]
fn test_analyze_leaves_a_no_index_module_out_of_every_index() {
    // Given
    let doc = module_document(rinx_ast::ModuleOptions {
        flags: [rinx_ast::ModuleFlag::NoIndex].into(),
        ..rinx_ast::ModuleOptions::default()
    });

    // When
    let index = analyze(&doc);

    // Then
    assert!(index.modules.is_empty());
    assert!(index.domain_objects.is_empty());
    assert!(index.genindex_entries.is_empty());
}
#[test]
fn test_analyze_still_makes_a_no_index_module_current() {
    // Given — Sphinx sets the current module before it looks at `:no-index:`.
    let doc = Document::new(
        "api.rst".to_string(),
        vec![
            Node::Directive(Directive::DomainObject(
                rinx_ast::DomainObjectBody::PyModule {
                    name: "greetings".to_string(),
                    options: rinx_ast::ModuleOptions {
                        flags: [rinx_ast::ModuleFlag::NoIndex].into(),
                        ..rinx_ast::ModuleOptions::default()
                    },
                    body: vec![],
                },
            )),
            Node::Directive(Directive::DomainObject(
                rinx_ast::DomainObjectBody::PyFunction {
                    flags: rinx_ast::DescriptionFlags::default(),
                    is_async: false,
                    signatures: NonEmptyVector::single("greet()".to_string()),
                    is_decorator: false,
                    module: None,
                    body: vec![],
                },
            )),
        ],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert!(lookup_domain_object(&index, "py:function:greetings.greet").is_some());
}
#[test]
fn test_record_module_records_nothing_for_another_object_type() {
    // Given
    let obj = rinx_ast::DomainObjectBody::PyFunction {
        flags: rinx_ast::DescriptionFlags::default(),
        is_async: false,
        signatures: NonEmptyVector::single("greet()".to_string()),
        is_decorator: false,
        module: None,
        body: vec![],
    };
    let mut index = ProjectIndex::default();

    // When
    record_module(&obj, "greet", "api.rst", &mut index);

    // Then
    assert!(index.modules.is_empty());
}
#[test]
fn test_analyze_registers_data_domain_object() {
    // Given
    let doc = Document::new(
        "api.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            rinx_ast::DomainObjectBody::PyData {
                flags: rinx_ast::DescriptionFlags::default(),
                module: None,
                signatures: NonEmptyVector::single("DEFAULT_TIMEOUT".to_string()),
                type_: Some("int".to_string()),
                value: Some("30".to_string()),
                body: vec![],
            },
        ))],
    );

    // When
    let index = analyze(&doc);

    // Then — registered under the canonical "py:data:..." key, the same
    // one both `:py:data:` and `:py:const:` roles resolve against.
    assert_eq!(index.domain_objects.len(), 1);
    assert_eq!(
        lookup_domain_object(&index, "py:data:DEFAULT_TIMEOUT"),
        Some(&"api.rst".to_string())
    );
}
#[test]
fn test_analyze_registers_cmdoption_domain_object_without_program() {
    // Given
    let doc = Document::new(
        "cmdline.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            rinx_ast::DomainObjectBody::StdCmdoption {
                flags: rinx_ast::DescriptionFlags::default(),
                signatures: NonEmptyVector::single("-m <module-name>".to_string()),
                body: vec![],
            },
        ))],
    );

    // When
    let index = analyze(&doc);

    // Then — no ambient `.. program::`, so the key is bare.
    assert_eq!(index.domain_objects.len(), 1);
    assert_eq!(
        lookup_domain_object(&index, "std:cmdoption:-m"),
        Some(&"cmdline.rst".to_string())
    );
}
#[test]
fn test_analyze_registers_cmdoption_domain_object_qualified_by_program() {
    // Given
    let doc = Document::new(
        "dis.rst".to_string(),
        vec![
            Node::Directive(Directive::StdProgram {
                name: Some("dis".to_string()),
            }),
            Node::Directive(Directive::DomainObject(
                rinx_ast::DomainObjectBody::StdCmdoption {
                    flags: rinx_ast::DescriptionFlags::default(),
                    signatures: NonEmptyVector::single("-O".to_string()),
                    body: vec![],
                },
            )),
        ],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert_eq!(
        lookup_domain_object(&index, "std:cmdoption:dis.-o"),
        Some(&"dis.rst".to_string())
    );
}
#[test]
fn test_analyze_registers_each_flag_of_a_comma_separated_cmdoption_spec() {
    // Given — real Sphinx's `.. option:: -c, --compress` shape: one raw
    // line, two independently-referenceable flags.
    let doc = Document::new(
        "zipapp.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            rinx_ast::DomainObjectBody::StdCmdoption {
                flags: rinx_ast::DescriptionFlags::default(),
                signatures: NonEmptyVector::single("-c, --compress".to_string()),
                body: vec![],
            },
        ))],
    );

    // When
    let index = analyze(&doc);

    // Then — both flags resolve independently, from one directive.
    assert_eq!(index.domain_objects.len(), 2);
    assert_eq!(
        lookup_domain_object(&index, "std:cmdoption:-c"),
        Some(&"zipapp.rst".to_string())
    );
    assert_eq!(
        lookup_domain_object(&index, "std:cmdoption:--compress"),
        Some(&"zipapp.rst".to_string())
    );
}
#[test]
fn test_analyze_registers_each_flag_of_a_continuation_line_cmdoption() {
    // Given — the `mimetypes.rst` shape: one flag per line, no commas.
    let doc = Document::new(
        "mimetypes.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            rinx_ast::DomainObjectBody::StdCmdoption {
                flags: rinx_ast::DescriptionFlags::default(),
                signatures: NonEmptyVector::new("-h".to_string(), vec!["--help".to_string()]),
                body: vec![],
            },
        ))],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert_eq!(index.domain_objects.len(), 2);
    assert_eq!(
        lookup_domain_object(&index, "std:cmdoption:-h"),
        Some(&"mimetypes.rst".to_string())
    );
    assert_eq!(
        lookup_domain_object(&index, "std:cmdoption:--help"),
        Some(&"mimetypes.rst".to_string())
    );
}
#[test]
fn test_analyze_registers_genindex_entry_for_each_cmdoption_flag() {
    // Given
    let doc = Document::new(
        "zipapp.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            rinx_ast::DomainObjectBody::StdCmdoption {
                flags: rinx_ast::DescriptionFlags::default(),
                signatures: NonEmptyVector::single("-c, --compress".to_string()),
                body: vec![],
            },
        ))],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert_eq!(index.genindex_entries.len(), 2);
    assert_eq!(index.genindex_entries[0].anchor, "std:cmdoption:-c");
    assert_eq!(index.genindex_entries[1].anchor, "std:cmdoption:--compress");
}
#[test]
fn test_analyze_indexes_a_cmdoptions_body_once_shared_across_flags() {
    // Given — a glossary term nested in the shared description, reachable
    // regardless of how many flags share it.
    let doc = Document::new(
        "zipapp.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            rinx_ast::DomainObjectBody::StdCmdoption {
                flags: rinx_ast::DescriptionFlags::default(),
                signatures: NonEmptyVector::single("-c, --compress".to_string()),
                body: vec![Node::Directive(Directive::Glossary {
                    entries: vec![rinx_ast::GlossaryEntry {
                        terms: vec!["compression".to_string()],
                        definition: vec![],
                    }],
                    sorted: false,
                })],
            },
        ))],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert!(
        index
            .glossary_terms
            .contains_key(&TargetName::new("compression"))
    );
}
#[test]
fn test_analyze_registers_exception_domain_object() {
    // Given
    let doc = Document::new(
        "api.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            rinx_ast::DomainObjectBody::PyException {
                flags: rinx_ast::DescriptionFlags::default(),
                module: None,
                signatures: NonEmptyVector::single("GreeterError".to_string()),
                is_final: false,
                body: vec![],
            },
        ))],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert_eq!(index.domain_objects.len(), 1);
    assert_eq!(
        lookup_domain_object(&index, "py:exception:GreeterError"),
        Some(&"api.rst".to_string())
    );
}
#[test]
fn test_analyze_registers_distinct_keys_for_same_name_in_different_domains() {
    // Given — same object name "add" declared under both py and c domains
    let doc = Document::new(
        "api.rst".to_string(),
        vec![
            Node::Directive(Directive::DomainObject(
                rinx_ast::DomainObjectBody::PyFunction {
                    flags: rinx_ast::DescriptionFlags::default(),
                    is_async: false,
                    module: None,
                    is_decorator: false,
                    signatures: NonEmptyVector::single("add(a, b)".to_string()),
                    body: vec![],
                },
            )),
            Node::Directive(Directive::DomainObject(
                rinx_ast::DomainObjectBody::CFunction {
                    flags: rinx_ast::DescriptionFlags::default(),
                    signatures: NonEmptyVector::single("int add(int a, int b)".into()),
                    body: vec![],
                },
            )),
        ],
    );

    // When
    let index = analyze(&doc);

    // Then — same qualified name ("add"), but two distinct object types
    // coexist under it, one entry per domain.
    assert_eq!(index.domain_objects.len(), 1);
    assert_eq!(
        index
            .domain_objects
            .get(&TargetName::new("add"))
            .map(std::collections::BTreeMap::len),
        Some(2)
    );
    assert!(lookup_domain_object(&index, "py:function:add").is_some());
    assert!(lookup_domain_object(&index, "c:function:add").is_some());
}
#[test]
fn test_analyze_no_index_suppresses_target_and_genindex_entry() {
    // Given
    let doc = Document::new(
        "api.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            DomainObjectBody::CMember {
                signatures: NonEmptyVector::single("count".into()),
                flags: rinx_ast::DescriptionFlags::of([rinx_ast::DescriptionFlag::NoIndex]),
                body: vec![],
            },
        ))],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert!(lookup_domain_object(&index, "c:member:count").is_none());
    assert!(index.genindex_entries.is_empty());
}
#[test]
fn test_analyze_no_index_entry_keeps_target_but_suppresses_genindex_entry() {
    // Given
    let doc = Document::new(
        "api.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            DomainObjectBody::CMember {
                signatures: NonEmptyVector::single("count".into()),
                flags: rinx_ast::DescriptionFlags::of([rinx_ast::DescriptionFlag::NoIndexEntry]),
                body: vec![],
            },
        ))],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert_eq!(
        lookup_domain_object(&index, "c:member:count"),
        Some(&"api.rst".to_string())
    );
    assert!(index.genindex_entries.is_empty());
}
#[test]
fn test_analyze_leaves_domain_objects_empty_for_plain_document() {
    // Given
    let doc = Document::new(
        "plain.rst".to_string(),
        vec![Node::Paragraph(vec![InlineNode::Text(
            "No domain objects here.".to_string(),
        )])],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert!(index.domain_objects.is_empty());
}
#[test]
fn test_analyze_current_module_directive_creates_no_index_entry_of_its_own() {
    // Given — real Sphinx's `currentmodule` documents nothing; unlike
    // `py:module`, it must not appear in `domain_objects` or
    // `genindex_entries`.
    let doc = Document::new(
        "test.rst".to_string(),
        vec![Node::Directive(Directive::PyCurrentModule {
            module: Some("enum".to_string()),
        })],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert!(index.domain_objects.is_empty());
    assert!(index.genindex_entries.is_empty());
}
#[test]
fn test_analyze_registers_genindex_entry_for_domain_object() {
    // Given
    let doc = Document::new(
        "api.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            rinx_ast::DomainObjectBody::PyFunction {
                flags: rinx_ast::DescriptionFlags::default(),
                is_async: false,
                module: None,
                is_decorator: false,
                signatures: NonEmptyVector::single("greet(name)".to_string()),
                body: vec![],
            },
        ))],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert_eq!(index.genindex_entries.len(), 1);
    let entry = &index.genindex_entries[0];
    assert_eq!(entry.primary, "greet (function)");
    assert_eq!(entry.subentry, None);
    assert!(!entry.main);
    assert_eq!(entry.doc_path, "api.rst");
    assert_eq!(entry.anchor, "py:function:greet");
}
#[test]
fn test_analyze_registers_domain_object_nested_in_table_cell() {
    // Given — a `.. data::` directive nested inside a grid-table cell,
    // mirroring CPython's `curses.rst` attribute table (`A_NORMAL` etc.)
    let doc = Document::new(
        "curses.rst".to_string(),
        vec![Node::Table {
            header_rows: vec![],
            body_rows: vec![rinx_ast::TableRow {
                cells: vec![rinx_ast::TableCell {
                    colspan: 1,
                    rowspan: 1,
                    content: vec![Node::Directive(Directive::DomainObject(
                        rinx_ast::DomainObjectBody::PyData {
                            flags: rinx_ast::DescriptionFlags::default(),
                            module: None,
                            signatures: NonEmptyVector::single("A_NORMAL".to_string()),
                            type_: None,
                            value: None,
                            body: vec![],
                        },
                    ))],
                }],
            }],
        }],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert_eq!(
        lookup_domain_object(&index, "py:data:A_NORMAL"),
        Some(&"curses.rst".to_string())
    );
}
#[test]
fn test_analyze_registers_domain_object_nested_in_list_table_cell() {
    // Given — a `.. py:attribute::` nested inside a list-table cell,
    // mirroring the docs/dev/known_bugs.md `reference/datamodel.rst` scenario
    let doc = Document::new(
        "datamodel.rst".to_string(),
        vec![Node::Directive(Directive::DataTable {
            source: TableSource::List,
            title: None,
            header_rows: 0,
            stub_columns: 0,
            widths: None,
            width: None,
            align: None,
            classes: vec![],
            name: None,
            rows: vec![rinx_ast::TableRow {
                cells: vec![rinx_ast::TableCell {
                    colspan: 1,
                    rowspan: 1,
                    content: vec![Node::Directive(Directive::DomainObject(
                        rinx_ast::DomainObjectBody::PyAttribute {
                            flags: rinx_ast::DescriptionFlags::default(),
                            module: None,
                            signatures: NonEmptyVector::single("method.__self__".to_string()),
                            type_: None,
                            value: None,
                            canonical: None,
                            body: vec![],
                        },
                    ))],
                }],
            }],
        })],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert_eq!(
        lookup_domain_object(&index, "py:attribute:method.__self__"),
        Some(&"datamodel.rst".to_string())
    );
}
#[test]
fn test_analyze_registers_domain_object_nested_in_bullet_list() {
    // Given
    let doc = Document::new(
        "test.rst".to_string(),
        vec![Node::BulletList {
            bullet: '-',
            items: vec![rinx_ast::ListItem {
                nodes: vec![Node::Directive(Directive::DomainObject(
                    rinx_ast::DomainObjectBody::PyFunction {
                        flags: rinx_ast::DescriptionFlags::default(),
                        is_async: false,
                        module: None,
                        is_decorator: false,
                        signatures: NonEmptyVector::single("greet(name)".to_string()),
                        body: vec![],
                    },
                ))],
            }],
        }],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert_eq!(
        lookup_domain_object(&index, "py:function:greet"),
        Some(&"test.rst".to_string())
    );
}
#[test]
fn test_analyze_registers_domain_object_nested_in_definition_list() {
    // Given
    let doc = Document::new(
        "test.rst".to_string(),
        vec![Node::DefinitionList {
            items: vec![rinx_ast::DefinitionListItem {
                term: vec![InlineNode::Text("term".to_string())],
                definition: vec![Node::Directive(Directive::DomainObject(
                    rinx_ast::DomainObjectBody::PyFunction {
                        flags: rinx_ast::DescriptionFlags::default(),
                        is_async: false,
                        module: None,
                        is_decorator: false,
                        signatures: NonEmptyVector::single("greet(name)".to_string()),
                        body: vec![],
                    },
                ))],
            }],
        }],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert_eq!(
        lookup_domain_object(&index, "py:function:greet"),
        Some(&"test.rst".to_string())
    );
}
#[test]
fn test_analyze_registers_domain_object_nested_in_admonition_body() {
    // Given
    let doc = Document::new(
        "test.rst".to_string(),
        vec![Node::Directive(Directive::Admonition {
            kind: rinx_ast::AdmonitionKind::Note,
            title: None,
            collapsible: None,
            body: vec![Node::Directive(Directive::DomainObject(
                rinx_ast::DomainObjectBody::PyFunction {
                    flags: rinx_ast::DescriptionFlags::default(),
                    is_async: false,
                    module: None,
                    is_decorator: false,
                    signatures: NonEmptyVector::single("greet(name)".to_string()),
                    body: vec![],
                },
            ))],
        })],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert_eq!(
        lookup_domain_object(&index, "py:function:greet"),
        Some(&"test.rst".to_string())
    );
}
#[test]
fn test_analyze_registers_domain_object_nested_in_another_domain_objects_body() {
    // Given — a `py:module` whose body contains a nested `py:function`
    let doc = Document::new(
        "test.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            rinx_ast::DomainObjectBody::PyModule {
                name: "greetings".to_string(),
                options: rinx_ast::ModuleOptions::default(),
                body: vec![Node::Directive(Directive::DomainObject(
                    rinx_ast::DomainObjectBody::PyFunction {
                        flags: rinx_ast::DescriptionFlags::default(),
                        is_async: false,
                        module: None,
                        is_decorator: false,
                        signatures: NonEmptyVector::single("greet(name)".to_string()),
                        body: vec![],
                    },
                ))],
            },
        ))],
    );

    // When
    let index = analyze(&doc);

    // Then — both the outer module and the nested function are indexed,
    // the function qualified by the enclosing module's name
    assert_eq!(index.domain_objects.len(), 2);
    assert!(lookup_domain_object(&index, "py:module:greetings").is_some());
    assert!(lookup_domain_object(&index, "py:function:greetings.greet").is_some());
}

/// A `.. option::` for `-v`, written with `flags`, whose body holds a label.
fn cmdoption_with(flags: rinx_ast::DescriptionFlags) -> Document {
    Document::new(
        "cmdline.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            rinx_ast::DomainObjectBody::StdCmdoption {
                flags,
                signatures: NonEmptyVector::single("-v, --verbose".to_string()),
                body: vec![Node::Target {
                    name: TargetName::new("verbosity"),
                    destination: None,
                }],
            },
        ))],
    )
}

#[test]
fn test_analyze_skips_a_no_index_cmdoption_but_indexes_its_body() {
    // Given
    let doc = cmdoption_with(rinx_ast::DescriptionFlags::of([
        rinx_ast::DescriptionFlag::NoIndex,
    ]));

    // When
    let index = analyze(&doc);

    // Then — neither a target nor an index entry, as in Sphinx, while what
    // its description defines is still reachable.
    assert!(index.domain_objects.is_empty());
    assert!(index.genindex_entries.is_empty());
    assert!(index.targets.contains_key(&TargetName::new("verbosity")));
}

#[test]
fn test_analyze_keeps_a_no_index_entry_cmdoption_as_a_target_only() {
    // Given
    let doc = cmdoption_with(rinx_ast::DescriptionFlags::of([
        rinx_ast::DescriptionFlag::NoIndexEntry,
    ]));

    // When
    let index = analyze(&doc);

    // Then
    assert_eq!(
        lookup_domain_object(&index, "std:cmdoption:--verbose"),
        Some(&"cmdline.rst".to_string())
    );
    assert!(index.genindex_entries.is_empty());
}

#[test]
fn test_analyze_skips_a_no_index_python_class() {
    // Given — CPython's `functions.rst` repeats `bytearray` under
    // `:noindex:`; only `stdtypes.rst`'s description may define it.
    let doc = Document::new(
        "functions.rst".to_string(),
        vec![Node::Directive(Directive::DomainObject(
            rinx_ast::DomainObjectBody::PyClass {
                flags: rinx_ast::DescriptionFlags::of([rinx_ast::DescriptionFlag::NoIndex]),
                module: None,
                signatures: NonEmptyVector::single("bytearray(source=b'')".to_string()),
                is_final: false,
                body: vec![],
            },
        ))],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert!(index.domain_objects.is_empty());
    assert!(index.genindex_entries.is_empty());
}

use rusty_sphinx_ast::{Directive, Document, IndexEntry, Node, TargetName};
use rusty_sphinx_index::{GenIndexEntry, ProjectIndex, TargetLocation};
use rusty_sphinx_scope::Scope;

use super::domain_object_index::index_domain_object;

/// Analyzes a single `Document` and returns a local `ProjectIndex`.
///
/// This extracts targets, document titles, and glossary terms. The `nav_tree` is not
/// populated here — it is built globally by [`build_project_index()`].
#[must_use]
pub fn analyze(doc: &Document) -> ProjectIndex {
    let mut index = ProjectIndex::default();
    let mut found_title = false;
    for node in &doc.nodes {
        if !found_title && let Node::Heading { level: 1, text } = node {
            index
                .document_titles
                .insert(doc.path.clone(), rusty_sphinx_ast::inline_plain_text(text));
            found_title = true;
        }
    }
    index_nodes(&doc.nodes, &doc.path, &mut index, &mut Scope::default());
    index
}

/// Recursively registers targets, glossary terms, and domain objects found
/// anywhere in `nodes`, including inside table cells, list items, and
/// directive bodies — not just at the document's top level. This mirrors
/// the recursion shape `render_nodes` (in the renderer crate) uses, since a
/// definition nested in a container still needs to be indexed for
/// cross-references to resolve, exactly like it's still rendered with a
/// working anchor.
///
/// `scope.python` carries the enclosing `py:class`/`py:exception` stack
/// (lexical, pushed/popped around a nested body — see
/// [`rusty_sphinx_ast::DomainObjectBody::deduce_local_scope`], shared with
/// the renderer so index keys and anchor `id`s can't drift apart) and the
/// most recently seen `py:module` (document-order state, not lexical
/// nesting — real Sphinx docs write `py:module` and the functions/classes it
/// documents as *siblings*, not nested underneath it, so it is never popped
/// when returning from a nested body; a module stays "current" for the rest
/// of the document until another `py:module`, or `py:currentmodule`,
/// changes it). `scope.c` is the same idea for the `c` domain's
/// `c:struct`/`c:union` nesting — a wholly separate stack (see
/// [`rusty_sphinx_scope::CScope`]'s doc comment for why it isn't a variant of
/// `PythonScope`); `c:function`/`c:macro` never touch it and keep qualifying
/// via `scope.python` exactly as before it existed.
pub(super) fn index_nodes(
    nodes: &[Node],
    doc_path: &str,
    index: &mut ProjectIndex,
    scope: &mut Scope,
) {
    for node in nodes {
        match node {
            Node::Target { name, uri } => {
                let location = uri.as_ref().map_or_else(
                    || TargetLocation::Internal(doc_path.to_string()),
                    |url| TargetLocation::External(url.clone()),
                );
                index.targets.insert(name.clone(), location);
            }
            Node::Directive(Directive::Glossary { entries, .. }) => {
                for entry in entries {
                    for term in &entry.terms {
                        index
                            .glossary_terms
                            .insert(TargetName::new(term), doc_path.to_string());
                    }
                }
            }
            Node::Directive(Directive::Index { entries, id }) => {
                for entry in entries {
                    if let IndexEntry::Term {
                        primary,
                        subentry,
                        main,
                    } = entry
                    {
                        index.genindex_entries.push(GenIndexEntry {
                            primary: primary.clone(),
                            subentry: subentry.clone(),
                            main: *main,
                            doc_path: doc_path.to_string(),
                            anchor: id.clone(),
                        });
                    }
                    // IndexEntry::See/SeeAlso redirect rather than link to
                    // content and are not surfaced in genindex_entries yet
                    // (see spec_gaps.md).
                }
            }
            Node::Directive(Directive::DomainObject(obj)) => {
                index_domain_object(obj, doc_path, index, scope);
            }
            Node::Directive(Directive::PyCurrentModule { module }) => match module {
                Some(name) => scope.python.set_module(name),
                None => scope.python.clear_module(),
            },
            Node::Directive(Directive::CNamespace { namespace }) => {
                scope.c.set_namespace(namespace.as_deref());
            }
            Node::Directive(Directive::CNamespacePush { namespace }) => {
                scope.c.push_namespace(namespace);
            }
            Node::Directive(Directive::CNamespacePop) => scope.c.pop_namespace(),
            Node::Directive(Directive::StdProgram { name }) => match name {
                Some(name) => scope.program.set(name),
                None => scope.program.clear(),
            },
            Node::Directive(
                Directive::Admonition { body, .. }
                | Directive::VersionChange { body, .. }
                | Directive::SeeAlso { body },
            ) => {
                index_nodes(body, doc_path, index, scope);
            }
            Node::BulletList { items, .. } | Node::EnumeratedList { items, .. } => {
                for item in items {
                    index_nodes(&item.nodes, doc_path, index, scope);
                }
            }
            Node::DefinitionList { items } => {
                for item in items {
                    index_nodes(&item.definition, doc_path, index, scope);
                }
            }
            Node::Table {
                header_rows,
                body_rows,
            } => {
                for row in header_rows.iter().chain(body_rows) {
                    for cell in &row.cells {
                        index_nodes(&cell.content, doc_path, index, scope);
                    }
                }
            }
            Node::Directive(Directive::ListTable { rows, name, .. }) => {
                if let Some(target_name) = name {
                    index.targets.insert(
                        target_name.clone(),
                        TargetLocation::Internal(doc_path.to_string()),
                    );
                }
                for row in rows {
                    for cell in &row.cells {
                        index_nodes(&cell.content, doc_path, index, scope);
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_sphinx_ast::{
        Domain, InlineNode, NonEmptyVector, ObjectType, PyObjectType, TargetSearchOrder,
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
    fn test_analyze_returns_default_index_for_empty_document() {
        // Given
        let doc = Document::new("test.rst".to_string(), vec![]);

        // When
        let index = analyze(&doc);

        // Then
        let _ = format!("{index:?}"); // Ensures it doesn't panic
    }
    #[test]
    fn test_analyze_returns_default_index_for_populated_document() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Heading {
                level: 1,
                text: vec![InlineNode::Text("Title".to_string())],
            }],
        );

        // When
        let index = analyze(&doc);

        // Then
        // Currently analyze does not populate anything, but it shouldn't panic
        let _ = format!("{index:?}");
    }
    #[test]
    fn test_analyze_populates_targets_for_target_nodes() {
        // Given
        let doc = Document::new(
            "docs/my-file.rst".to_string(),
            vec![
                Node::Target {
                    name: TargetName::new("section-1"),
                    uri: None,
                },
                Node::Paragraph(vec![InlineNode::Text("some text".to_string())]),
            ],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert_eq!(index.targets.len(), 1);
        assert_eq!(
            index.targets.get(&TargetName::new("section-1")).unwrap(),
            &TargetLocation::Internal("docs/my-file.rst".to_string())
        );
    }
    #[test]
    fn test_analyze_extracts_h1_title() {
        // Given
        let doc = Document::new(
            "docs/my-file.rst".to_string(),
            vec![
                Node::Paragraph(vec![InlineNode::Text("some text".to_string())]),
                Node::Heading {
                    level: 1,
                    text: vec![InlineNode::Text("My Title".to_string())],
                },
                Node::Heading {
                    level: 1,
                    text: vec![InlineNode::Text("Ignored Second H1".to_string())],
                },
            ],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert_eq!(index.document_titles.len(), 1);
        assert_eq!(
            index.document_titles.get("docs/my-file.rst").unwrap(),
            "My Title"
        );
    }
    #[test]
    fn test_analyze_extracts_h1_title_as_plain_text_when_heading_has_domain_object_reference() {
        // Given — a heading containing a `~`-shortened domain-object reference
        let doc = Document::new(
            "docs/greetings.rst".to_string(),
            vec![Node::Heading {
                level: 1,
                text: vec![
                    InlineNode::Text("The ".to_string()),
                    InlineNode::DomainObjectReference {
                        object_type: ObjectType::Py(PyObjectType::Module),
                        name: "pkg.greetings".to_string(),
                        display: "greetings".to_string(),
                        link: true,
                        search_order: TargetSearchOrder::LeastQualifiedFirst,
                    },
                    InlineNode::Text(" Module".to_string()),
                ],
            }],
        );

        // When
        let index = analyze(&doc);

        // Then — the title is flattened to plain text, using the shortened display
        assert_eq!(
            index.document_titles.get("docs/greetings.rst").unwrap(),
            "The greetings Module"
        );
    }
    #[test]
    fn test_analyze_registers_glossary_terms() {
        // Given
        let doc = Document::new(
            "glossary.rst".to_string(),
            vec![Node::Directive(Directive::Glossary {
                entries: vec![rusty_sphinx_ast::GlossaryEntry {
                    terms: vec!["environment".to_string()],
                    definition: vec![],
                }],
                sorted: false,
            })],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert_eq!(index.glossary_terms.len(), 1);
        assert_eq!(
            index.glossary_terms.get(&TargetName::new("environment")),
            Some(&"glossary.rst".to_string())
        );
    }
    #[test]
    fn test_analyze_registers_all_terms_in_multi_term_entry() {
        // Given
        let doc = Document::new(
            "glossary.rst".to_string(),
            vec![Node::Directive(Directive::Glossary {
                entries: vec![rusty_sphinx_ast::GlossaryEntry {
                    terms: vec!["term 1".to_string(), "term 2".to_string()],
                    definition: vec![],
                }],
                sorted: false,
            })],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert_eq!(index.glossary_terms.len(), 2);
        assert!(
            index
                .glossary_terms
                .contains_key(&TargetName::new("term 1"))
        );
        assert!(
            index
                .glossary_terms
                .contains_key(&TargetName::new("term 2"))
        );
    }
    #[test]
    fn test_analyze_registers_every_declared_name_of_a_multi_signature_object() {
        // Given — the confirmed `library/socket.rst` shape: one directive
        // declaring three aliases, which real Sphinx resolves individually.
        let doc = Document::new(
            "api.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyData {
                    module: None,
                    signatures: NonEmptyVector::new(
                        "AF_UNIX".to_string(),
                        vec!["AF_INET".to_string(), "AF_INET6".to_string()],
                    ),
                    type_: None,
                    value: None,
                    body: vec![],
                },
            ))],
        );

        // When
        let index = analyze(&doc);

        // Then — each alias is an independently resolvable target.
        assert_eq!(index.domain_objects.len(), 3);
        for name in ["AF_UNIX", "AF_INET", "AF_INET6"] {
            assert_eq!(
                lookup_domain_object(&index, &format!("py:data:{name}")),
                Some(&"api.rst".to_string()),
                "{name} should resolve"
            );
        }
    }
    #[test]
    fn test_analyze_gives_every_declared_name_its_own_genindex_entry() {
        // Given
        let doc = Document::new(
            "api.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyData {
                    module: None,
                    signatures: NonEmptyVector::new("A".to_string(), vec!["ASCII".to_string()]),
                    type_: None,
                    value: None,
                    body: vec![],
                },
            ))],
        );

        // When
        let index = analyze(&doc);

        // Then — one entry per alias, each anchored to its own name.
        assert_eq!(index.genindex_entries.len(), 2);
        assert_eq!(index.genindex_entries[0].anchor, "py:data:a");
        assert_eq!(index.genindex_entries[1].anchor, "py:data:ascii");
    }
    #[test]
    fn test_analyze_indexes_a_multi_signature_objects_body_only_once() {
        // Given — the aliases share one docstring; indexing it per alias
        // would register its contents several times over.
        let doc = Document::new(
            "api.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyData {
                    module: None,
                    signatures: NonEmptyVector::new(
                        "AF_UNIX".to_string(),
                        vec!["AF_INET".to_string()],
                    ),
                    type_: None,
                    value: None,
                    body: vec![Node::Target {
                        name: rusty_sphinx_ast::TargetName::new("address-families"),
                        uri: None,
                    }],
                },
            ))],
        );

        // When
        let index = analyze(&doc);

        // Then — the body's target is registered exactly once, even though
        // two names were.
        assert_eq!(index.domain_objects.len(), 2);
        assert_eq!(index.targets.len(), 1);
    }
    #[test]
    fn test_analyze_qualifies_every_alias_of_a_multi_signature_object_by_module() {
        // Given — a multi-signature object under a current module: every
        // alias, not just the primary, has to pick the module qualifier up.
        let doc = Document::new(
            "api.rst".to_string(),
            vec![
                Node::Directive(Directive::DomainObject(
                    rusty_sphinx_ast::DomainObjectBody::PyModule {
                        name: "socket".to_string(),
                        platform: None,
                        synopsis: None,
                        deprecated: false,
                        body: vec![],
                    },
                )),
                Node::Directive(Directive::DomainObject(
                    rusty_sphinx_ast::DomainObjectBody::PyData {
                        module: None,
                        signatures: NonEmptyVector::new(
                            "AF_UNIX".to_string(),
                            vec!["AF_INET".to_string()],
                        ),
                        type_: None,
                        value: None,
                        body: vec![],
                    },
                )),
            ],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert_eq!(
            lookup_domain_object(&index, "py:data:socket.AF_UNIX"),
            Some(&"api.rst".to_string())
        );
        assert_eq!(
            lookup_domain_object(&index, "py:data:socket.AF_INET"),
            Some(&"api.rst".to_string())
        );
    }
    #[test]
    fn test_analyze_registers_target_nested_in_table_cell() {
        // Given — a target nested inside a grid-table cell
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Table {
                header_rows: vec![],
                body_rows: vec![rusty_sphinx_ast::TableRow {
                    cells: vec![rusty_sphinx_ast::TableCell {
                        colspan: 1,
                        rowspan: 1,
                        content: vec![Node::Target {
                            name: TargetName::new("nested-target"),
                            uri: None,
                        }],
                    }],
                }],
            }],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert_eq!(
            index.targets.get(&TargetName::new("nested-target")),
            Some(&TargetLocation::Internal("test.rst".to_string()))
        );
    }
    #[test]
    fn test_analyze_registers_list_table_name_as_target() {
        // Given — a `.. list-table::` with a `:name:` option
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::ListTable {
                title: None,
                header_rows: 0,
                stub_columns: 0,
                widths: None,
                width: None,
                align: None,
                classes: vec![],
                name: Some(TargetName::new("fruit-table")),
                rows: vec![],
            })],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert_eq!(
            index.targets.get(&TargetName::new("fruit-table")),
            Some(&TargetLocation::Internal("test.rst".to_string()))
        );
    }
    #[test]
    fn test_analyze_list_table_without_name_registers_no_target() {
        // Given — a `.. list-table::` with no `:name:` option
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::ListTable {
                title: None,
                header_rows: 0,
                stub_columns: 0,
                widths: None,
                width: None,
                align: None,
                classes: vec![],
                name: None,
                rows: vec![],
            })],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert!(index.targets.is_empty());
    }
    #[test]
    fn test_analyze_does_not_double_qualify_already_qualified_nested_attribute() {
        // Given — mirrors CPython's `Doc/library/exceptions.rst`, which
        // nests `.. attribute:: StopIteration.value` (already fully
        // qualified) inside `.. exception:: StopIteration`, rather than
        // writing the bare name `value`.
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyException {
                    module: None,
                    signatures: NonEmptyVector::single("StopIteration".to_string()),
                    is_final: false,
                    body: vec![Node::Directive(Directive::DomainObject(
                        rusty_sphinx_ast::DomainObjectBody::PyAttribute {
                            module: None,
                            signatures: NonEmptyVector::single("StopIteration.value".to_string()),
                            type_: None,
                            value: None,
                            canonical: None,
                            body: vec![],
                        },
                    ))],
                },
            ))],
        );

        // When
        let index = analyze(&doc);

        // Then — the attribute is indexed under its own already-qualified
        // name, not doubled to "StopIteration.StopIteration.value"
        assert_eq!(index.domain_objects.len(), 2);
        assert!(lookup_domain_object(&index, "py:exception:StopIteration").is_some());
        assert!(lookup_domain_object(&index, "py:attribute:StopIteration.value").is_some());
    }
    #[test]
    fn test_analyze_object_before_any_module_directive_stays_unqualified() {
        // Given — a `py:function` appearing before any `py:module` in the
        // document has no current module to fall back to.
        let doc = Document::new(
            "test.rst".to_string(),
            vec![
                Node::Directive(Directive::DomainObject(
                    rusty_sphinx_ast::DomainObjectBody::PyFunction {
                        module: None,
                        is_decorator: false,
                        signatures: NonEmptyVector::single("greet(name)".to_string()),
                        body: vec![],
                    },
                )),
                Node::Directive(Directive::DomainObject(
                    rusty_sphinx_ast::DomainObjectBody::PyModule {
                        name: "greetings".to_string(),
                        platform: None,
                        synopsis: None,
                        deprecated: false,
                        body: vec![],
                    },
                )),
            ],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert!(lookup_domain_object(&index, "py:function:greet").is_some());
    }
    #[test]
    fn test_analyze_composes_module_and_class_qualifiers() {
        // Given — a `py:class` documented as a sibling after `py:module`
        // (module-qualified), with a `py:method` nested inside the class
        // (class-qualified) — both qualifiers must compose.
        let doc = Document::new(
            "test.rst".to_string(),
            vec![
                Node::Directive(Directive::DomainObject(
                    rusty_sphinx_ast::DomainObjectBody::PyModule {
                        name: "types".to_string(),
                        platform: None,
                        synopsis: None,
                        deprecated: false,
                        body: vec![],
                    },
                )),
                Node::Directive(Directive::DomainObject(
                    rusty_sphinx_ast::DomainObjectBody::PyClass {
                        module: None,
                        signatures: NonEmptyVector::single("DynamicClassAttribute".to_string()),
                        is_final: false,
                        body: vec![Node::Directive(Directive::DomainObject(
                            rusty_sphinx_ast::DomainObjectBody::PyMethod {
                                module: None,
                                is_decorator: false,
                                signatures: NonEmptyVector::single(
                                    "__get__(self, instance, owner)".to_string(),
                                ),
                                is_classmethod: false,
                                is_staticmethod: false,
                                is_abstractmethod: false,
                                is_async: false,
                                body: vec![],
                            },
                        ))],
                    },
                )),
            ],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert!(lookup_domain_object(&index, "py:class:types.DynamicClassAttribute").is_some());
        assert!(
            lookup_domain_object(&index, "py:method:types.DynamicClassAttribute.__get__").is_some()
        );
    }
    #[test]
    fn test_analyze_registers_genindex_entry_for_index_directive_single() {
        // Given
        let doc = Document::new(
            "guide.rst".to_string(),
            vec![Node::Directive(Directive::Index {
                entries: vec![rusty_sphinx_ast::IndexEntry::Term {
                    primary: "execution".to_string(),
                    subentry: None,
                    main: false,
                }],
                id: "index-0".to_string(),
            })],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert_eq!(index.genindex_entries.len(), 1);
        let entry = &index.genindex_entries[0];
        assert_eq!(entry.primary, "execution");
        assert_eq!(entry.doc_path, "guide.rst");
        assert_eq!(entry.anchor, "index-0");
    }
    #[test]
    fn test_analyze_registers_genindex_entry_with_subentry_and_main_flag() {
        // Given
        let doc = Document::new(
            "guide.rst".to_string(),
            vec![Node::Directive(Directive::Index {
                entries: vec![rusty_sphinx_ast::IndexEntry::Term {
                    primary: "Python".to_string(),
                    subentry: Some("interpreter".to_string()),
                    main: true,
                }],
                id: "index-0".to_string(),
            })],
        );

        // When
        let index = analyze(&doc);

        // Then
        let entry = &index.genindex_entries[0];
        assert_eq!(entry.subentry, Some("interpreter".to_string()));
        assert!(entry.main);
    }
    #[test]
    fn test_analyze_skips_see_and_seealso_index_entries() {
        // Given
        let doc = Document::new(
            "guide.rst".to_string(),
            vec![Node::Directive(Directive::Index {
                entries: vec![
                    rusty_sphinx_ast::IndexEntry::See {
                        entry: "foo".to_string(),
                        target: "bar".to_string(),
                    },
                    rusty_sphinx_ast::IndexEntry::SeeAlso {
                        entry: "foo".to_string(),
                        target: "bar".to_string(),
                    },
                ],
                id: "index-0".to_string(),
            })],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert!(index.genindex_entries.is_empty());
    }
    #[test]
    fn test_analyze_registers_genindex_entry_for_index_directive_nested_in_bullet_list() {
        // Given
        let doc = Document::new(
            "guide.rst".to_string(),
            vec![Node::BulletList {
                bullet: '-',
                items: vec![rusty_sphinx_ast::ListItem {
                    nodes: vec![Node::Directive(Directive::Index {
                        entries: vec![rusty_sphinx_ast::IndexEntry::Term {
                            primary: "execution".to_string(),
                            subentry: None,
                            main: false,
                        }],
                        id: "index-0".to_string(),
                    })],
                }],
            }],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert_eq!(index.genindex_entries.len(), 1);
    }
    #[test]
    fn test_analyze_registers_genindex_entry_for_index_directive_nested_in_admonition() {
        // Given
        let doc = Document::new(
            "guide.rst".to_string(),
            vec![Node::Directive(Directive::Admonition {
                kind: rusty_sphinx_ast::AdmonitionKind::Note,
                title: None,
                collapsible: None,
                body: vec![Node::Directive(Directive::Index {
                    entries: vec![rusty_sphinx_ast::IndexEntry::Term {
                        primary: "execution".to_string(),
                        subentry: None,
                        main: false,
                    }],
                    id: "index-0".to_string(),
                })],
            })],
        );

        // When
        let index = analyze(&doc);

        // Then
        assert_eq!(index.genindex_entries.len(), 1);
    }
}

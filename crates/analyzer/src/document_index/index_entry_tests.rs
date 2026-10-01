//! `.. index::` directive, `:index:` role, registry role (`:pep:`, `:rfc:`,
//! …) and `:pep-reference:`/`:rfc-reference:` role tests for [`super::analyze`]:
//! which general-index entries they register, and where in the node tree they
//! are found.

use super::*;

#[test]
fn test_analyze_registers_genindex_entry_for_index_directive_single() {
    // Given
    let doc = Document::new(
        "guide.rst".to_string(),
        vec![Node::Directive(Directive::Index {
            entries: vec![rinx_ast::IndexEntry::Term {
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
            entries: vec![rinx_ast::IndexEntry::Term {
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
fn test_analyze_records_see_and_seealso_index_entries_as_redirects() {
    // Given
    let doc = Document::new(
        "guide.rst".to_string(),
        vec![Node::Directive(Directive::Index {
            entries: vec![
                rinx_ast::IndexEntry::See {
                    entry: "foo".to_string(),
                    target: "bar".to_string(),
                },
                rinx_ast::IndexEntry::SeeAlso {
                    entry: "foo".to_string(),
                    target: "bar".to_string(),
                },
            ],
            id: "index-0".to_string(),
        })],
    );

    // When
    let index = analyze(&doc);

    // Then — redirects, not linked entries
    assert!(index.genindex_entries.is_empty());
    assert_eq!(
        index.genindex_redirects,
        vec![
            GenIndexRedirect {
                primary: "foo".to_string(),
                kind: GenIndexRedirectKind::See,
                target: "bar".to_string(),
            },
            GenIndexRedirect {
                primary: "foo".to_string(),
                kind: GenIndexRedirectKind::SeeAlso,
                target: "bar".to_string(),
            },
        ]
    );
}

#[test]
fn test_analyze_registers_genindex_entry_for_index_directive_nested_in_bullet_list() {
    // Given
    let doc = Document::new(
        "guide.rst".to_string(),
        vec![Node::BulletList {
            bullet: '-',
            items: vec![rinx_ast::ListItem {
                nodes: vec![Node::Directive(Directive::Index {
                    entries: vec![rinx_ast::IndexEntry::Term {
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
            kind: rinx_ast::AdmonitionKind::Note,
            title: None,
            collapsible: None,
            body: vec![Node::Directive(Directive::Index {
                entries: vec![rinx_ast::IndexEntry::Term {
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

fn pep(target: &str, index_id: &str) -> InlineNode {
    registry_reference(rinx_ast::Registry::Pep, target, index_id)
}

fn registry_reference(registry: rinx_ast::Registry, target: &str, index_id: &str) -> InlineNode {
    InlineNode::RegistryReference {
        target: rinx_ast::RegistryTarget::parse(registry, target).unwrap(),
        display: Some("ignored for the entry".to_string()),
        index_id: index_id.to_string(),
        span: None,
    }
}

#[test]
fn test_analyze_registers_a_genindex_entry_for_a_pep_role() {
    // Given
    let doc = Document::new(
        "guide.rst".to_string(),
        vec![Node::Paragraph(vec![pep("8#naming", "index-3")])],
    );

    // When
    let index = analyze(&doc);

    // Then — Sphinx's entry text: the target as written, never the title
    assert_eq!(
        index.genindex_entries,
        vec![GenIndexEntry {
            primary: "Python Enhancement Proposals".to_string(),
            subentry: Some("PEP 8#naming".to_string()),
            main: false,
            doc_path: "guide.rst".to_string(),
            anchor: "index-3".to_string(),
        }]
    );
}

#[test]
fn test_analyze_finds_a_pep_role_in_a_dropdown_title() {
    // Given inline content no block-level indexing walks
    let mut dropdown = rinx_ast::Dropdown::new();
    dropdown.title = vec![pep("20", "index-0")];
    dropdown.body = vec![Node::Paragraph(vec![pep("8", "index-1")])];
    let doc = Document::new(
        "guide.rst".to_string(),
        vec![Node::Directive(Directive::Dropdown(Box::new(dropdown)))],
    );

    // When
    let index = analyze(&doc);

    // Then
    let anchors: Vec<&str> = index
        .genindex_entries
        .iter()
        .map(|entry| entry.anchor.as_str())
        .collect();
    assert_eq!(anchors, vec!["index-0", "index-1"]);
}

#[test]
fn test_analyze_files_each_registry_under_its_own_group() {
    // Given
    let doc = Document::new(
        "guide.rst".to_string(),
        vec![Node::Paragraph(vec![
            registry_reference(rinx_ast::Registry::Rfc, "2324#section-2.3", "index-0"),
            registry_reference(rinx_ast::Registry::Cve, "2024-3094", "index-1"),
            registry_reference(rinx_ast::Registry::Cwe, "787", "index-2"),
        ])],
    );

    // When
    let index = analyze(&doc);

    // Then — Sphinx's groups, and an RFC section spelled out
    let entries: Vec<(&str, Option<&str>)> = index
        .genindex_entries
        .iter()
        .map(|entry| (entry.primary.as_str(), entry.subentry.as_deref()))
        .collect();
    assert_eq!(
        entries,
        vec![
            ("RFC", Some("RFC 2324 Section 2.3")),
            (
                "Common Vulnerabilities and Exposures",
                Some("CVE 2024-3094")
            ),
            ("Common Weakness Enumeration", Some("CWE 787")),
        ]
    );
}

#[test]
fn test_analyze_registers_no_genindex_entry_for_an_rfc_reference_role() {
    // Given — docutils' role makes no index entry, unlike Sphinx's `:rfc:`
    let doc = Document::new(
        "guide.rst".to_string(),
        vec![Node::Paragraph(vec![InlineNode::DocutilsRfcReference {
            number: rinx_ast::DocutilsRfcNumber::parse("2822").unwrap(),
            span: None,
        }])],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert!(index.genindex_entries.is_empty());
}

#[test]
fn test_analyze_registers_no_genindex_entry_for_a_pep_reference_role() {
    // Given — docutils' role makes no index entry, unlike Sphinx's `:pep:`
    let doc = Document::new(
        "guide.rst".to_string(),
        vec![Node::Paragraph(vec![InlineNode::DocutilsPepReference {
            number: rinx_ast::DocutilsPepNumber::parse("8").unwrap(),
            span: None,
        }])],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert!(
        index.genindex_entries.is_empty(),
        "{:?}",
        index.genindex_entries
    );
}

fn index_role(entries: Vec<rinx_ast::IndexEntry>, index_id: &str) -> InlineNode {
    InlineNode::IndexReference {
        title: "text".to_string(),
        entries,
        index_id: index_id.to_string(),
        span: None,
    }
}

#[test]
fn test_analyze_registers_an_index_role_entries_at_its_anchor() {
    // Given
    let doc = Document::new(
        "guide.rst".to_string(),
        vec![Node::Paragraph(vec![index_role(
            vec![
                rinx_ast::IndexEntry::Term {
                    primary: "loop".to_string(),
                    subentry: Some("statement".to_string()),
                    main: true,
                },
                rinx_ast::IndexEntry::See {
                    entry: "goto".to_string(),
                    target: "jump".to_string(),
                },
            ],
            "index-4",
        )])],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert_eq!(
        index.genindex_entries,
        vec![GenIndexEntry {
            primary: "loop".to_string(),
            subentry: Some("statement".to_string()),
            main: true,
            doc_path: "guide.rst".to_string(),
            anchor: "index-4".to_string(),
        }]
    );
    assert_eq!(
        index.genindex_redirects,
        vec![GenIndexRedirect {
            primary: "goto".to_string(),
            kind: GenIndexRedirectKind::See,
            target: "jump".to_string(),
        }]
    );
}

#[test]
fn test_analyze_finds_an_index_role_in_a_glossary_definition() {
    // Given inline content block-level indexing does not walk
    let term = rinx_ast::IndexEntry::Term {
        primary: "execution".to_string(),
        subentry: None,
        main: false,
    };
    let doc = Document::new(
        "guide.rst".to_string(),
        vec![Node::Directive(Directive::Glossary {
            entries: vec![rinx_ast::GlossaryEntry {
                terms: vec!["term".to_string()],
                definition: vec![Node::Paragraph(vec![index_role(vec![term], "index-0")])],
            }],
            sorted: false,
        })],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert_eq!(index.genindex_entries.len(), 1);
    assert_eq!(index.genindex_entries[0].anchor, "index-0");
}

#[test]
fn test_analyze_registers_nothing_for_an_index_role_whose_entry_was_refused() {
    // Given — a refused entry leaves the text and anchor, no entries
    let doc = Document::new(
        "guide.rst".to_string(),
        vec![Node::Paragraph(vec![index_role(Vec::new(), "index-0")])],
    );

    // When
    let index = analyze(&doc);

    // Then
    assert!(index.genindex_entries.is_empty());
    assert!(index.genindex_redirects.is_empty());
}

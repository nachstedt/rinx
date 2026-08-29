//! `.. index::` directive tests for [`super::analyze`]: which general-index
//! entries it registers, and where in the node tree it finds them.

use super::*;

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

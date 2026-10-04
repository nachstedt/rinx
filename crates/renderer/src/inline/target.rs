//! Where an inline cross-reference leads, from the same resolution its
//! writer uses — the dispatcher behind [`crate::ReferenceResolver`].

use rinx_ast::InlineNode;

use super::any_reference::{AnyRef, any_target};
use super::doc_reference::doc_target;
use super::domain_object_reference::{DomainObjectRef, domain_object_target};
use super::entity_reference::entity_target;
use super::math::equation_target;
use super::number_reference::{NumRef, number_reference_target};
use super::option_reference::option_target;
use super::reference::label_target;
use super::term_reference::term_target;
use crate::resolution::AnyResolver;
use crate::{ReferenceResolver, ReferenceTarget};

/// Where `inline` leads when the page `resolver` holds renders it — `None`
/// for anything but a cross-reference resolved against the project index,
/// and for one that would be drawn broken or unlinked.
pub(crate) fn reference_target(
    inline: &InlineNode,
    resolver: &ReferenceResolver<'_>,
) -> Option<ReferenceTarget> {
    let index = resolver.index;
    let doc_path = resolver.doc_path;
    match inline {
        InlineNode::Reference {
            target, inventory, ..
        } => label_target(index, target, inventory, doc_path),
        InlineNode::DocReference {
            target,
            link,
            inventory,
            ..
        } => link
            .then(|| doc_target(index, target, inventory, doc_path))
            .flatten(),
        InlineNode::TermReference {
            term, inventory, ..
        } => term_target(index, term, inventory, doc_path),
        InlineNode::NumberReference {
            title,
            target,
            link,
            span,
        } => number_reference_target(
            NumRef {
                title: title.as_ref(),
                target,
                link: *link,
                span: *span,
            },
            index,
            &resolver.numbering,
        ),
        InlineNode::EquationReference { label, .. } => equation_target(index, label),
        InlineNode::DomainObjectReference { .. }
        | InlineNode::OptionReference { .. }
        | InlineNode::AnyReference { .. } => scoped_reference_target(inline, resolver),
        InlineNode::EntityReference { role, target, .. } => {
            entity_target(role, target, &resolver.entities)
        }
        // Listed rather than caught by a `_`, so a reference variant added
        // later is a compile error here instead of silently having no target.
        // These either link outside the index — a URL, a registry, a file —
        // or are no reference at all.
        InlineNode::Text(_)
        | InlineNode::Emphasis(_)
        | InlineNode::Strong(_)
        | InlineNode::Literal(_)
        | InlineNode::Program(_)
        | InlineNode::Script { .. }
        | InlineNode::TitleReference(_)
        | InlineNode::Hyperlink { .. }
        | InlineNode::AnonymousReference { .. }
        | InlineNode::AnonymousHyperlink { .. }
        | InlineNode::DownloadReference { .. }
        | InlineNode::RefusedRole { .. }
        | InlineNode::RegistryReference { .. }
        | InlineNode::DocutilsPepReference { .. }
        | InlineNode::DocutilsRfcReference { .. }
        | InlineNode::IndexReference { .. }
        | InlineNode::Math { .. }
        | InlineNode::Code { .. }
        | InlineNode::SubstitutionReference { .. }
        | InlineNode::InlineImage(_) => None,
    }
}

/// [`reference_target`] for the roles resolved against the scope they are
/// written in — the domain roles, `:option:` and `:any:` — split out to keep
/// that function readable, as `render_cross_reference` splits its arms.
fn scoped_reference_target(
    inline: &InlineNode,
    resolver: &ReferenceResolver<'_>,
) -> Option<ReferenceTarget> {
    let index = resolver.index;
    let doc_path = resolver.doc_path;
    let scope = resolver.scopes.scope_at(inline);
    match inline {
        InlineNode::DomainObjectReference {
            object_type,
            name,
            display,
            link,
            search_order,
            span,
            inventory,
        } => domain_object_target(
            DomainObjectRef {
                object_type: *object_type,
                name,
                display,
                link: *link,
                search_order: *search_order,
                span: *span,
                inventory,
            },
            &resolver.domains,
            doc_path,
            scope,
        ),
        InlineNode::OptionReference {
            target, inventory, ..
        } => option_target(
            target,
            inventory,
            &resolver.options,
            scope.program.current(),
            doc_path,
        ),
        InlineNode::AnyReference {
            display,
            target,
            link,
            span,
            inventory,
        } => any_target(
            AnyRef {
                title: display.as_deref(),
                target,
                link: *link,
                span: *span,
                inventory,
            },
            &AnyResolver {
                index,
                domains: &resolver.domains,
                options: &resolver.options,
            },
            scope,
            doc_path,
        ),
        _ => unreachable!("reference_target routes only scoped references here"),
    }
}

#[cfg(test)]
mod tests {
    use rinx_ast::{
        Directive, Document, EntityId, InventorySelector, Node, ObjectType, PyObjectType,
        SectionId, StdObjectType, TargetName, TargetSearchOrder,
    };
    use rinx_entity::EntitySchema;
    use rinx_index::{
        DocumentNumbers, EntityRecord, EquationLocation, NumrefSubject, NumrefTarget, ProjectIndex,
    };

    use std::collections::BTreeMap;

    use crate::config::SiteConfig;
    use crate::{Destination, ReferenceResolver, ReferenceTarget};

    /// Where `reference` leads when written in `guide/intro.rst`, after
    /// `before` — the nodes that set its scope.
    fn target_of(
        index: &ProjectIndex,
        before: Vec<Node>,
        reference: rinx_ast::InlineNode,
    ) -> Option<ReferenceTarget> {
        let mut nodes = before;
        nodes.push(Node::Paragraph(vec![reference]));
        let doc = Document::new("guide/intro.rst".to_string(), nodes);
        let config = SiteConfig::default();
        let resolver = ReferenceResolver::new(&doc, index, &config, EntitySchema::empty_ref());
        let Some(Node::Paragraph(inlines)) = doc.nodes.last() else {
            unreachable!()
        };
        resolver.resolve(&inlines[0])
    }

    fn in_document(title: &str, doc_path: &str, anchor: Option<&str>) -> ReferenceTarget {
        ReferenceTarget {
            title: title.to_string(),
            destination: Destination::Document {
                doc_path: doc_path.to_string(),
                anchor: anchor.map(str::to_string),
            },
        }
    }

    fn label(target: &str) -> rinx_ast::InlineNode {
        rinx_ast::InlineNode::Reference {
            display: Some("written title".to_string()),
            target: target.to_string(),
            span: None,
            inventory: InventorySelector::Any,
        }
    }

    #[test]
    fn test_reference_target_leads_a_label_to_its_section_title_and_anchor() {
        // Given
        let mut index = ProjectIndex::default();
        index
            .targets
            .insert(TargetName::new("install"), "setup.rst".to_string());
        index
            .target_titles
            .insert(TargetName::new("install"), "Installing".to_string());

        // When
        let found = target_of(&index, Vec::new(), label("install"));

        // Then — the target's own title, not the one this reference wrote
        assert_eq!(
            found,
            Some(in_document("Installing", "setup.rst", Some("install")))
        );
    }

    #[test]
    fn test_reference_target_has_none_for_a_label_no_document_defines() {
        // Given
        let index = ProjectIndex::default();

        // When / Then
        assert_eq!(target_of(&index, Vec::new(), label("missing")), None);
    }

    #[test]
    fn test_reference_target_leads_genindex_to_the_generated_page() {
        // Given
        let index = ProjectIndex::default();

        // When
        let found = target_of(&index, Vec::new(), label("genindex"));

        // Then
        assert_eq!(
            found,
            Some(ReferenceTarget {
                title: "Index".to_string(),
                destination: Destination::GeneratedPage {
                    path: "genindex.html".to_string(),
                },
            })
        );
    }

    #[test]
    fn test_reference_target_leads_a_label_another_site_lists_to_its_address() {
        // Given
        let index = crate::test_support::index_linking_into_python();

        // When
        let found = target_of(&index, Vec::new(), label("tut-intro"));

        // Then
        assert_eq!(
            found,
            Some(ReferenceTarget {
                title: "An Informal Introduction to Python".to_string(),
                destination: Destination::External {
                    url: "https://docs.python.org/3/tutorial/introduction.html#tut-intro"
                        .to_string(),
                    site: "(in Python v3.12)".to_string(),
                },
            })
        );
    }

    #[test]
    fn test_reference_target_leads_a_doc_reference_relative_to_its_document() {
        // Given
        let mut index = ProjectIndex::default();
        index.documents.insert("setup.rst".to_string());
        index
            .document_titles
            .insert("setup.rst".to_string(), "Setup".to_string());
        let reference = rinx_ast::InlineNode::DocReference {
            display: None,
            target: "../setup".to_string(),
            link: true,
            span: None,
            inventory: InventorySelector::Any,
        };

        // When
        let found = target_of(&index, Vec::new(), reference);

        // Then
        assert_eq!(found, Some(in_document("Setup", "setup.rst", None)));
    }

    #[test]
    fn test_reference_target_has_none_for_a_doc_reference_written_unlinked() {
        // Given — the `!` form is never looked up
        let mut index = ProjectIndex::default();
        index.documents.insert("guide/setup.rst".to_string());
        let reference = rinx_ast::InlineNode::DocReference {
            display: None,
            target: "setup".to_string(),
            link: false,
            span: None,
            inventory: InventorySelector::Any,
        };

        // When / Then
        assert_eq!(target_of(&index, Vec::new(), reference), None);
    }

    #[test]
    fn test_reference_target_leads_a_term_to_its_glossary_entry() {
        // Given
        let mut index = ProjectIndex::default();
        index
            .glossary_terms
            .insert(TargetName::new("widget"), "glossary.rst".to_string());
        let reference = rinx_ast::InlineNode::TermReference {
            display: "widgets".to_string(),
            term: "widget".to_string(),
            span: None,
            inventory: InventorySelector::Any,
        };

        // When
        let found = target_of(&index, Vec::new(), reference);

        // Then
        assert_eq!(
            found,
            Some(in_document("widget", "glossary.rst", Some("term-widget")))
        );
    }

    #[test]
    fn test_reference_target_titles_a_numref_with_the_number_it_shows() {
        // Given — a numbered section, which needs no `numfig`
        let mut index = ProjectIndex::default();
        let usage = SectionId::from_title("Usage");
        index.numref_targets.insert(
            TargetName::new("usage"),
            NumrefTarget {
                doc_path: "setup.rst".to_string(),
                subject: NumrefSubject::Section(usage.clone()),
            },
        );
        let mut sections = DocumentNumbers::default();
        sections.set_section(&usage, vec![2, 3]);
        index
            .section_numbers
            .insert("setup.rst".to_string(), sections);
        let reference = rinx_ast::InlineNode::NumberReference {
            title: None,
            target: "usage".to_string(),
            link: true,
            span: None,
        };

        // When
        let found = target_of(&index, Vec::new(), reference);

        // Then
        assert_eq!(
            found,
            Some(in_document("Section 2.3", "setup.rst", Some("usage")))
        );
    }

    #[test]
    fn test_reference_target_titles_an_equation_with_its_number() {
        // Given
        let mut index = ProjectIndex::default();
        index.equations.insert(
            TargetName::new("euler"),
            EquationLocation::new("math.rst", 3),
        );
        let reference = rinx_ast::InlineNode::EquationReference {
            label: "euler".to_string(),
            span: None,
        };

        // When
        let found = target_of(&index, Vec::new(), reference);

        // Then
        assert_eq!(
            found,
            Some(in_document("(3)", "math.rst", Some("equation-euler")))
        );
    }

    #[test]
    fn test_reference_target_resolves_a_domain_object_in_the_scope_it_is_written_in() {
        // Given — a bare name, resolvable only through the current module
        let function = ObjectType::Py(PyObjectType::Function);
        let mut index = ProjectIndex::default();
        index.insert_domain_object(function, "pkg.run", "api.rst");
        let current_module = Node::Directive(Directive::PyCurrentModule {
            module: Some("pkg".to_string()),
        });
        let reference = rinx_ast::InlineNode::DomainObjectReference {
            object_type: function,
            name: "run".to_string(),
            display: "run()".to_string(),
            link: true,
            search_order: TargetSearchOrder::default(),
            span: None,
            inventory: InventorySelector::Any,
        };

        // When
        let found = target_of(&index, vec![current_module], reference);

        // Then
        let anchor = rinx_ast::build_domain_object_key(function, "pkg.run");
        assert_eq!(
            found,
            Some(in_document("pkg.run", "api.rst", Some(anchor.as_str())))
        );
    }

    #[test]
    fn test_reference_target_resolves_an_option_under_the_ambient_program() {
        // Given
        let option = ObjectType::Std(StdObjectType::Cmdoption);
        let mut index = ProjectIndex::default();
        index.insert_domain_object(option, "tool.-v", "cli.rst");
        let program = Node::Directive(Directive::StdProgram {
            name: Some("tool".to_string()),
        });
        let reference = rinx_ast::InlineNode::OptionReference {
            display: "-v".to_string(),
            target: "-v".to_string(),
            span: None,
            inventory: InventorySelector::Any,
        };

        // When
        let found = target_of(&index, vec![program], reference);

        // Then
        let anchor = rinx_ast::build_domain_object_key(option, "tool.-v");
        assert_eq!(
            found,
            Some(in_document("tool.-v", "cli.rst", Some(anchor.as_str())))
        );
    }

    #[test]
    fn test_reference_target_leads_an_any_reference_to_its_one_hit() {
        // Given
        let mut index = ProjectIndex::default();
        index
            .glossary_terms
            .insert(TargetName::new("widget"), "glossary.rst".to_string());
        let reference = rinx_ast::InlineNode::AnyReference {
            display: None,
            target: "widget".to_string(),
            link: true,
            span: None,
            inventory: InventorySelector::Any,
        };

        // When
        let found = target_of(&index, Vec::new(), reference);

        // Then
        assert_eq!(
            found,
            Some(in_document("widget", "glossary.rst", Some("term-widget")))
        );
    }

    #[test]
    fn test_reference_target_has_none_for_an_ambiguous_any_reference() {
        // Given — a label and a term of one name
        let mut index = ProjectIndex::default();
        index
            .glossary_terms
            .insert(TargetName::new("widget"), "glossary.rst".to_string());
        index
            .targets
            .insert(TargetName::new("widget"), "setup.rst".to_string());
        let reference = rinx_ast::InlineNode::AnyReference {
            display: None,
            target: "widget".to_string(),
            link: true,
            span: None,
            inventory: InventorySelector::Any,
        };

        // When / Then
        assert_eq!(target_of(&index, Vec::new(), reference), None);
    }

    #[test]
    fn test_reference_target_titles_an_entity_with_its_own_title() {
        // Given
        let mut index = ProjectIndex::default();
        index.entities.insert(
            EntityId::new("REQ_001").unwrap(),
            EntityRecord {
                type_name: "req".to_string(),
                doc_path: "reqs.rst".to_string(),
                title: Some("Fast builds".to_string()),
                attributes: BTreeMap::default(),
                outgoing: BTreeMap::default(),
                uml: BTreeMap::default(),
            },
        );
        let reference = rinx_ast::InlineNode::EntityReference {
            role: "entity".to_string(),
            target: "REQ_001".to_string(),
            display: "REQ_001".to_string(),
            span: None,
        };

        // When
        let found = target_of(&index, Vec::new(), reference);

        // Then
        assert_eq!(
            found,
            Some(in_document(
                "Fast builds",
                "reqs.rst",
                Some(&rinx_index::entity_anchor("REQ_001"))
            ))
        );
    }

    #[test]
    fn test_reference_target_has_none_for_text() {
        // Given
        let index = ProjectIndex::default();

        // When / Then
        assert_eq!(
            target_of(
                &index,
                Vec::new(),
                rinx_ast::InlineNode::Text("prose".to_string())
            ),
            None
        );
    }
}

//! `:any:` cross-reference rendering.
//!
//! The search is [`crate::resolution::AnyResolver`]'s; what is decided here is
//! what its outcome looks like. A single hit is drawn exactly as the role that
//! names that kind of target would draw it — through the same href builders —
//! with an extra `any` class where that role's markup has a class list, as
//! Sphinx adds one.

use std::fmt::Write as _;

use rinx_ast::{InventorySelector, Span};
use rinx_index::{ProjectIndex, relative_doc_href};
use rinx_scope::Scope;

use super::doc_reference::write_doc_link;
use super::domain_object_reference::domain_object_href;
use super::external_link::write_external_link;
use super::math::write_equation_link;
use super::reference::{label_href, label_link_text};
use super::term_reference::term_href;
use crate::blocks::equation_anchor_id;
use crate::resolution::{AnyHit, AnyResolution, AnyResolver, unresolved_kind};
use crate::{BrokenLink, BrokenLinkKind};

/// An `:any:` as the author wrote it.
#[derive(Debug, Clone, Copy)]
pub(super) struct AnyRef<'a> {
    /// The explicit title of the `Title <target>` form, if one was written.
    pub title: Option<&'a str>,
    /// The name to search every kind of target for.
    pub target: &'a str,
    /// `false` for the `!` form, which is never looked up.
    pub link: bool,
    /// Where the role was written, when the parser could place it.
    pub span: Option<Span>,
    /// Which sites may define the target — see [`InventorySelector`].
    pub inventory: &'a InventorySelector,
}

/// Renders an `:any:` cross-reference written inside `scope`.
///
/// Exactly one hit links; none, or several, draws the broken-link fallback
/// and reports why. Several is reported rather than resolved to the first, as
/// Sphinx does: which hit comes first is an accident of search order the
/// reader cannot see.
pub(super) fn render_inline_any_reference(
    html: &mut String,
    reference: AnyRef<'_>,
    resolver: &AnyResolver<'_, '_>,
    scope: &Scope,
    doc_path: &str,
    broken_links: &mut Vec<BrokenLink>,
) {
    let AnyRef {
        title,
        target,
        link,
        span,
        inventory,
    } = reference;
    let literal = any_literal(title.unwrap_or(target));
    if !link {
        html.push_str(&literal);
        return;
    }
    let mut report = |html: &mut String, kind| {
        let _ = write!(html, "<a href=\"#\" class=\"broken-link\">{literal}</a>");
        broken_links.push(BrokenLink {
            kind,
            target: target.to_string(),
            span,
        });
    };
    match resolver.resolve(scope, scope.program.current(), doc_path, target, inventory) {
        AnyResolution::Local(hits) => {
            if let [hit] = &hits[..] {
                write_hit(html, hit, title, target, resolver.index, doc_path);
            } else {
                report(
                    html,
                    BrokenLinkKind::AmbiguousAnyReference {
                        candidates: hits
                            .iter()
                            .map(|hit| hit.disambiguating_role(target))
                            .collect(),
                    },
                );
            }
        }
        AnyResolution::External(hit) => {
            let shown = title.unwrap_or_else(|| hit.target.display_text());
            // An inventory gives a title only to what has one — a label or a
            // document — and those Sphinx shows as text, not code.
            let inner = if hit.target.display_name.is_some() {
                format!(
                    "<span class=\"xref any\">{}</span>",
                    html_escape::encode_text(shown)
                )
            } else {
                any_literal(shown)
            };
            write_external_link(html, &hit, doc_path, &inner);
        }
        AnyResolution::NotFound => report(
            html,
            unresolved_kind(
                inventory,
                &resolver.index.external_inventories,
                BrokenLinkKind::AnyReference,
            ),
        ),
    }
}

/// The literal an `:any:` that found no single target shows, as Sphinx
/// shows it.
fn any_literal(text: &str) -> String {
    format!(
        "<code class=\"xref any docutils literal\">{}</code>",
        html_escape::encode_text(text)
    )
}

/// The href a page at `doc_path` links `hit` by — each kind's own role's.
fn hit_href(hit: &AnyHit<'_>, index: &ProjectIndex, doc_path: &str) -> String {
    match hit {
        AnyHit::Label {
            name,
            doc_path: target_doc,
        } => label_href(index, name, target_doc, doc_path),
        AnyHit::Term {
            name,
            doc_path: term_doc,
        } => term_href(term_doc, name.as_str(), doc_path),
        AnyHit::Option {
            qualified_name,
            doc_path: target_doc,
        } => domain_object_href(
            rinx_ast::ObjectType::Std(rinx_ast::StdObjectType::Cmdoption),
            qualified_name,
            target_doc,
            doc_path,
        ),
        AnyHit::Document {
            doc_path: target_doc,
        } => relative_doc_href(target_doc, doc_path),
        AnyHit::Equation { label, location } => format!(
            "{}#{}",
            relative_doc_href(&location.doc_path, doc_path),
            equation_anchor_id(label)
        ),
        AnyHit::DomainObject {
            object_type,
            qualified_name,
            doc_path: target_doc,
        } => domain_object_href(*object_type, qualified_name, target_doc, doc_path),
    }
}

/// Writes the link to the one target an `:any:` resolved to.
fn write_hit(
    html: &mut String,
    hit: &AnyHit<'_>,
    title: Option<&str>,
    target: &str,
    index: &ProjectIndex,
    doc_path: &str,
) {
    let shown = title.unwrap_or(target);
    let inner = match hit {
        // An equation shows its number whatever was written, as `:eq:` does.
        AnyHit::Equation { label, location } => {
            write_equation_link(html, label, location, doc_path);
            return;
        }
        AnyHit::Label { name, .. } => {
            html_escape::encode_text(label_link_text(index, title, name, target)).into_owned()
        }
        // Drawn whole by `:doc:`'s own writer, `<no title>` included.
        AnyHit::Document {
            doc_path: target_doc,
        } => {
            write_doc_link(html, index, title, target_doc, doc_path);
            return;
        }
        AnyHit::Term { .. } => format!(
            "<span class=\"xref any std std-term\">{}</span>",
            html_escape::encode_text(shown)
        ),
        AnyHit::Option { .. } => format!(
            "<code class=\"xref any std cmdoption docutils literal\">{}</code>",
            html_escape::encode_text(shown)
        ),
        AnyHit::DomainObject { object_type, .. } => format!(
            "<code class=\"xref any {} {} docutils literal\">{}</code>",
            object_type.domain().as_str(),
            object_type.as_str(),
            html_escape::encode_text(shown)
        ),
    };
    let href = hit_href(hit, index, doc_path);
    let href_attr = html_escape::encode_double_quoted_attribute(&href);
    let _ = write!(
        html,
        "<a class=\"reference internal\" href=\"{href_attr}\">{inner}</a>"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolution::{DomainObjectResolver, OptionResolver};
    use rinx_ast::{CObjectType, ObjectType, PyObjectType, TargetName};
    use rinx_index::{EquationLocation, TargetLocation};

    /// Renders `` :any:`written` `` from `index.rst`, returning the HTML and
    /// what was reported.
    fn render(
        index: &ProjectIndex,
        scope: &Scope,
        written: AnyRef<'_>,
    ) -> (String, Vec<BrokenLink>) {
        let domains = DomainObjectResolver::new(index);
        let options = OptionResolver::new(index);
        let mut html = String::new();
        let mut broken_links = Vec::new();
        render_inline_any_reference(
            &mut html,
            written,
            &AnyResolver {
                index,
                domains: &domains,
                options: &options,
            },
            scope,
            "index.rst",
            &mut broken_links,
        );
        (html, broken_links)
    }

    fn bare(target: &str) -> AnyRef<'_> {
        AnyRef {
            title: None,
            target,
            link: true,
            span: None,
            inventory: &InventorySelector::Any,
        }
    }

    #[test]
    fn test_a_label_hit_shows_its_section_title() {
        // Given
        let mut index = ProjectIndex::default();
        index.targets.insert(
            TargetName::new("install"),
            TargetLocation::Internal("guide.rst".to_string()),
        );
        index
            .target_titles
            .insert(TargetName::new("install"), "Installing".to_string());

        // When
        let (html, broken) = render(&index, &Scope::default(), bare("install"));

        // Then
        assert_eq!(
            html,
            "<a class=\"reference internal\" href=\"guide.html#install\">Installing</a>"
        );
        assert!(broken.is_empty());
    }

    #[test]
    fn test_a_python_object_hit_is_drawn_as_its_role_would_with_an_any_class() {
        // Given
        let mut index = ProjectIndex::default();
        index.insert_domain_object(ObjectType::Py(PyObjectType::Function), "pkg.run", "api.rst");
        let mut scope = Scope::default();
        scope.python.set_module("pkg");

        // When
        let (html, _) = render(&index, &scope, bare("run()"));

        // Then — the parens stay in the text, as the author wrote them
        assert_eq!(
            html,
            "<a class=\"reference internal\" href=\"api.html#py:function:pkg.run\">\
             <code class=\"xref any py function docutils literal\">run()</code></a>"
        );
    }

    #[test]
    fn test_a_term_hit_links_the_glossary_entry() {
        // Given
        let mut index = ProjectIndex::default();
        index
            .glossary_terms
            .insert(TargetName::new("Widget"), "glossary.rst".to_string());

        // When
        let (html, _) = render(&index, &Scope::default(), bare("widget"));

        // Then
        assert_eq!(
            html,
            "<a class=\"reference internal\" href=\"glossary.html#term-widget\">\
             <span class=\"xref any std std-term\">widget</span></a>"
        );
    }

    #[test]
    fn test_a_document_hit_shows_the_document_title() {
        // Given
        let mut index = ProjectIndex::default();
        index
            .document_titles
            .insert("guide.rst".to_string(), "The guide".to_string());

        // When
        let (html, _) = render(&index, &Scope::default(), bare("guide"));

        // Then
        assert_eq!(
            html,
            "<a class=\"reference internal\" href=\"guide.html\">\
             <span class=\"doc\">The guide</span></a>"
        );
    }

    #[test]
    fn test_a_titleless_document_hit_shows_no_title_as_doc_does() {
        // Given
        let mut index = ProjectIndex::default();
        index.documents.insert("notes.rst".to_string());

        // When
        let (html, broken) = render(&index, &Scope::default(), bare("notes"));

        // Then
        assert_eq!(
            html,
            "<a class=\"reference internal\" href=\"notes.html\">\
             <span class=\"doc\">&lt;no title&gt;</span></a>"
        );
        assert!(broken.is_empty());
    }

    #[test]
    fn test_an_equation_hit_shows_its_number_whatever_the_title() {
        // Given
        let mut index = ProjectIndex::default();
        index.equations.insert(
            TargetName::new("euler"),
            EquationLocation::new("math.rst", 2),
        );
        let written = AnyRef {
            title: Some("Euler"),
            ..bare("euler")
        };

        // When
        let (html, _) = render(&index, &Scope::default(), written);

        // Then
        assert!(html.contains("<span class=\"eqno\">(2)</span>"), "{html}");
        assert!(html.contains("href=\"math.html#equation-euler\""), "{html}");
    }

    #[test]
    fn test_an_option_hit_uses_the_ambient_program() {
        // Given
        let mut index = ProjectIndex::default();
        index.insert_domain_object(
            ObjectType::Std(rinx_ast::StdObjectType::Cmdoption),
            "prog.--verbose",
            "cli.rst",
        );
        let mut scope = Scope::default();
        scope.program.set("prog");

        // When
        let (html, broken) = render(&index, &scope, bare("--verbose"));

        // Then
        assert!(
            html.contains(
                "<code class=\"xref any std cmdoption docutils literal\">--verbose</code>"
            ),
            "{html}"
        );
        assert!(broken.is_empty());
    }

    #[test]
    fn test_an_explicit_title_replaces_the_target_text() {
        // Given
        let mut index = ProjectIndex::default();
        index.insert_domain_object(ObjectType::C(CObjectType::Macro), "MY_MACRO", "c.rst");
        let written = AnyRef {
            title: Some("the macro"),
            ..bare("MY_MACRO")
        };

        // When
        let (html, _) = render(&index, &Scope::default(), written);

        // Then
        assert!(
            html.contains("<code class=\"xref any c macro docutils literal\">the macro</code>"),
            "{html}"
        );
    }

    #[test]
    fn test_a_suppressed_reference_is_a_literal_that_looks_nothing_up() {
        // Given — nothing at all is indexed
        let written = AnyRef {
            link: false,
            ..bare("pkg.run")
        };

        // When
        let (html, broken) = render(&ProjectIndex::default(), &Scope::default(), written);

        // Then
        assert_eq!(
            html,
            "<code class=\"xref any docutils literal\">pkg.run</code>"
        );
        assert!(broken.is_empty());
    }

    #[test]
    fn test_an_ambiguous_reference_links_nothing_and_names_every_candidate() {
        // Given — a label and a Python function both called `shared`
        let mut index = ProjectIndex::default();
        index.targets.insert(
            TargetName::new("shared"),
            TargetLocation::Internal("other.rst".to_string()),
        );
        index.insert_domain_object(ObjectType::Py(PyObjectType::Function), "shared", "api.rst");

        // When
        let (html, broken) = render(&index, &Scope::default(), bare("shared"));

        // Then
        assert_eq!(
            html,
            "<a href=\"#\" class=\"broken-link\">\
             <code class=\"xref any docutils literal\">shared</code></a>"
        );
        assert_eq!(
            broken,
            vec![BrokenLink {
                kind: BrokenLinkKind::AmbiguousAnyReference {
                    candidates: vec![
                        ":std:ref:`shared`".to_string(),
                        ":py:func:`shared`".to_string(),
                    ],
                },
                target: "shared".to_string(),
                span: None,
            }]
        );
    }

    #[test]
    fn test_an_unknown_target_is_reported_broken() {
        // Given / When
        let (html, broken) = render(&ProjectIndex::default(), &Scope::default(), bare("nothing"));

        // Then
        assert!(html.contains("class=\"broken-link\""), "{html}");
        assert_eq!(broken[0].kind, BrokenLinkKind::AnyReference);
        assert_eq!(broken[0].target, "nothing");
    }

    #[test]
    fn test_an_external_object_hit_is_drawn_as_a_literal_link_into_the_other_site() {
        // Given — nothing here is called `dict`, but Python's inventory is
        let index = crate::test_support::index_linking_into_python();

        // When
        let (html, broken) = render(&index, &Scope::default(), bare("dict"));

        // Then
        assert!(
            html.starts_with("<a class=\"reference external\""),
            "{html}"
        );
        assert!(
            html.contains("<code class=\"xref any docutils literal\">dict</code>"),
            "{html}"
        );
        assert!(broken.is_empty());
    }
}

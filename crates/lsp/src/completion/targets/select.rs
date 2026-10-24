//! Which candidates a target completion offers, and how each is written.
//!
//! A candidate is offered only where the page would link it: its full
//! spelling is resolved, as the role being typed would resolve it where it is
//! written, and a candidate that leads nowhere — or into an ambiguity — is
//! left out. It is then written in its shortest spelling that leads to the
//! same place from there, preferring one that continues what the author has
//! typed — so an object of the current class is offered by its bare name,
//! and the list ranks it first, being the nearest.

use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionItemLabelDetails, CompletionTextEdit,
    Documentation, Range, TextEdit,
};
use rinx_ast::InlineNode;
use rinx_renderer::{Destination, ReferenceResolver, ReferenceTarget};

use super::candidates::Candidate;
use super::is_scoped;

/// How many items one answer holds at most. A larger list is cut to its
/// nearest items and marked incomplete, so the editor asks again as the
/// author types and the server filters it.
pub(super) const MAX_ITEMS: usize = 200;

/// The items completing a target with one of `candidates`, each written over
/// `range`, documented by `document` and said to lie in the file `written_in`
/// names for it, given its full spelling — or else its target's document —
/// and whether the answer is incomplete.
///
/// `reference` stands for the role being typed, `resolver` answers for the
/// document it is written in, and `typed` is what the author has written of
/// the target. A role resolved in its scope — a domain role, `:option:`,
/// `:any:` — is filtered by what is typed and always marked incomplete,
/// since the spelling chosen depends on it; any other role is filtered only
/// when it has more candidates than one answer holds.
pub(super) fn select(
    candidates: Vec<Candidate>,
    reference: &InlineNode,
    resolver: &ReferenceResolver<'_>,
    typed: &str,
    range: Range,
    document: &dyn Fn(&ReferenceTarget) -> Option<Documentation>,
    written_in: &dyn Fn(&ReferenceTarget, &str) -> Option<String>,
) -> (Vec<CompletionItem>, bool) {
    let scoped = is_scoped(reference);
    let filtered = scoped || candidates.len() > MAX_ITEMS;
    let typed_lower = typed.to_lowercase();
    let mut matched: Vec<Candidate> = candidates
        .into_iter()
        .filter(|candidate| {
            !filtered
                || candidate
                    .forms
                    .iter()
                    .any(|form| continues(form, &typed_lower))
        })
        .collect();
    let checked_lazily = matched.len() > CHECK_ALL;
    if checked_lazily {
        matched.sort_by_cached_key(|candidate| rank(candidate.external, &candidate.forms[0]));
    }
    let mut chosen: Vec<Chosen> = Vec::new();
    for candidate in &matched {
        if checked_lazily && chosen.len() > MAX_ITEMS {
            break;
        }
        chosen.extend(choose(candidate, reference, resolver, &typed_lower));
    }
    chosen.sort_by_cached_key(|chosen| rank(chosen.external, &chosen.written));
    let cut = chosen.len() > MAX_ITEMS;
    chosen.truncate(MAX_ITEMS);
    let items = chosen
        .into_iter()
        .map(|chosen| item(chosen, range, document, written_in))
        .collect();
    (items, scoped || cut)
}

/// How many candidates every one of which is resolved before the nearest
/// are chosen. Past it — `:any:` over a large project with nothing typed —
/// they are resolved in the order their shortest spelling ranks, only until
/// the answer is full: resolving tens of thousands cost a quarter second, and
/// a list that long is refined as the author types anyway.
const CHECK_ALL: usize = 5000;

/// Where an item written `written` ranks: this site's before another's, then
/// the fewer dotted parts — the nearer to where the reference is written —
/// then by name.
fn rank(external: bool, written: &str) -> (bool, usize, String) {
    (external, segments(written), written.to_lowercase())
}

/// A candidate as it is offered: the spelling written, the one the editor
/// filters by, and where it leads.
#[derive(Debug)]
struct Chosen {
    written: String,
    /// The candidate's full spelling, which names it whatever is written.
    full: String,
    filtered_by: String,
    kind: CompletionItemKind,
    external: bool,
    target: ReferenceTarget,
}

/// `candidate` as it is offered, or `None` when its full spelling leads
/// nowhere: written in its shortest spelling leading to the same place —
/// among those continuing `typed` first.
fn choose(
    candidate: &Candidate,
    reference: &InlineNode,
    resolver: &ReferenceResolver<'_>,
    typed: &str,
) -> Option<Chosen> {
    let forms = &candidate.forms;
    let target = resolver.resolve_as(reference, forms.last()?)?;
    // Each spelling is resolved at most once, and only when asked about.
    let mut answers: Vec<Option<Option<ReferenceTarget>>> = vec![None; forms.len()];
    let mut leads_there = |at: usize| {
        answers[at]
            .get_or_insert_with(|| resolver.resolve_as(reference, &forms[at]))
            .as_ref()
            == Some(&target)
    };
    let written = (0..forms.len())
        .filter(|&at| continues(&forms[at], typed))
        .find(|&at| leads_there(at))
        .or_else(|| (0..forms.len()).find(|&at| leads_there(at)))
        .map(|at| forms[at].clone())?;
    let filtered_by = forms
        .iter()
        .find(|form| continues(form, typed))
        .unwrap_or(&written)
        .clone();
    Some(Chosen {
        written,
        full: forms.last()?.clone(),
        filtered_by,
        kind: candidate.kind,
        external: candidate.external,
        target,
    })
}

/// The completion item offering `chosen`, written over `range`: titled as a
/// reference to it with no title of its own would show, saying where it is —
/// the file `written_in` names, else the document it leads to — and
/// documented by `document`.
fn item(
    chosen: Chosen,
    range: Range,
    document: &dyn Fn(&ReferenceTarget) -> Option<Documentation>,
    written_in: &dyn Fn(&ReferenceTarget, &str) -> Option<String>,
) -> CompletionItem {
    let description = written_in(&chosen.target, &chosen.full).unwrap_or_else(|| {
        match &chosen.target.destination {
            Destination::Document { doc_path, .. } => doc_path.clone(),
            Destination::GeneratedPage { path } => path.clone(),
            Destination::External { site, .. } => site.clone(),
        }
    });
    CompletionItem {
        documentation: document(&chosen.target),
        detail: Some(chosen.target.title),
        label_details: Some(CompletionItemLabelDetails {
            detail: None,
            description: Some(description),
        }),
        kind: Some(chosen.kind),
        filter_text: (chosen.filtered_by != chosen.written).then_some(chosen.filtered_by),
        text_edit: Some(CompletionTextEdit::Edit(TextEdit {
            range,
            new_text: chosen.written.clone(),
        })),
        label: chosen.written,
        ..CompletionItem::default()
    }
}

/// Whether `form` continues what is typed, `typed` given in lower case.
fn continues(form: &str, typed: &str) -> bool {
    form.to_lowercase().starts_with(typed)
}

/// How many dotted parts `written` has: the fewer, the nearer the target is
/// to where the reference is written.
fn segments(written: &str) -> usize {
    written.matches('.').count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::Position;
    use rinx_ast::{
        Directive, Document, InventorySelector, Node, ObjectType, PyObjectType, TargetName,
        TargetSearchOrder,
    };
    use rinx_entity::EntitySchema;
    use rinx_index::ProjectIndex;
    use rinx_renderer::config::SiteConfig;

    fn range() -> Range {
        Range::new(Position::new(3, 6), Position::new(3, 9))
    }

    fn method_reference() -> InlineNode {
        InlineNode::DomainObjectReference {
            object_type: ObjectType::Py(PyObjectType::Method),
            name: "x".to_string(),
            display: "x".to_string(),
            link: true,
            search_order: TargetSearchOrder::default(),
            span: None,
            inventory: InventorySelector::Any,
        }
    }

    /// A document under `.. py:currentmodule:: pkg`, holding `reference`.
    fn in_module_pkg(reference: InlineNode) -> Document {
        Document::new(
            "guide.rst".to_string(),
            vec![
                Node::Directive(Directive::PyCurrentModule {
                    module: Some("pkg".to_string()),
                }),
                Node::Paragraph(vec![reference]),
            ],
        )
    }

    fn written(doc: &Document) -> &InlineNode {
        let Some(Node::Paragraph(inlines)) = doc.nodes.last() else {
            unreachable!()
        };
        &inlines[0]
    }

    fn method(forms: &[&str]) -> Candidate {
        Candidate {
            forms: forms.iter().map(ToString::to_string).collect(),
            kind: CompletionItemKind::METHOD,
            external: false,
        }
    }

    fn labels(items: &[CompletionItem]) -> Vec<&str> {
        items.iter().map(|item| item.label.as_str()).collect()
    }

    /// Two methods, `pkg.Box.open` and `other.Box.close`.
    fn index() -> ProjectIndex {
        let mut index = ProjectIndex::default();
        let method = ObjectType::Py(PyObjectType::Method);
        index.insert_domain_object(method, "pkg.Box.open", "a.rst");
        index.insert_domain_object(method, "other.Box.close", "b.rst");
        index
    }

    fn candidates() -> Vec<Candidate> {
        vec![
            method(&["close", "Box.close", "other.Box.close"]),
            method(&["open", "Box.open", "pkg.Box.open"]),
        ]
    }

    #[test]
    fn test_select_writes_each_target_as_short_as_it_still_leads_there() {
        // Given — under module `pkg`, `Box.open` reaches `pkg.Box.open`
        let index = index();
        let doc = in_module_pkg(method_reference());
        let config = SiteConfig::default();
        let resolver = ReferenceResolver::new(&doc, &index, &config, EntitySchema::empty_ref());

        // When
        let (items, incomplete) = select(
            candidates(),
            written(&doc),
            &resolver,
            "",
            range(),
            &|_| None,
            &|_, _| None,
        );

        // Then — the nearer one first; both marked incomplete, being scoped
        assert_eq!(labels(&items), ["Box.open", "other.Box.close"]);
        assert_eq!(items[0].detail.as_deref(), Some("pkg.Box.open"));
        assert_eq!(
            items[0]
                .label_details
                .as_ref()
                .and_then(|d| d.description.as_deref()),
            Some("a.rst")
        );
        assert!(incomplete);
    }

    #[test]
    fn test_select_keeps_only_what_continues_the_typed_text() {
        // Given
        let index = index();
        let doc = in_module_pkg(method_reference());
        let config = SiteConfig::default();
        let resolver = ReferenceResolver::new(&doc, &index, &config, EntitySchema::empty_ref());

        // When
        let (items, _) = select(
            candidates(),
            written(&doc),
            &resolver,
            "oth",
            range(),
            &|_| None,
            &|_, _| None,
        );

        // Then
        assert_eq!(labels(&items), ["other.Box.close"]);
    }

    #[test]
    fn test_select_writes_the_spelling_the_author_began() {
        // Given
        let index = index();
        let doc = in_module_pkg(method_reference());
        let config = SiteConfig::default();
        let resolver = ReferenceResolver::new(&doc, &index, &config, EntitySchema::empty_ref());

        // When
        let (items, _) = select(
            candidates(),
            written(&doc),
            &resolver,
            "pkg.",
            range(),
            &|_| None,
            &|_, _| None,
        );

        // Then
        assert_eq!(labels(&items), ["pkg.Box.open"]);
    }

    #[test]
    fn test_select_leaves_out_a_target_that_leads_nowhere() {
        // Given — a candidate no document defines
        let index = index();
        let doc = in_module_pkg(method_reference());
        let config = SiteConfig::default();
        let resolver = ReferenceResolver::new(&doc, &index, &config, EntitySchema::empty_ref());

        // When
        let (items, _) = select(
            vec![method(&["gone", "pkg.gone"])],
            written(&doc),
            &resolver,
            "",
            range(),
            &|_| None,
            &|_, _| None,
        );

        // Then
        assert!(items.is_empty());
    }

    #[test]
    fn test_select_filters_an_unscoped_role_only_past_the_cap() {
        // Given — a label that does not continue what is typed
        let mut index = ProjectIndex::default();
        index
            .targets
            .insert(TargetName::new("install"), "setup.rst".to_string());
        let doc = Document::new("guide.rst".to_string(), Vec::new());
        let config = SiteConfig::default();
        let resolver = ReferenceResolver::new(&doc, &index, &config, EntitySchema::empty_ref());
        let reference = InlineNode::Reference {
            display: None,
            target: "x".to_string(),
            span: None,
            inventory: InventorySelector::Any,
        };
        let candidate = Candidate {
            forms: vec!["install".to_string()],
            kind: CompletionItemKind::REFERENCE,
            external: false,
        };

        // When
        let (items, incomplete) = select(
            vec![candidate],
            &reference,
            &resolver,
            "zz",
            range(),
            &|_| None,
            &|_, _| None,
        );

        // Then — the editor filters a complete list itself
        assert_eq!(labels(&items), ["install"]);
        assert!(!incomplete);
    }

    #[test]
    fn test_select_cuts_a_list_past_what_is_resolved_whole() {
        // Given — more labels than are resolved before choosing
        let mut index = ProjectIndex::default();
        let names: Vec<String> = (0..=CHECK_ALL).map(|n| format!("label-{n:05}")).collect();
        for name in &names {
            index
                .targets
                .insert(TargetName::new(name), "setup.rst".to_string());
        }
        let doc = Document::new("guide.rst".to_string(), Vec::new());
        let config = SiteConfig::default();
        let resolver = ReferenceResolver::new(&doc, &index, &config, EntitySchema::empty_ref());
        let reference = InlineNode::Reference {
            display: None,
            target: "x".to_string(),
            span: None,
            inventory: InventorySelector::Any,
        };
        let candidates = names
            .iter()
            .rev()
            .map(|name| Candidate {
                forms: vec![name.clone()],
                kind: CompletionItemKind::REFERENCE,
                external: false,
            })
            .collect();

        // When
        let (items, incomplete) = select(
            candidates,
            &reference,
            &resolver,
            "",
            range(),
            &|_| None,
            &|_, _| None,
        );

        // Then — the first of them by rank, and the editor told to ask again
        assert_eq!(items.len(), MAX_ITEMS);
        assert_eq!(items[0].label, "label-00000");
        assert!(incomplete);
    }

    #[test]
    fn test_select_says_an_item_lies_where_written_in_names_else_in_its_document() {
        // Given — two labels of `setup.rst`, one written in a fragment
        let mut index = ProjectIndex::default();
        for name in ["install", "shared"] {
            index
                .targets
                .insert(TargetName::new(name), "setup.rst".to_string());
        }
        let doc = Document::new("guide.rst".to_string(), Vec::new());
        let config = SiteConfig::default();
        let resolver = ReferenceResolver::new(&doc, &index, &config, EntitySchema::empty_ref());
        let reference = InlineNode::Reference {
            display: None,
            target: "x".to_string(),
            span: None,
            inventory: InventorySelector::Any,
        };
        let candidates = ["install", "shared"]
            .map(|name| Candidate {
                forms: vec![name.to_string()],
                kind: CompletionItemKind::REFERENCE,
                external: false,
            })
            .to_vec();

        // When
        let (items, _) = select(
            candidates,
            &reference,
            &resolver,
            "",
            range(),
            &|_| None,
            &|_, full| (full == "shared").then(|| "parts/shared.inc".to_string()),
        );

        // Then
        let described: Vec<Option<&str>> = items
            .iter()
            .map(|item| {
                item.label_details
                    .as_ref()
                    .and_then(|details| details.description.as_deref())
            })
            .collect();
        assert_eq!(described, [Some("setup.rst"), Some("parts/shared.inc")]);
    }

    #[test]
    fn test_rank_puts_this_site_and_the_nearer_first() {
        // When / Then
        assert!(rank(false, "pkg.open") < rank(true, "open"));
        assert!(rank(false, "open") < rank(false, "Box.open"));
        assert!(rank(false, "Alpha") < rank(false, "beta"));
    }

    #[test]
    fn test_continues_ignores_case() {
        // When / Then
        assert!(continues("Box.open", "box."));
        assert!(!continues("Box.open", "open"));
    }

    #[test]
    fn test_segments_counts_the_dots() {
        // When / Then
        assert_eq!(segments("pkg.Box.open"), 2);
        assert_eq!(segments("open"), 0);
    }
}

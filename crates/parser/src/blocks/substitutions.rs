//! Resolving `|name|` substitution references against `.. |name| directive::`
//! definitions — the one whole-document pass `parse_with_ctx` runs after
//! [`super::index_ids::assign_index_ids`], for the same reason that one does:
//! a `.. index::` id and a substitution's content both depend on having seen
//! the *whole* document first, not just what came before the point they're
//! used at. A `|name|` may be written before its definition in source order,
//! which is exactly what `CPython`'s own docs do (`|release|` is defined once,
//! near the bottom of a shared prelude, and used throughout).
//!
//! Two passes: [`collect_definitions`] (read-only, so it reuses
//! [`rusty_sphinx_ast::walk_nodes`] — this is exactly the "collect matching
//! nodes anywhere in the tree" case that walker exists for) builds the name
//! table first, then [`resolve_substitutions`] walks the tree a second time,
//! mutably, splicing every reference's resolved content into place. The
//! second pass cannot reuse `walk_nodes`: it needs to reach every
//! `Vec<InlineNode>` in the tree, which `walk_nodes` deliberately never
//! descends into (see its own module doc comment), and it mutates rather
//! than merely visiting.

use std::collections::{HashMap, HashSet};

use rusty_sphinx_ast::{
    Diagnostic, DiagnosticCode, Directive, InlineNode, Node, Span, SubstitutionKind, TrimSides,
    walk_nodes,
};

use crate::diagnostics::Diagnostics;

/// Every substitution definition found in a document, keyed by exact name,
/// plus a case-insensitive index used only as a fallback when an exact match
/// fails — matching references are "case-sensitive but forgiving", per
/// docutils.
struct Definitions {
    exact: HashMap<String, SubstitutionKind>,
    by_lowercase: HashMap<String, Vec<String>>,
}

impl Definitions {
    /// The exact definition name `written` resolves to, if any: itself when
    /// it matches a definition exactly, or the sole case-insensitive match
    /// when exactly one exists. Two definitions differing only in case are
    /// deliberately left unresolvable rather than guessing between them.
    fn resolve_name(&self, written: &str) -> Option<&str> {
        if let Some((exact, _)) = self.exact.get_key_value(written) {
            return Some(exact.as_str());
        }
        match self.by_lowercase.get(&written.to_lowercase()) {
            Some(candidates) if candidates.len() == 1 => Some(candidates[0].as_str()),
            _ => None,
        }
    }
}

/// Walks `nodes` once, collecting every `.. |name| directive::` definition
/// anywhere in the tree into a lookup table, reporting (and dropping) a
/// second definition of a name already seen.
fn collect_definitions(nodes: &[Node], diagnostics: &mut Diagnostics) -> Definitions {
    let mut exact: HashMap<String, SubstitutionKind> = HashMap::new();
    let mut spans: HashMap<String, Option<Span>> = HashMap::new();
    walk_nodes(nodes, &mut |node| {
        let Node::Directive(Directive::SubstitutionDefinition(definition)) = node else {
            return;
        };
        if exact.contains_key(&definition.name) {
            diagnostics.push(Diagnostic::at(
                DiagnosticCode::SubstitutionDuplicateDefinition,
                format!(
                    "substitution '{}' is defined more than once; the first definition is used",
                    definition.name
                ),
                definition.span,
            ));
            return;
        }
        exact.insert(definition.name.clone(), definition.kind.clone());
        spans.insert(definition.name.clone(), definition.span);
    });

    let mut by_lowercase: HashMap<String, Vec<String>> = HashMap::new();
    for name in exact.keys() {
        by_lowercase
            .entry(name.to_lowercase())
            .or_default()
            .push(name.clone());
    }

    Definitions {
        exact,
        by_lowercase,
    }
}

/// The `unicode` trim sides a resolved definition asks for, or no trimming at
/// all for every other kind — `:ltrim:`/`:rtrim:`/`:trim:` are options
/// `unicode` alone has.
fn trim_sides_of(defs: &Definitions, exact_name: &str) -> TrimSides {
    match defs.exact.get(exact_name) {
        Some(SubstitutionKind::Unicode { trim, .. }) => *trim,
        _ => TrimSides::default(),
    }
}

/// Strips trailing whitespace off the last node of `out`, if it is plain
/// text — a `unicode` substitution's `:ltrim:`.
fn ltrim_end_of(out: &mut [InlineNode]) {
    if let Some(InlineNode::Text(text)) = out.last_mut() {
        let trimmed = text.trim_end();
        if trimmed.len() != text.len() {
            text.truncate(trimmed.len());
        }
    }
}

/// Strips leading whitespace off `node`, if it is plain text — a `unicode`
/// substitution's `:rtrim:`.
fn rtrim_start_of(node: &mut InlineNode) {
    if let InlineNode::Text(text) = node {
        *text = text.trim_start().to_string();
    }
}

/// The content a resolved substitution definition contributes at its point
/// of reference, memoized and cycle-checked so a `replace` chain referencing
/// itself degrades instead of recursing forever.
///
/// `visiting` names every definition currently being resolved somewhere up
/// the call stack; a name already in it means this call was reached while
/// resolving itself — directly or through a chain of other `replace`
/// substitutions, the only kind whose content can itself hold a reference.
fn resolve_definition(
    exact_name: &str,
    defs: &Definitions,
    cache: &mut HashMap<String, Vec<InlineNode>>,
    visiting: &mut HashSet<String>,
    diagnostics: &mut Diagnostics,
    ref_span: Option<Span>,
) -> Vec<InlineNode> {
    if let Some(cached) = cache.get(exact_name) {
        return cached.clone();
    }
    if !visiting.insert(exact_name.to_string()) {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::SubstitutionCircularReference,
            format!("substitution '{exact_name}' directly or indirectly refers to itself"),
            ref_span,
        ));
        return vec![InlineNode::Text(format!("|{exact_name}|"))];
    }

    let result = match defs
        .exact
        .get(exact_name)
        .expect("resolve_definition is only ever called with a name just looked up")
    {
        SubstitutionKind::Replace(nodes) => {
            resolve_inline_nodes(nodes, defs, cache, visiting, diagnostics)
        }
        SubstitutionKind::Unicode { text, .. } => vec![InlineNode::Text(text.clone())],
        SubstitutionKind::Image(options) => vec![InlineNode::InlineImage(options.clone())],
    };

    visiting.remove(exact_name);
    cache.insert(exact_name.to_string(), result.clone());
    result
}

/// Resolves every [`InlineNode::SubstitutionReference`] in `nodes`, splicing
/// each one's resolved content in place and applying a `unicode`
/// substitution's `:ltrim:`/`:rtrim:` against its immediate neighbors in this
/// same list.
///
/// Shared by every `Vec<InlineNode>` in the document tree *and* by
/// [`resolve_definition`] for a `replace` substitution's own content, so a
/// `unicode` reference written inside a `replace` definition is trimmed
/// against that definition's own surrounding text, exactly as if it had been
/// written directly in a document.
fn resolve_inline_nodes(
    nodes: &[InlineNode],
    defs: &Definitions,
    cache: &mut HashMap<String, Vec<InlineNode>>,
    visiting: &mut HashSet<String>,
    diagnostics: &mut Diagnostics,
) -> Vec<InlineNode> {
    let mut out: Vec<InlineNode> = Vec::with_capacity(nodes.len());
    let mut pending_rtrim = false;

    for node in nodes {
        let ltrim_next = pending_rtrim;
        pending_rtrim = false;

        let InlineNode::SubstitutionReference { name, span } = node else {
            let mut node = node.clone();
            if ltrim_next {
                rtrim_start_of(&mut node);
            }
            out.push(node);
            continue;
        };

        let Some(exact_name) = defs.resolve_name(name) else {
            diagnostics.push(Diagnostic::at(
                DiagnosticCode::SubstitutionUndefined,
                format!("no substitution definition found for '|{name}|'"),
                *span,
            ));
            out.push(InlineNode::Text(format!("|{name}|")));
            continue;
        };
        let exact_name = exact_name.to_string();

        if trim_sides_of(defs, &exact_name).ltrim {
            ltrim_end_of(&mut out);
        }
        let resolved = resolve_definition(&exact_name, defs, cache, visiting, diagnostics, *span);
        out.extend(resolved);
        pending_rtrim = trim_sides_of(defs, &exact_name).rtrim;
    }

    out
}

/// Resolves every substitution reference anywhere in `nodes`, mutating the
/// tree in place. Run once, at the end of [`super::parse_with_ctx`], after
/// the whole document is known.
pub(super) fn resolve_substitutions(nodes: &mut [Node], diagnostics: &mut Diagnostics) {
    let defs = collect_definitions(nodes, diagnostics);
    let mut cache: HashMap<String, Vec<InlineNode>> = HashMap::new();
    let mut visiting: HashSet<String> = HashSet::new();
    resolve_nodes(nodes, &defs, &mut cache, &mut visiting, diagnostics);
}

/// A single `Vec<InlineNode>` field's worth of resolution, applied in place.
fn resolve_list(
    list: &mut Vec<InlineNode>,
    defs: &Definitions,
    cache: &mut HashMap<String, Vec<InlineNode>>,
    visiting: &mut HashSet<String>,
    diagnostics: &mut Diagnostics,
) {
    *list = resolve_inline_nodes(list, defs, cache, visiting, diagnostics);
}

/// Walks every block-level container the document tree can hold, resolving
/// every `Vec<InlineNode>` it finds — the mutable, inline-content-reaching
/// counterpart of [`rusty_sphinx_ast::walk_nodes`], which deliberately does
/// neither. Exhaustive rather than a `_` catch-all, so a variant added later
/// that carries inline content is a compile error here rather than a
/// silently unresolved subtree.
fn resolve_nodes(
    nodes: &mut [Node],
    defs: &Definitions,
    cache: &mut HashMap<String, Vec<InlineNode>>,
    visiting: &mut HashSet<String>,
    diagnostics: &mut Diagnostics,
) {
    for node in nodes {
        match node {
            Node::Heading { text, .. } => resolve_list(text, defs, cache, visiting, diagnostics),
            Node::Paragraph(inlines) => resolve_list(inlines, defs, cache, visiting, diagnostics),
            Node::BlockQuote {
                content,
                attribution,
            } => {
                resolve_nodes(content, defs, cache, visiting, diagnostics);
                if let Some(attribution) = attribution {
                    resolve_list(attribution, defs, cache, visiting, diagnostics);
                }
            }
            Node::BulletList { items, .. } | Node::EnumeratedList { items, .. } => {
                for item in items {
                    resolve_nodes(&mut item.nodes, defs, cache, visiting, diagnostics);
                }
            }
            Node::DefinitionList { items } => {
                for item in items {
                    resolve_list(&mut item.term, defs, cache, visiting, diagnostics);
                    resolve_nodes(&mut item.definition, defs, cache, visiting, diagnostics);
                }
            }
            Node::OptionList { items } => {
                for item in items {
                    resolve_nodes(&mut item.description, defs, cache, visiting, diagnostics);
                }
            }
            Node::Table {
                header_rows,
                body_rows,
            } => {
                for row in header_rows.iter_mut().chain(body_rows.iter_mut()) {
                    for cell in &mut row.cells {
                        resolve_nodes(&mut cell.content, defs, cache, visiting, diagnostics);
                    }
                }
            }
            Node::LineBlock(items) => {
                resolve_line_block_items(items, defs, cache, visiting, diagnostics);
            }
            Node::Directive(directive) => {
                resolve_directive(directive, defs, cache, visiting, diagnostics);
            }
            // Leaves with no `InlineNode`/`Node` content of their own: a
            // target's URI, a literal/doctest block's verbatim text, and a
            // comment/transition carry nothing this pass can resolve.
            Node::Target { .. }
            | Node::AnonymousTarget { .. }
            | Node::LiteralBlock { .. }
            | Node::DoctestBlock(_)
            | Node::Comment
            | Node::Transition => {}
        }
    }
}

fn resolve_line_block_items(
    items: &mut [rusty_sphinx_ast::LineBlockItem],
    defs: &Definitions,
    cache: &mut HashMap<String, Vec<InlineNode>>,
    visiting: &mut HashSet<String>,
    diagnostics: &mut Diagnostics,
) {
    for item in items {
        match item {
            rusty_sphinx_ast::LineBlockItem::Line(inlines) => {
                resolve_list(inlines, defs, cache, visiting, diagnostics);
            }
            rusty_sphinx_ast::LineBlockItem::Nested(nested) => {
                resolve_line_block_items(nested, defs, cache, visiting, diagnostics);
            }
        }
    }
}

/// Descends into the block-level/inline children a [`Directive`] carries.
/// Split out of [`resolve_nodes`] for the same reason
/// [`rusty_sphinx_ast::visit::walk_directive`] is: it is by far the most
/// branch-heavy arm.
fn resolve_directive(
    directive: &mut Directive,
    defs: &Definitions,
    cache: &mut HashMap<String, Vec<InlineNode>>,
    visiting: &mut HashSet<String>,
    diagnostics: &mut Diagnostics,
) {
    match directive {
        Directive::Admonition { body, .. }
        | Directive::VersionChange { body, .. }
        | Directive::SeeAlso { body } => resolve_nodes(body, defs, cache, visiting, diagnostics),
        // Every section of an entity is ordinary body content, so a `|name|`
        // written inside a `.. verification-criteria::` resolves like any other.
        // Attribute values are deliberately *not* touched: they are typed
        // values validated against the schema, not inline markup.
        Directive::Entity(entity) => {
            for section in &mut entity.sections {
                resolve_nodes(&mut section.body, defs, cache, visiting, diagnostics);
            }
        }
        Directive::EntitySection { body, .. } => {
            resolve_nodes(body, defs, cache, visiting, diagnostics);
        }
        Directive::Glossary { entries, .. } => {
            for entry in entries {
                resolve_nodes(&mut entry.definition, defs, cache, visiting, diagnostics);
            }
        }
        Directive::DataTable { rows, .. } => {
            for row in rows {
                for cell in &mut row.cells {
                    resolve_nodes(&mut cell.content, defs, cache, visiting, diagnostics);
                }
            }
        }
        Directive::Table {
            header_rows,
            body_rows,
            ..
        } => {
            for row in header_rows.iter_mut().chain(body_rows.iter_mut()) {
                for cell in &mut row.cells {
                    resolve_nodes(&mut cell.content, defs, cache, visiting, diagnostics);
                }
            }
        }
        Directive::DomainObject(body) => {
            resolve_nodes(body.body_mut(), defs, cache, visiting, diagnostics);
        }
        Directive::Figure(figure) => {
            if let Some(caption) = &mut figure.caption {
                resolve_list(caption, defs, cache, visiting, diagnostics);
            }
            resolve_nodes(&mut figure.legend, defs, cache, visiting, diagnostics);
        }
        // No `InlineNode`/`Node` content: an image's `:alt:` and a toctree
        // entry's title are plain strings, not inline-parsed, matching every
        // other caption in this codebase; the rest carry no text at all.
        Directive::Image(_)
        | Directive::DocTest(_)
        | Directive::CodeBlock(_)
        | Directive::Highlight { .. }
        | Directive::Math { .. }
        | Directive::Toctree { .. }
        | Directive::Contents { .. }
        | Directive::Sectnum(_)
        | Directive::PlantUml(_)
        | Directive::Index { .. }
        | Directive::PyCurrentModule { .. }
        | Directive::CNamespace { .. }
        | Directive::CNamespacePush { .. }
        | Directive::CNamespacePop
        | Directive::StdProgram { .. }
        | Directive::SubstitutionDefinition(_)
        | Directive::Unknown { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_sphinx_ast::{ImageOptions, ImageUri, SubstitutionDefinition};

    fn replace_def(name: &str, content: Vec<InlineNode>) -> Node {
        Node::Directive(Directive::SubstitutionDefinition(SubstitutionDefinition {
            name: name.to_string(),
            kind: SubstitutionKind::Replace(content),
            span: None,
        }))
    }

    fn unicode_def(name: &str, text: &str, trim: TrimSides) -> Node {
        Node::Directive(Directive::SubstitutionDefinition(SubstitutionDefinition {
            name: name.to_string(),
            kind: SubstitutionKind::Unicode {
                text: text.to_string(),
                trim,
            },
            span: None,
        }))
    }

    fn reference(name: &str) -> InlineNode {
        InlineNode::SubstitutionReference {
            name: name.to_string(),
            span: None,
        }
    }

    #[test]
    fn test_resolves_a_reference_written_before_its_definition() {
        // Given — the common real-world shape: the definition lives near the
        // bottom of the document, the reference is used throughout it.
        let mut nodes = vec![
            Node::Paragraph(vec![reference("release")]),
            replace_def("release", vec![InlineNode::Text("3.13.0".to_string())]),
        ];
        let mut diagnostics = Diagnostics::default();

        // When
        resolve_substitutions(&mut nodes, &mut diagnostics);

        // Then
        assert_eq!(
            nodes[0],
            Node::Paragraph(vec![InlineNode::Text("3.13.0".to_string())])
        );
        assert!(diagnostics.entries().is_empty());
    }

    #[test]
    fn test_resolves_a_reference_used_more_than_once() {
        // Given
        let mut nodes = vec![
            replace_def("x", vec![InlineNode::Text("value".to_string())]),
            Node::Paragraph(vec![reference("x")]),
            Node::Paragraph(vec![reference("x")]),
        ];
        let mut diagnostics = Diagnostics::default();

        // When
        resolve_substitutions(&mut nodes, &mut diagnostics);

        // Then
        assert_eq!(
            nodes[1],
            Node::Paragraph(vec![InlineNode::Text("value".to_string())])
        );
        assert_eq!(
            nodes[2],
            Node::Paragraph(vec![InlineNode::Text("value".to_string())])
        );
    }

    #[test]
    fn test_falls_back_case_insensitively() {
        // Given
        let mut nodes = vec![
            replace_def("ReST", vec![InlineNode::Text("reST".to_string())]),
            Node::Paragraph(vec![reference("rest")]),
        ];
        let mut diagnostics = Diagnostics::default();

        // When
        resolve_substitutions(&mut nodes, &mut diagnostics);

        // Then
        assert_eq!(
            nodes[1],
            Node::Paragraph(vec![InlineNode::Text("reST".to_string())])
        );
        assert!(diagnostics.entries().is_empty());
    }

    #[test]
    fn test_ambiguous_case_insensitive_match_is_left_undefined() {
        // Given — two definitions differing only in case: neither is picked
        // over the other, so a case-insensitive reference resolves to neither
        let mut nodes = vec![
            replace_def("Foo", vec![InlineNode::Text("upper".to_string())]),
            replace_def("foo", vec![InlineNode::Text("lower".to_string())]),
            Node::Paragraph(vec![reference("FOO")]),
        ];
        let mut diagnostics = Diagnostics::default();

        // When
        resolve_substitutions(&mut nodes, &mut diagnostics);

        // Then
        assert_eq!(
            nodes[2],
            Node::Paragraph(vec![InlineNode::Text("|FOO|".to_string())])
        );
        assert_eq!(
            diagnostics
                .entries()
                .iter()
                .map(|d| d.code)
                .collect::<Vec<_>>(),
            vec![DiagnosticCode::SubstitutionUndefined]
        );
    }

    #[test]
    fn test_reports_an_undefined_reference_and_keeps_the_written_text() {
        // Given
        let mut nodes = vec![Node::Paragraph(vec![reference("nope")])];
        let mut diagnostics = Diagnostics::default();

        // When
        resolve_substitutions(&mut nodes, &mut diagnostics);

        // Then
        assert_eq!(
            nodes[0],
            Node::Paragraph(vec![InlineNode::Text("|nope|".to_string())])
        );
        assert_eq!(
            diagnostics
                .entries()
                .iter()
                .map(|d| d.code)
                .collect::<Vec<_>>(),
            vec![DiagnosticCode::SubstitutionUndefined]
        );
    }

    #[test]
    fn test_reports_a_duplicate_definition_and_keeps_the_first() {
        // Given
        let mut nodes = vec![
            replace_def("x", vec![InlineNode::Text("first".to_string())]),
            replace_def("x", vec![InlineNode::Text("second".to_string())]),
            Node::Paragraph(vec![reference("x")]),
        ];
        let mut diagnostics = Diagnostics::default();

        // When
        resolve_substitutions(&mut nodes, &mut diagnostics);

        // Then
        assert_eq!(
            nodes[2],
            Node::Paragraph(vec![InlineNode::Text("first".to_string())])
        );
        assert_eq!(
            diagnostics
                .entries()
                .iter()
                .map(|d| d.code)
                .collect::<Vec<_>>(),
            vec![DiagnosticCode::SubstitutionDuplicateDefinition]
        );
    }

    #[test]
    fn test_reports_a_direct_circular_reference() {
        // Given — `a` refers to itself
        let mut nodes = vec![
            replace_def("a", vec![reference("a")]),
            Node::Paragraph(vec![reference("a")]),
        ];
        let mut diagnostics = Diagnostics::default();

        // When
        resolve_substitutions(&mut nodes, &mut diagnostics);

        // Then — degrades instead of overflowing the stack
        assert_eq!(
            diagnostics
                .entries()
                .iter()
                .map(|d| d.code)
                .collect::<Vec<_>>(),
            vec![DiagnosticCode::SubstitutionCircularReference]
        );
    }

    #[test]
    fn test_reports_an_indirect_circular_reference() {
        // Given — `a` refers to `b`, which refers back to `a`
        let mut nodes = vec![
            replace_def("a", vec![reference("b")]),
            replace_def("b", vec![reference("a")]),
            Node::Paragraph(vec![reference("a")]),
        ];
        let mut diagnostics = Diagnostics::default();

        // When
        resolve_substitutions(&mut nodes, &mut diagnostics);

        // Then
        assert_eq!(
            diagnostics
                .entries()
                .iter()
                .map(|d| d.code)
                .collect::<Vec<_>>(),
            vec![DiagnosticCode::SubstitutionCircularReference]
        );
    }

    #[test]
    fn test_resolves_a_chain_of_replace_substitutions() {
        // Given — `a` embeds `b`'s reference, `b` is plain text
        let mut nodes = vec![
            replace_def(
                "a",
                vec![InlineNode::Text("see ".to_string()), reference("b")],
            ),
            replace_def("b", vec![InlineNode::Text("here".to_string())]),
            Node::Paragraph(vec![reference("a")]),
        ];
        let mut diagnostics = Diagnostics::default();

        // When
        resolve_substitutions(&mut nodes, &mut diagnostics);

        // Then
        assert_eq!(
            nodes[2],
            Node::Paragraph(vec![
                InlineNode::Text("see ".to_string()),
                InlineNode::Text("here".to_string())
            ])
        );
        assert!(diagnostics.entries().is_empty());
    }

    #[test]
    fn test_resolves_an_image_substitution_reference_to_an_inline_image() {
        // Given
        let options = ImageOptions::new(ImageUri::new("logo.png"));
        let mut nodes = vec![
            Node::Directive(Directive::SubstitutionDefinition(SubstitutionDefinition {
                name: "logo".to_string(),
                kind: SubstitutionKind::Image(Box::new(options.clone())),
                span: None,
            })),
            Node::Paragraph(vec![reference("logo")]),
        ];
        let mut diagnostics = Diagnostics::default();

        // When
        resolve_substitutions(&mut nodes, &mut diagnostics);

        // Then
        assert_eq!(
            nodes[1],
            Node::Paragraph(vec![InlineNode::InlineImage(Box::new(options))])
        );
    }

    #[test]
    fn test_trim_strips_whitespace_on_both_sides_of_the_reference() {
        // Given
        let mut nodes = vec![
            unicode_def(
                "nbsp",
                "\u{a0}",
                TrimSides {
                    ltrim: true,
                    rtrim: true,
                },
            ),
            Node::Paragraph(vec![
                InlineNode::Text("left ".to_string()),
                reference("nbsp"),
                InlineNode::Text(" right".to_string()),
            ]),
        ];
        let mut diagnostics = Diagnostics::default();

        // When
        resolve_substitutions(&mut nodes, &mut diagnostics);

        // Then
        assert_eq!(
            nodes[1],
            Node::Paragraph(vec![
                InlineNode::Text("left".to_string()),
                InlineNode::Text("\u{a0}".to_string()),
                InlineNode::Text("right".to_string())
            ])
        );
    }

    #[test]
    fn test_ltrim_alone_leaves_the_trailing_side_untouched() {
        // Given
        let mut nodes = vec![
            unicode_def(
                "x",
                "X",
                TrimSides {
                    ltrim: true,
                    rtrim: false,
                },
            ),
            Node::Paragraph(vec![
                InlineNode::Text("left ".to_string()),
                reference("x"),
                InlineNode::Text(" right".to_string()),
            ]),
        ];
        let mut diagnostics = Diagnostics::default();

        // When
        resolve_substitutions(&mut nodes, &mut diagnostics);

        // Then
        assert_eq!(
            nodes[1],
            Node::Paragraph(vec![
                InlineNode::Text("left".to_string()),
                InlineNode::Text("X".to_string()),
                InlineNode::Text(" right".to_string())
            ])
        );
    }

    #[test]
    fn test_resolves_a_definition_nested_inside_an_admonition() {
        // Given — a definition need not be top-level
        let mut nodes = vec![Node::Directive(Directive::Admonition {
            kind: rusty_sphinx_ast::AdmonitionKind::Note,
            title: None,
            collapsible: None,
            body: vec![
                replace_def("x", vec![InlineNode::Text("value".to_string())]),
                Node::Paragraph(vec![reference("x")]),
            ],
        })];
        let mut diagnostics = Diagnostics::default();

        // When
        resolve_substitutions(&mut nodes, &mut diagnostics);

        // Then
        let Node::Directive(Directive::Admonition { body, .. }) = &nodes[0] else {
            panic!("expected an admonition");
        };
        assert_eq!(
            body[1],
            Node::Paragraph(vec![InlineNode::Text("value".to_string())])
        );
    }

    #[test]
    fn test_resolves_a_reference_inside_a_heading() {
        // Given
        let mut nodes = vec![
            replace_def("v", vec![InlineNode::Text("2.0".to_string())]),
            Node::Heading {
                level: 1,
                text: vec![InlineNode::Text("Version ".to_string()), reference("v")],
            },
        ];
        let mut diagnostics = Diagnostics::default();

        // When
        resolve_substitutions(&mut nodes, &mut diagnostics);

        // Then
        let Node::Heading { text, .. } = &nodes[1] else {
            panic!("expected a heading");
        };
        assert_eq!(
            text,
            &vec![
                InlineNode::Text("Version ".to_string()),
                InlineNode::Text("2.0".to_string())
            ]
        );
    }
}

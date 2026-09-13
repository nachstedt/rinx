//! Recognizing a directive at the head of a block and routing it to the
//! parser for its kind — the domain-object family via [`super::domains`],
//! everything else by directive name.

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::headings::Adornment;
use crate::indent::indent_width;

use super::admonitions::{parse_admonition, parse_seealso, parse_version_change};
use super::body::{collect_argument_continuation_lines, collect_directive_body};
use super::code_block::{parse_code_block, parse_highlight, parse_literal_include};
use super::contents::parse_contents;
use super::data_table::{parse_csv_table, parse_list_table};
use super::doctest::{DocTestDirectiveKind, parse_doctest_directive};
use super::domains::object_type::{DirectiveObjectType, resolve_domain_object_type};
use super::domains::{DirectiveSignatures, parse_domain_object};
use super::dropdown::parse_dropdown;
use super::entity::{EntityDirective, parse_entity};
use super::entity_flow::parse_entity_flow;
use super::entity_pie::parse_entity_pie;
use super::entity_section::{EntitySectionSite, try_parse_entity_section};
use super::entity_table::parse_entity_table;
use super::error_node::unknown_directive;
use super::glossary::parse_glossary;
use super::grid::{parse_grid, parse_grid_item};
use super::if_builder::parse_if_builder;
use super::image::{parse_figure_directive, parse_image_directive};
use super::include::parse_include;
use super::index_directive::parse_index_directive;
use super::math::parse_math_directive;
use super::needimport::parse_needimport;
use super::scope::try_parse_scope_directive;
use super::sectnum::{is_sectnum, parse_sectnum};
use super::substitution::{parse_substitution_definition, split_substitution_marker};
use super::table::parse_table_directive;
use super::toctree::parse_toctree;
use super::uml::parse_uml;
use rusty_sphinx_ast::{CodeBlockSource, Directive, Node};

/// The indentation every directive-body parser strips before parsing, so a
/// `ParseCtx` can be shifted by the same amount.
///
/// Mirrors [`crate::indent::unindent_body_lines`]'s rule exactly — the first
/// non-blank line's indent — because that is the function whose effect this
/// compensates for. Without it every position inside a directive body would
/// be short by the body's indent.
fn body_indent(body_lines: &[&str]) -> usize {
    body_lines
        .iter()
        .find(|line| !line.trim().is_empty())
        .map_or(0, |line| indent_width(line))
}

pub(crate) fn try_parse_directive(
    lines: &[&str],
    i: usize,
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<(usize, Vec<Node>)> {
    let line = lines[i].trim_end();
    if !(line.trim().starts_with(".. ") && line.contains("::")) {
        return None;
    }

    let trimmed = line.trim();
    let (name_part, arg_part) = trimmed.split_once("::")?;
    let name = name_part.strip_prefix(".. ")?.trim().to_string();
    let argument = arg_part.trim().to_string();

    let min_indent = indent_width(line);

    // Checked before the generic body collection below, because a domain
    // object splits the lines after its marker differently: any further
    // argument lines are peeled off as extra signatures first, and only what
    // remains is its body. No other directive name can reach this branch —
    // `resolve_domain_object_type` matches a disjoint set of names from the
    // ones handled afterwards.
    if let Some(object_type) = resolve_domain_object_type(&name, ctx.default_domain) {
        let (continuations_consumed, continuations) =
            if object_type_supports_multiple_signatures(object_type) {
                collect_argument_continuation_lines(lines, i + 1, min_indent)
            } else {
                (0, Vec::new())
            };
        let body = collect_directive_body(lines, i + 1 + continuations_consumed, min_indent);
        let body_ctx = ctx.nested(
            i + 1 + continuations_consumed + body.first_line_offset,
            body_indent(&body.lines),
        );
        let domain_object = parse_domain_object(
            object_type,
            DirectiveSignatures {
                argument,
                continuations,
                span: ctx.line_span(i, line),
            },
            &body.lines,
            adornment_order,
            diagnostics,
            &body_ctx,
        );
        return Some((
            1 + continuations_consumed + body.consumed,
            vec![Node::Directive(Directive::DomainObject(domain_object))],
        ));
    }

    // Checked here rather than in the `if name ==` chain below because an
    // entity type is a *vocabulary* lookup, like a domain object's, not a
    // fixed name — and because the entity parser needs the directive's own
    // line to discriminate a generated id.
    if let Some(result) = try_parse_entity_directive(
        &name,
        &argument,
        &DirectiveSite {
            lines,
            index: i,
            line,
            min_indent,
        },
        adornment_order,
        diagnostics,
        ctx,
    ) {
        return Some(result);
    }

    let body = collect_directive_body(lines, i + 1, min_indent);
    // Every directive parser below receives a context already positioned at
    // its own body's first line *and* column, so none of them has to know
    // where in the document the directive was written. The column shift
    // matters because the parsers unindent the body before parsing it.
    //
    // The directive's *own* line is therefore no longer reachable from that
    // context, so it is captured here and passed alongside: a directive whose
    // content can still fail after parsing (an image whose bytes never reach
    // the renderer) needs a position to report against, and a bare
    // `.. image:: logo.png` has no body line to borrow one from.
    let directive_span = ctx.line_span(i, line);
    let body_ctx = ctx.nested(i + 1 + body.first_line_offset, body_indent(&body.lines));
    let (consumed_lines, body_lines) = (body.consumed, body.lines);

    // Checked before the one-node chain below, because these are the
    // directives that answer with any number of nodes rather than exactly one.
    if let Some(nodes) = try_parse_splicing_directive(
        &name,
        &argument,
        directive_span,
        &body_lines,
        adornment_order,
        diagnostics,
        &body_ctx,
    ) {
        return Some((1 + consumed_lines, nodes));
    }

    let node = parse_body_directive(
        name,
        argument,
        directive_span,
        &body_lines,
        adornment_order,
        diagnostics,
        &body_ctx,
    );
    Some((1 + consumed_lines, vec![node]))
}

/// Dispatches the directives that contribute their nodes to the *enclosing*
/// block instead of wrapping them in one of their own.
///
/// All three are transclusions rather than containers, and that is the
/// property they share: a section heading, hyperlink target, index entry or
/// `.. toctree::` written inside one belongs to this document exactly as if it
/// had been typed here. Wrapping the result in a node would break every one of
/// those — section nesting first — so none of them can live in
/// [`try_parse_extension_directive`] or the chain below, which answer with a
/// single [`Directive`].
///
/// `.. needimport::` is the one whose nodes are not reStructuredText the
/// author wrote: it builds entities out of a `needs.json`. It belongs here
/// all the same, and for the same reason — an imported entity must be an
/// ordinary entity of this document, not something nested inside a directive
/// that every later traversal would have to learn about.
///
/// Returns `None` for a name that is none of them, leaving it to that chain.
fn try_parse_splicing_directive(
    name: &str,
    argument: &str,
    directive_span: Option<rusty_sphinx_ast::Span>,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<Vec<Node>> {
    match name {
        // Splices another file's reStructuredText in where it stands.
        "include" => Some(parse_include(
            argument,
            body_lines,
            adornment_order,
            diagnostics,
            ctx,
        )),
        // sphinx-needs': splices the entities it reads out of a needs.json,
        // rather than its own body — the only one of the three whose nodes
        // come from somewhere other than reStructuredText.
        "needimport" => Some(parse_needimport(
            argument,
            directive_span,
            body_lines,
            adornment_order,
            diagnostics,
            ctx,
        )),
        // sphinx-simplepdf's: splices its own body, but only for the builder
        // this build is.
        "if-builder" => Some(parse_if_builder(
            argument,
            directive_span,
            body_lines,
            adornment_order,
            diagnostics,
            ctx,
        )),
        _ => None,
    }
}

/// Dispatches every directive whose body is collected the ordinary way — that
/// is, all of them except domain objects, which peel extra signature lines off
/// the argument before their body starts and so are handled by the caller.
///
/// Falls back to [`Directive::Unknown`] — reported as `directive.unknown` and
/// drawn as a visible error block, and what the benchmark counts as an
/// unsupported directive.
/// Recognizes and parses a substitution definition's marker, `.. |name|
/// inner::`, which is not a directive name at all — tried before every other
/// name below, which would otherwise match nothing and fall all the way to
/// [`Directive::Unknown`].
fn try_parse_substitution_definition(
    name: &str,
    argument: &str,
    directive_span: Option<rusty_sphinx_ast::Span>,
    body_lines: &[&str],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<Node> {
    let (sub_name, inner) = split_substitution_marker(name)?;
    let directive = parse_substitution_definition(
        sub_name,
        inner,
        argument,
        directive_span,
        body_lines,
        diagnostics,
        ctx,
    )?;
    Some(Node::Directive(directive))
}

/// Recognizes and parses the two directives that each build a table of
/// contents from a document set: project-wide `.. toctree::` and this
/// document's own `.. contents::`. Grouped for the same reason
/// [`try_parse_substitution_definition`] is split out — one more `if name ==`
/// pair would have pushed [`parse_body_directive`] past its line limit.
fn try_parse_contents_family(
    name: &str,
    argument: &str,
    body_lines: &[&str],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<Node> {
    let directive = match name {
        "toctree" => parse_toctree(body_lines, diagnostics, ctx),
        "contents" => parse_contents(argument, body_lines, diagnostics, ctx),
        _ => return None,
    };
    Some(Node::Directive(directive))
}

/// Recognizes and parses `.. sectnum::`/`.. section-numbering::`. Split out
/// for the same reason [`try_parse_contents_family`] is: one more `if` in
/// [`parse_body_directive`] would have pushed it past its line limit.
fn try_parse_sectnum(
    name: &str,
    argument: &str,
    body_lines: &[&str],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<Node> {
    if !is_sectnum(name) {
        return None;
    }
    Some(Node::Directive(parse_sectnum(
        argument,
        body_lines,
        diagnostics,
        ctx,
    )))
}

/// Where in the source a directive marker sits, and the lines it may claim.
///
/// One value rather than four parameters, because every one of them is read
/// only to answer the same question — how far this directive's body reaches
/// and where its diagnostics point.
struct DirectiveSite<'a> {
    /// The whole line slice being parsed.
    lines: &'a [&'a str],
    /// The marker's index within it.
    index: usize,
    /// The marker line itself.
    line: &'a str,
    /// The indent the body must exceed.
    min_indent: usize,
}

/// Parses `.. <type>::` when the schema declares an entity type of that name.
///
/// Split out of [`try_parse_directive`] purely to keep that function's line
/// count readable, the same reason `try_parse_contents_family` exists.
fn try_parse_entity_directive(
    name: &str,
    argument: &str,
    site: &DirectiveSite<'_>,
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<(usize, Vec<Node>)> {
    let entity_type = ctx.schema.entity_type(name)?;
    let &DirectiveSite {
        lines,
        index: i,
        line,
        min_indent,
    } = site;
    let body = collect_directive_body(lines, i + 1, min_indent);
    let body_ctx = ctx.nested(i + 1 + body.first_line_offset, body_indent(&body.lines));
    let discriminator = ctx.position(i, 0).map_or(0, |point| point.position.line);
    let directive = parse_entity(
        &EntityDirective {
            entity_type,
            argument,
            span: ctx.line_span(i, line),
            body_lines: &body.lines,
            discriminator,
            doc_path: ctx.doc_path,
        },
        adornment_order,
        diagnostics,
        &body_ctx,
    );
    Some((1 + body.consumed, vec![Node::Directive(directive)]))
}

fn parse_body_directive(
    name: String,
    argument: String,
    directive_span: Option<rusty_sphinx_ast::Span>,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Node {
    if let Some(node) = try_parse_substitution_definition(
        &name,
        &argument,
        directive_span,
        body_lines,
        diagnostics,
        ctx,
    ) {
        return node;
    }
    if let Some(node) = try_parse_entity_section(
        &name,
        &argument,
        &EntitySectionSite {
            span: directive_span,
            body_lines,
        },
        adornment_order,
        diagnostics,
        ctx,
    ) {
        return node;
    }
    if let Some(node) = try_parse_contents_family(&name, &argument, body_lines, diagnostics, ctx) {
        return node;
    }
    parse_remaining_body_directive(
        name,
        argument,
        directive_span,
        body_lines,
        adornment_order,
        diagnostics,
        ctx,
    )
}

/// The rest of the directive chain, from `.. sectnum::` onwards.
///
/// Split from [`parse_body_directive`] only to keep both under the line limit
/// as the chain grows; the order across the two is the order a name is tried
/// in, and nothing else distinguishes them.
fn parse_remaining_body_directive(
    name: String,
    argument: String,
    directive_span: Option<rusty_sphinx_ast::Span>,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Node {
    if let Some(node) = try_parse_sectnum(&name, &argument, body_lines, diagnostics, ctx) {
        return node;
    }
    if let Some(directive) = parse_code_family(&name, &argument, body_lines, diagnostics, ctx) {
        return Node::Directive(directive);
    }
    if let Ok(kind) = name.parse::<rusty_sphinx_ast::VersionChangeKind>() {
        let directive = parse_version_change(
            kind,
            argument,
            body_lines,
            adornment_order,
            diagnostics,
            ctx,
        );
        return Node::Directive(directive);
    }
    if let Some(directive) = try_parse_extension_directive(
        &name,
        &argument,
        directive_span,
        body_lines,
        adornment_order,
        diagnostics,
        ctx,
    ) {
        return Node::Directive(directive);
    }
    if name == "seealso" {
        let directive = parse_seealso(body_lines, adornment_order, diagnostics, ctx);
        return Node::Directive(directive);
    }
    if let Ok(kind) = name.parse::<rusty_sphinx_ast::AdmonitionKind>() {
        let directive = parse_admonition(
            kind,
            argument,
            body_lines,
            adornment_order,
            diagnostics,
            ctx,
        );
        return Node::Directive(directive);
    }
    if name == "glossary" {
        let directive = parse_glossary(body_lines, adornment_order, diagnostics, ctx);
        return Node::Directive(directive);
    }
    if name == "list-table" {
        let directive = parse_list_table(&argument, body_lines, adornment_order, diagnostics, ctx);
        return Node::Directive(directive);
    }
    if name == "csv-table" {
        let directive = parse_csv_table(&argument, body_lines, adornment_order, diagnostics, ctx);
        return Node::Directive(directive);
    }
    if let Some(directive) = parse_image_family(
        &name,
        &argument,
        directive_span,
        body_lines,
        adornment_order,
        diagnostics,
        ctx,
    ) {
        return Node::Directive(directive);
    }
    if name == "math" {
        let directive = parse_math_directive(argument, body_lines, diagnostics, ctx);
        return Node::Directive(directive);
    }
    if name == "table" {
        let directive =
            parse_table_directive(&argument, body_lines, adornment_order, diagnostics, ctx);
        return Node::Directive(directive);
    }
    if name == "index" {
        let directive = parse_index_directive(&argument, body_lines, diagnostics, ctx);
        return Node::Directive(directive);
    }
    if let Some(kind) = DocTestDirectiveKind::from_name(&name) {
        let directive = parse_doctest_directive(kind, &argument, body_lines, diagnostics, ctx);
        return Node::Directive(directive);
    }
    if let Some(directive) = try_parse_scope_directive(
        &name,
        &argument,
        directive_span,
        body_lines,
        diagnostics,
        ctx,
    ) {
        return Node::Directive(directive);
    }
    let directive = unknown_directive(name, argument, body_lines, directive_span, diagnostics);
    Node::Directive(directive)
}

/// Parses the directives that come from a *Sphinx extension* rather than from
/// docutils or Sphinx itself.
///
/// Grouped because they share a rule the built-ins do not: this build has no
/// `extensions =` config, so a supported extension directive is simply always
/// available and its name is reserved against entity schemas. See
/// `spec_gaps.md`'s "Third-party extension directives" section, which lists
/// exactly these.
fn try_parse_extension_directive(
    name: &str,
    argument: &str,
    directive_span: Option<rusty_sphinx_ast::Span>,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<Directive> {
    // sphinx-design's collapsible container.
    if name == "dropdown" {
        return Some(parse_dropdown(
            argument,
            directive_span,
            body_lines,
            adornment_order,
            diagnostics,
            ctx,
        ));
    }
    // sphinx-design's responsive row, and the cell that goes in it. Two
    // directives, one construct — a grid-item written anywhere else is still
    // parsed, so its content survives to be warned about rather than lost.
    if name == "grid" {
        return Some(parse_grid(
            argument,
            directive_span,
            body_lines,
            adornment_order,
            diagnostics,
            ctx,
        ));
    }
    if name == "grid-item" {
        return Some(parse_grid_item(
            directive_span,
            body_lines,
            adornment_order,
            diagnostics,
            ctx,
        ));
    }
    // This build's own listing directive, and sphinx-needs' spelling of it.
    if let Ok(source) = name.parse::<rusty_sphinx_ast::EntityTableSource>() {
        return Some(parse_entity_table(
            source,
            argument,
            directive_span,
            body_lines,
            diagnostics,
            ctx,
        ));
    }
    // This build's own flowchart, and sphinx-needs' spelling of it. A sibling
    // of the listing directive above rather than of the diagram below: it
    // carries a question, not a template, and its picture is generated.
    if let Ok(source) = name.parse::<rusty_sphinx_ast::EntityFlowSource>() {
        return Some(parse_entity_flow(
            source,
            argument,
            directive_span,
            body_lines,
            diagnostics,
            ctx,
        ));
    }
    // This build's own pie chart, and sphinx-needs' spelling of it. The third
    // presentation of the listing directive's question — rows, a graph, or
    // proportions — and the only picture here that is never compiled: its SVG
    // is drawn by the render action itself.
    if let Ok(source) = name.parse::<rusty_sphinx_ast::EntityPieSource>() {
        return Some(parse_entity_pie(
            source,
            argument,
            directive_span,
            body_lines,
            diagnostics,
            ctx,
        ));
    }
    // sphinxcontrib-plantuml's two names, this build's two diagram names and
    // sphinx-needs' two — one node, one parser. The plain PlantUML pair is
    // nobody's extension in the sense the others are, but it belongs to the
    // same construct, and splitting the six across two dispatch sites is
    // exactly how the compiled set and the rendered set drift apart.
    if let Ok(source) = name.parse::<rusty_sphinx_ast::UmlSource>() {
        return Some(parse_uml(
            source,
            directive_span,
            body_lines,
            diagnostics,
            ctx,
        ));
    }
    None
}

/// Whether a directive of this object type may declare more than one
/// signature, as several argument lines below its marker.
///
/// Every object type may except `py:module`: real Sphinx's `module`
/// directive takes exactly one argument, so a line below it is body content
/// even when it looks like a further name.
const fn object_type_supports_multiple_signatures(object_type: DirectiveObjectType) -> bool {
    !matches!(object_type, DirectiveObjectType::PyModule)
}

/// Parses `.. image::` or `.. figure::`, or `None` when `name` is neither.
///
/// Grouped the same way [`parse_code_family`] groups its four, purely to keep
/// [`parse_body_directive`]'s own dispatch chain from growing past a readable
/// length as directive families accumulate.
fn parse_image_family(
    name: &str,
    argument: &str,
    directive_span: Option<rusty_sphinx_ast::Span>,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<Directive> {
    match name {
        "image" => Some(parse_image_directive(
            argument,
            directive_span,
            body_lines,
            diagnostics,
            ctx,
        )),
        "figure" => Some(parse_figure_directive(
            argument,
            directive_span,
            body_lines,
            adornment_order,
            diagnostics,
            ctx,
        )),
        _ => None,
    }
}

/// Parses the four directives [`super::code_block`] owns, or `None` when
/// `name` is not one of them.
///
/// Grouped into one branch of the dispatcher because they share a module and a
/// vocabulary, and because three of them produce the very same
/// [`Directive::CodeBlock`]: `.. code-block::`, docutils' `.. code::`, and
/// `.. literalinclude::`, which differ only in how their source spells the
/// block. `.. highlight::` rides along as the directive the first three
/// inherit their language from.
fn parse_code_family(
    name: &str,
    argument: &str,
    body_lines: &[&str],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<Directive> {
    if let Some(source) = code_block_source(name) {
        return Some(parse_code_block(
            source,
            argument,
            body_lines,
            diagnostics,
            ctx,
        ));
    }
    match name {
        "literalinclude" => Some(parse_literal_include(
            argument,
            body_lines,
            diagnostics,
            ctx,
        )),
        "highlight" => Some(parse_highlight(argument, body_lines, diagnostics, ctx)),
        _ => None,
    }
}

/// Which of the two inline code-block directives `name` spells, if either.
///
/// `.. code::` is docutils' name for the same construct; both lower to one
/// [`Directive::CodeBlock`] carrying a [`CodeBlockSource`].
fn code_block_source(name: &str) -> Option<CodeBlockSource> {
    match name {
        "code-block" => Some(CodeBlockSource::CodeBlock),
        "code" => Some(CodeBlockSource::Code),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use rusty_sphinx_ast::CodeLanguage;
    use rusty_sphinx_ast::Domain;

    /// A plain toctree document entry spanning the whole of source line
    /// `line`, which is the shape `parse_toctree` produces for an unindented
    /// entry written under a top-level directive.
    fn toc_document_entry(docname: &str, line: u32) -> rusty_sphinx_ast::TocEntry {
        let column_end = u32::try_from(docname.chars().count()).unwrap_or(0) + 4;
        rusty_sphinx_ast::TocEntry::Document {
            title: None,
            docname: docname.to_string(),
            span: Some(rusty_sphinx_ast::Span::new(
                rusty_sphinx_ast::Position::new(line, 4),
                rusty_sphinx_ast::Position::new(line, column_end),
            )),
        }
    }

    /// Dispatches `name`/`argument`/`body` through [`parse_body_directive`]
    /// with throwaway state, returning the node and any diagnostics.
    fn dispatch(name: &str, argument: &str, body: &[&str]) -> (Node, Diagnostics) {
        let mut adornment_order = Vec::new();
        let mut diagnostics = Diagnostics::default();
        let node = parse_body_directive(
            name.to_string(),
            argument.to_string(),
            None,
            body,
            &mut adornment_order,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );
        (node, diagnostics)
    }

    #[test]
    fn test_parse_body_directive_dispatches_a_known_directive() {
        // Given
        let body = ["   Careful."];

        // When
        let (node, _) = dispatch("note", "", &body);

        // Then
        assert!(matches!(
            node,
            Node::Directive(Directive::Admonition { .. })
        ));
    }

    #[test]
    fn test_parse_body_directive_dispatches_the_doctest_family() {
        // Given
        let body = ["   >>> 1"];

        // When
        let (node, _) = dispatch("doctest", "", &body);

        // Then
        assert!(matches!(node, Node::Directive(Directive::DocTest(_))));
    }

    #[test]
    fn test_parse_body_directive_routes_both_code_block_spellings() {
        // Given — Sphinx's name and docutils' name for one construct
        let body = ["   print(1)"];

        // When
        let (sphinx_spelling, _) = dispatch("code-block", "python", &body);
        let (docutils_spelling, _) = dispatch("code", "python", &body);

        // Then — both reach the same variant, tagged with who wrote them
        let Node::Directive(Directive::CodeBlock(sphinx)) = sphinx_spelling else {
            panic!("Expected CodeBlock, got {sphinx_spelling:?}");
        };
        let Node::Directive(Directive::CodeBlock(docutils)) = docutils_spelling else {
            panic!("Expected CodeBlock, got {docutils_spelling:?}");
        };
        assert_eq!(sphinx.source, CodeBlockSource::CodeBlock);
        assert_eq!(docutils.source, CodeBlockSource::Code);
    }

    #[test]
    fn test_parse_body_directive_falls_back_to_unknown() {
        // Given — this is what the benchmark counts as unsupported.
        let body = ["   content"];

        // When
        let (node, diagnostics) = dispatch("not-a-real-directive", "arg", &body);

        // Then — the name and body are kept for the visible error block the
        // renderer draws, and the fall-through is reported rather than silent
        match node {
            Node::Directive(Directive::Unknown {
                name,
                argument,
                body,
            }) => {
                assert_eq!(name, "not-a-real-directive");
                assert_eq!(argument, "arg");
                assert_eq!(body, "content");
            }
            other => panic!("expected an unknown directive, got {other:?}"),
        }
        let (found, _, _) = diagnostics.into_parts();
        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0].code,
            rusty_sphinx_ast::DiagnosticCode::DirectiveUnknown
        );
        assert_eq!(
            found[0].message,
            "unknown directive type 'not-a-real-directive'"
        );
    }

    #[test]
    fn test_parse_body_directive_forwards_diagnostics() {
        // Given
        let body = ["   :bogus:", "", "   >>> 1"];

        // When
        let (_, diagnostics) = dispatch("doctest", "", &body);

        // Then
        assert!(!diagnostics.is_empty());
    }

    #[test]
    fn test_parse_creates_admonition() {
        // Given
        let input = ".. note::\n\n   This is a note.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::Admonition {
            kind, title, body, ..
        }) = &doc.nodes[0]
        {
            assert_eq!(kind, &rusty_sphinx_ast::AdmonitionKind::Note);
            assert_eq!(title, &None);
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected Admonition directive");
        }
    }

    #[test]
    fn test_parse_creates_admonition_with_title() {
        // Given
        let input = ".. admonition:: My Title\n\n   Custom content.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::Admonition { kind, title, .. }) = &doc.nodes[0] {
            assert_eq!(kind, &rusty_sphinx_ast::AdmonitionKind::Admonition);
            assert_eq!(title, &Some("My Title".to_string()));
        } else {
            panic!("Expected Admonition directive");
        }
    }

    #[test]
    fn test_parse_creates_collapsible_admonition() {
        // Given
        let input = ".. note::\n   :collapsible:\n\n   Content.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::Admonition { collapsible, .. }) = &doc.nodes[0] {
            assert_eq!(collapsible, &Some(false));
        } else {
            panic!("Expected Admonition directive");
        }
    }

    #[test]
    fn test_parse_creates_collapsible_open_admonition() {
        // Given
        let input = ".. note::\n   :collapsible: open\n\n   Content.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::Admonition { collapsible, .. }) = &doc.nodes[0] {
            assert_eq!(collapsible, &Some(true));
        } else {
            panic!("Expected Admonition directive");
        }
    }

    #[test]
    fn test_parse_creates_directive() {
        // Given
        let input = ".. toctree::\n   \n   team_a/index\n   team_b/index\n\nNext Para";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 2);
        assert_eq!(
            doc.nodes[0],
            Node::Directive(Directive::Toctree(rusty_sphinx_ast::Toctree {
                entries: vec![
                    toc_document_entry("team_a/index", 3),
                    toc_document_entry("team_b/index", 4),
                ],
                options: rusty_sphinx_ast::ToctreeOptions::default(),
            }))
        );
        assert_eq!(
            doc.nodes[1],
            Node::Paragraph(vec![rusty_sphinx_ast::InlineNode::Text(
                "Next Para".to_string()
            )])
        );
    }

    #[test]
    fn test_parse_creates_directive_with_maxdepth() {
        // Given
        let input =
            ".. toctree::\n   :maxdepth: 2\n   \n   team_a/index\n   team_b/index\n\nNext Para";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 2);
        assert_eq!(
            doc.nodes[0],
            Node::Directive(Directive::Toctree(rusty_sphinx_ast::Toctree {
                entries: vec![
                    toc_document_entry("team_a/index", 4),
                    toc_document_entry("team_b/index", 5),
                ],
                options: rusty_sphinx_ast::ToctreeOptions {
                    maxdepth: std::num::NonZeroUsize::new(2),
                    ..rusty_sphinx_ast::ToctreeOptions::default()
                },
            }))
        );
        assert_eq!(
            doc.nodes[1],
            Node::Paragraph(vec![rusty_sphinx_ast::InlineNode::Text(
                "Next Para".to_string()
            )])
        );
    }

    #[test]
    fn test_parse_directive_with_argument_and_trailing_indents() {
        // Given
        let input = ".. code-block:: rust\n\n   let x = 1;\n   \n   let y = 2;\n\n";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        let Node::Directive(Directive::CodeBlock(block)) = &doc.nodes[0] else {
            panic!("Expected CodeBlock, got {:?}", doc.nodes[0]);
        };
        assert_eq!(block.language, CodeLanguage::parse("rust"));
        assert_eq!(block.content, "let x = 1;\n\nlet y = 2;");
    }

    #[test]
    fn test_parse_creates_index_directive() {
        // Given
        let input = ".. index:: single: execution\n\nNext Para";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 2);
        if let Node::Directive(Directive::Index { entries, .. }) = &doc.nodes[0] {
            assert_eq!(entries.len(), 1);
        } else {
            panic!("Expected Index directive, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_creates_plantuml_directive_with_hash() {
        // Given
        let input = ".. plantuml::\n\n   A -> B\n   B -> C\n\nNext Para";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 2);

        let Node::Directive(Directive::Uml(uml)) = &doc.nodes[0] else {
            panic!("expected a diagram, found {:?}", doc.nodes[0]);
        };
        assert_eq!(uml.source, rusty_sphinx_ast::UmlSource::PlantUml);
        assert_eq!(uml.template, "A -> B\nB -> C");
        assert_eq!(
            doc.nodes[1],
            Node::Paragraph(vec![rusty_sphinx_ast::InlineNode::Text(
                "Next Para".to_string()
            )])
        );
    }

    #[test]
    fn test_parse_code_block_directive_with_language() {
        // Given
        let input = ".. code-block:: python

    x = 1
";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        let Node::Directive(Directive::CodeBlock(block)) = &doc.nodes[0] else {
            panic!("Expected CodeBlock, got {:?}", doc.nodes[0]);
        };
        assert_eq!(block.language, CodeLanguage::parse("python"));
        assert_eq!(block.content, "x = 1");
    }

    #[test]
    fn test_parse_code_block_directive_without_language() {
        // Given
        let input = ".. code-block::

    x = 1
";

        // When
        let doc = parse("test.rst", input);

        // Then — an argumentless block inherits rather than naming nothing
        assert_eq!(doc.nodes.len(), 1);
        let Node::Directive(Directive::CodeBlock(block)) = &doc.nodes[0] else {
            panic!("Expected CodeBlock, got {:?}", doc.nodes[0]);
        };
        assert_eq!(block.language, CodeLanguage::Inherit);
        assert_eq!(block.content, "x = 1");
    }
}

/// Every directive name this build already dispatches.
///
/// Kept beside the chain above deliberately: the two are edited together, and
/// a name added there without being added here would let a project declare an
/// entity section that silently shadows it. `is_builtin_directive_name` is the
/// only consumer — the entity schema loader asks it through the worker, rather
/// than `rusty_sphinx_entity` keeping a copy that could drift.
///
/// Domain-object and scope directive names are included: they are resolved by
/// [`resolve_domain_object_type`] and [`try_parse_scope_directive`] rather than
/// by the chain, but they are just as much names a section must not take.
const BUILTIN_DIRECTIVE_NAMES: &[&str] = &[
    // Admonitions
    "attention",
    "caution",
    "danger",
    "error",
    "hint",
    "important",
    "note",
    "tip",
    "warning",
    "admonition",
    "seealso",
    // Version changes
    "versionadded",
    "versionchanged",
    "deprecated",
    // Doctests
    "doctest",
    "testcode",
    "testoutput",
    "testsetup",
    "testcleanup",
    // Code
    "code-block",
    "code",
    "literalinclude",
    "highlight",
    // Images
    "image",
    "figure",
    // Tables
    "list-table",
    "csv-table",
    "table",
    // Navigation and structure
    "toctree",
    "contents",
    "sectnum",
    "section-numbering",
    "include",
    "glossary",
    "index",
    // Other content
    "dropdown",
    "grid",
    "grid-item",
    // sphinx-simplepdf's conditional-on-the-builder content
    "if-builder",
    // Listing directives over the entity graph, in both spellings
    "entity-table",
    "needtable",
    // This build's own flowchart over the entity graph, and sphinx-needs'
    // spelling of it.
    "entity-flow",
    "needflow",
    // This build's own pie chart over the entity graph, and sphinx-needs'
    // spelling of it.
    "entity-pie",
    "needpie",
    // sphinx-needs' import, which keeps its own name alone — see
    // `docs/decisions/016-needimport.md` for why `entity-import` is not
    // claimed beside it.
    "needimport",
    // Diagram directives: sphinxcontrib-plantuml's two names, this build's
    // two and sphinx-needs' two
    "plantuml",
    "uml",
    "entity-diagram",
    "needuml",
    "entity-arch",
    "needarch",
    "math",
    // Domain objects and scope directives
    "function",
    "decorator",
    "module",
    "data",
    "method",
    "classmethod",
    "staticmethod",
    "decoratormethod",
    "class",
    "attribute",
    "exception",
    "macro",
    "struct",
    "union",
    "member",
    "var",
    "type",
    "option",
    "cmdoption",
    "currentmodule",
    "program",
    "namespace",
    "namespace-push",
    "namespace-pop",
];

/// Reports whether `name` is a directive this build already understands.
///
/// Used by the entity schema loader to refuse a section that would shadow one.
/// Matches the bare name only: a domain-qualified spelling like `py:class`
/// cannot collide with a section name, which never carries a colon.
#[must_use]
pub fn is_builtin_directive_name(name: &str) -> bool {
    BUILTIN_DIRECTIVE_NAMES.contains(&name)
}

#[cfg(test)]
mod builtin_name_tests {
    use super::*;

    /// The names derived from an enum elsewhere in the workspace, which is
    /// where silent drift is likeliest: adding an `AdmonitionKind` variant is a
    /// one-line change that would otherwise leave this list stale and let a
    /// project declare a section shadowing the new directive.
    ///
    /// The hand-written remainder of `BUILTIN_DIRECTIVE_NAMES` is maintained
    /// with the dispatch chain above, which is why the two sit in one file.
    #[test]
    fn test_every_admonition_kind_is_reserved() {
        // Given — one per `AdmonitionKind`, via its own `FromStr`
        let names = [
            "attention",
            "caution",
            "danger",
            "error",
            "hint",
            "important",
            "note",
            "tip",
            "warning",
            "admonition",
        ];

        // When / Then
        for name in names {
            assert!(
                name.parse::<rusty_sphinx_ast::AdmonitionKind>().is_ok(),
                "`{name}` is no longer an admonition; this list is stale"
            );
            assert!(
                is_builtin_directive_name(name),
                "admonition `{name}` is not reserved"
            );
        }
    }

    #[test]
    fn test_every_version_change_kind_is_reserved() {
        // Given
        let names = ["versionadded", "versionchanged", "deprecated"];

        // When / Then
        for name in names {
            assert!(
                name.parse::<rusty_sphinx_ast::VersionChangeKind>().is_ok(),
                "`{name}` is no longer a version change; this list is stale"
            );
            assert!(
                is_builtin_directive_name(name),
                "version change `{name}` is not reserved"
            );
        }
    }

    #[test]
    fn test_every_doctest_directive_is_reserved() {
        // Given
        let names = [
            "doctest",
            "testcode",
            "testoutput",
            "testsetup",
            "testcleanup",
        ];

        // When / Then
        for name in names {
            assert!(
                DocTestDirectiveKind::from_name(name).is_some(),
                "`{name}` is no longer a doctest directive; this list is stale"
            );
            assert!(
                is_builtin_directive_name(name),
                "doctest directive `{name}` is not reserved"
            );
        }
    }

    #[test]
    fn test_every_domain_object_directive_name_is_reserved() {
        // Given — both domains, since a bare name resolves through either
        let domains = [rusty_sphinx_ast::Domain::Py, rusty_sphinx_ast::Domain::C];

        // When / Then
        for domain in domains {
            for name in [
                "function",
                "module",
                "data",
                "method",
                "class",
                "attribute",
                "exception",
                "macro",
                "struct",
                "union",
                "member",
                "type",
                "option",
            ] {
                if resolve_domain_object_type(name, domain).is_some() {
                    assert!(
                        is_builtin_directive_name(name),
                        "domain object directive `{name}` is not reserved"
                    );
                }
            }
        }
    }

    #[test]
    fn test_the_reserved_list_holds_no_duplicates() {
        // Given
        let mut seen: Vec<&str> = Vec::new();

        // When / Then
        for name in BUILTIN_DIRECTIVE_NAMES {
            assert!(!seen.contains(name), "`{name}` is listed twice");
            seen.push(name);
        }
    }

    #[test]
    fn test_a_name_no_directive_uses_is_not_reserved() {
        // Given / When / Then
        assert!(!is_builtin_directive_name("verification-criteria"));
        assert!(!is_builtin_directive_name("req"));
        assert!(!is_builtin_directive_name("safety-comment"));
    }

    #[test]
    fn test_a_representative_directive_name_from_each_family_is_reserved() {
        // Given / When / Then
        assert!(is_builtin_directive_name("note"));
        assert!(is_builtin_directive_name("toctree"));
        assert!(is_builtin_directive_name("csv-table"));
        assert!(is_builtin_directive_name("code-block"));
        assert!(is_builtin_directive_name("image"));
        assert!(is_builtin_directive_name("include"));
        assert!(is_builtin_directive_name("currentmodule"));
    }
}

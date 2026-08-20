//! Directive-specific rendering helpers (admonitions, version changes, see-also, glossary).

use rusty_sphinx_ast::{ListTableWidths, Node, TableAlign, TableRow, TargetName};
use std::fmt::Write as _;

use super::RenderCtx;

/// Renders an admonition directive (note, warning, hint, etc.) as HTML.
pub(super) fn render_admonition(
    html: &mut String,
    kind: rusty_sphinx_ast::AdmonitionKind,
    title: Option<&str>,
    collapsible: Option<bool>,
    body: &[Node],
    ctx: &mut RenderCtx<'_>,
) {
    let kind_str = kind.as_str();
    let title_text = title.map_or_else(
        || {
            let mut chars = kind_str.chars();
            chars.next().map_or_else(String::new, |c| {
                c.to_uppercase().collect::<String>() + chars.as_str()
            })
        },
        String::from,
    );

    let kind_escaped = html_escape::encode_text(kind_str);
    let title_escaped = html_escape::encode_text(&title_text);

    if let Some(open) = collapsible {
        let open_attr = if open { " open" } else { "" };
        let _ = writeln!(
            html,
            "<details class=\"admonition {kind_escaped}\"{open_attr}>"
        );
        let _ = writeln!(
            html,
            "  <summary class=\"admonition-title\">{title_escaped}</summary>"
        );
        super::render_nodes(html, body, ctx);
        let _ = writeln!(html, "</details>");
    } else {
        let _ = writeln!(html, "<div class=\"admonition {kind_escaped}\">");
        let _ = writeln!(html, "  <p class=\"admonition-title\">{title_escaped}</p>");
        super::render_nodes(html, body, ctx);
        let _ = writeln!(html, "</div>");
    }
}

/// Renders a versionadded / versionchanged / deprecated directive as HTML.
pub(super) fn render_version_change(
    html: &mut String,
    kind: rusty_sphinx_ast::VersionChangeKind,
    version: &str,
    body: &[Node],
    ctx: &mut RenderCtx<'_>,
) {
    let kind_str = kind.as_str();
    let version_escaped = html_escape::encode_text(version);

    let label = match kind {
        rusty_sphinx_ast::VersionChangeKind::Added => format!("New in version {version_escaped}:"),
        rusty_sphinx_ast::VersionChangeKind::Changed => {
            format!("Changed in version {version_escaped}:")
        }
        rusty_sphinx_ast::VersionChangeKind::Deprecated => {
            format!("Deprecated since version {version_escaped}:")
        }
    };

    let inner_class = match kind {
        rusty_sphinx_ast::VersionChangeKind::Added => "added",
        rusty_sphinx_ast::VersionChangeKind::Changed => "changed",
        rusty_sphinx_ast::VersionChangeKind::Deprecated => "deprecated",
    };

    let _ = writeln!(html, "<div class=\"{kind_str}\">");
    let _ = write!(html, "  <p class=\"versionmodified {inner_class}\">");
    let _ = write!(html, "<span class=\"versionmodified-label\">{label}</span>");

    if body.is_empty() {
        let _ = writeln!(html, "</p>");
        let _ = writeln!(html, "</div>");
        return;
    }

    let _ = writeln!(html, "</p>");
    super::render_nodes(html, body, ctx);
    let _ = writeln!(html, "</div>");
}

/// Renders a `seealso` directive as an admonition-style HTML block.
pub(super) fn render_seealso(html: &mut String, body: &[Node], ctx: &mut RenderCtx<'_>) {
    let _ = writeln!(html, "<div class=\"admonition seealso\">");
    let _ = writeln!(html, "  <p class=\"admonition-title\">See also</p>");
    super::render_nodes(html, body, ctx);
    let _ = writeln!(html, "</div>");
}

/// Renders a domain object directive (e.g. `.. py:function::`, `.. c:function::`,
/// `.. py:module::`, `.. py:data::`) as a Sphinx-style object description
/// (`<dl class="{domain} {objtype}">`), using the same qualified key as the
/// analyzer for the anchor `id`.
///
/// The shared `<dl>`/`<dt>` wrapper and cross-reference key are built
/// generically via `obj`'s accessors; any option specific to one object type
/// (`py:module`'s `platform`/`synopsis`/`deprecated` and `py:data`'s
/// `type`/`value` — real Sphinx has no equivalent module/data index page
/// here, so they're rendered inline as leading `<dd>` paragraphs rather than
/// dropped) is matched explicitly, so adding a new object type with its own
/// options can't be forgotten here.
///
/// The body renders under whatever scope this object establishes (see
/// [`rusty_sphinx_ast::DomainObjectBody::deduce_local_scope`]), popped again
/// afterwards so it can't leak into later siblings — the analyzer's
/// `index_domain_object` applies the same scope to keep index keys and
/// anchor `id`s in agreement.
pub(super) fn render_domain_object(
    html: &mut String,
    obj: &rusty_sphinx_ast::DomainObjectBody,
    ctx: &mut RenderCtx<'_>,
) {
    let object_type = obj.object_type();
    let own_names = obj.names();
    // A `py:module`'s own name is never qualified against the *previous*
    // module: real Sphinx always writes it in full and sets it verbatim as
    // the new current module, matching `index_domain_object` in the analyzer.
    let is_module = matches!(obj, rusty_sphinx_ast::DomainObjectBody::PyModule { .. });
    // Every `c`-domain object qualifies against `ctx.scope.c` instead of
    // `ctx.scope.python` — mirrors the analyzer's `index_domain_object`
    // exactly, so anchor `id`s never drift from the index keys. See that
    // function's doc comment (`known_bugs.md` #2) for why `c:function`/
    // `c:macro` joined this set.
    let uses_c_scope = matches!(
        obj,
        rusty_sphinx_ast::DomainObjectBody::CStruct { .. }
            | rusty_sphinx_ast::DomainObjectBody::CUnion { .. }
            | rusty_sphinx_ast::DomainObjectBody::CMember { .. }
            | rusty_sphinx_ast::DomainObjectBody::CType { .. }
            | rusty_sphinx_ast::DomainObjectBody::CFunction { .. }
            | rusty_sphinx_ast::DomainObjectBody::CMacro { .. }
    );
    // Only the primary name qualifies the scope, exactly as in the analyzer's
    // `index_domain_object`; the rest are aliases that get their own `<dt>`
    // anchor but lend nothing to the body.
    let (qualified_primary, new_segments) = if is_module {
        (own_names.first().clone(), Vec::new())
    } else if uses_c_scope {
        let qualification = ctx.scope.c.qualify(own_names.first());
        (qualification.qualified_name, qualification.new_segments)
    } else {
        let qualification = ctx.scope.python.qualify(own_names.first());
        (qualification.qualified_name, qualification.new_segments)
    };
    if is_module {
        ctx.scope.python.set_module(&qualified_primary);
    }
    let domain_str = object_type.domain().as_str();
    let objtype_str = object_type.as_str();

    let _ = writeln!(html, "<dl class=\"{domain_str} {objtype_str}\">");
    // One `<dt>` per declared signature, all sharing the single `<dd>` below —
    // the shape real Sphinx renders a multi-signature object description in.
    for (index_in_object, (own_name, signature_text)) in own_names
        .as_slice()
        .iter()
        .zip(obj.signature_texts())
        .enumerate()
    {
        let qualified_name = if index_in_object == 0 {
            qualified_primary.clone()
        } else if uses_c_scope {
            ctx.scope.c.qualify(own_name).qualified_name
        } else {
            ctx.scope.python.qualify(own_name).qualified_name
        };
        let sig_escaped = html_escape::encode_text(signature_text);
        // `no_index` means no cross-reference target — omit the `id`
        // entirely rather than emitting a dangling anchor.
        if obj.no_index() {
            let _ = write!(html, "  <dt>");
        } else {
            let key = rusty_sphinx_ast::build_domain_object_key(object_type, &qualified_name);
            let id_attr = html_escape::encode_double_quoted_attribute(key.as_str());
            let _ = write!(html, "  <dt id=\"{id_attr}\">");
        }
        for label in domain_object_prefix_labels(obj) {
            let _ = write!(html, "<em class=\"property\">{label}</em> ");
        }
        // Real Sphinx's `PyDecoratorFunction`/`PyDecoratorMethod` insert a
        // literal `@` (`desc_addname('@', '@')`) directly before the
        // signature name — unlike the `<em class="property">` badges above,
        // which are separate flag annotations, this is fused onto the name
        // itself, so it's prepended inside the same `<code class="sig-name">`
        // element rather than rendered as its own node.
        let decorator_prefix = if is_decorator_signature(obj) { "@" } else { "" };
        let _ = writeln!(
            html,
            "<code class=\"sig-name\">{decorator_prefix}{sig_escaped}</code></dt>"
        );
    }
    let _ = write!(html, "  <dd>");
    render_domain_object_options(html, obj);
    let lend = obj.deduce_local_scope(&new_segments);
    if uses_c_scope {
        let saved = ctx.scope.c.push_containers(&lend);
        super::render_nodes(html, obj.body(), ctx);
        ctx.scope.c.restore_containers(saved);
    } else {
        let depth = ctx.scope.python.push_classes(&lend);
        super::render_nodes(html, obj.body(), ctx);
        ctx.scope.python.truncate_classes(depth);
    }
    let _ = writeln!(html, "</dd>");
    let _ = writeln!(html, "</dl>");
}

/// Canonical, deterministic prefix-label order for a domain object's `<dt>`
/// (e.g. `abstractmethod`/`async`/`classmethod`/`staticmethod` for
/// `py:method`, `final`/`class` for `py:class`) — independent of how the
/// author wrote the option flags.
fn domain_object_prefix_labels(obj: &rusty_sphinx_ast::DomainObjectBody) -> Vec<&'static str> {
    match obj {
        rusty_sphinx_ast::DomainObjectBody::PyMethod {
            is_classmethod,
            is_staticmethod,
            is_abstractmethod,
            is_async,
            ..
        } => [
            (*is_abstractmethod, "abstractmethod"),
            (*is_async, "async"),
            (*is_classmethod, "classmethod"),
            (*is_staticmethod, "staticmethod"),
        ]
        .into_iter()
        .filter_map(|(active, label)| active.then_some(label))
        .collect(),
        rusty_sphinx_ast::DomainObjectBody::PyClass { is_final, .. } => {
            class_like_prefix_labels(*is_final, "class")
        }
        rusty_sphinx_ast::DomainObjectBody::PyException { is_final, .. } => {
            class_like_prefix_labels(*is_final, "exception")
        }
        _ => Vec::new(),
    }
}

/// Whether this domain object is a `.. decorator::`/`.. decoratormethod::`
/// definition — real Sphinx prefixes such a signature with a literal `@`
/// (`desc_addname('@', '@')` in `PyDecoratorFunction`/`PyDecoratorMethod`),
/// distinct from the `<em class="property">` badges
/// [`domain_object_prefix_labels`] renders for flag options like
/// `classmethod`/`staticmethod`.
fn is_decorator_signature(obj: &rusty_sphinx_ast::DomainObjectBody) -> bool {
    match obj {
        rusty_sphinx_ast::DomainObjectBody::PyFunction { is_decorator, .. }
        | rusty_sphinx_ast::DomainObjectBody::PyMethod { is_decorator, .. } => *is_decorator,
        _ => false,
    }
}

/// Shared prefix-label construction for `py:class`/`py:exception` — both
/// objects have the same `is_final` option and only differ in the trailing
/// label naming the object type (`"class"` vs `"exception"`).
fn class_like_prefix_labels(is_final: bool, kind_label: &'static str) -> Vec<&'static str> {
    let mut labels = Vec::new();
    if is_final {
        labels.push("final");
    }
    labels.push(kind_label);
    labels
}

/// Renders a domain object's type-specific options (`py:module`'s
/// `platform`/`synopsis`/`deprecated`, `py:data`'s `type`/`value`,
/// `py:attribute`'s `type`/`value`/`canonical`) as leading `<dd>` paragraphs.
/// Object types with no such options (`py:function`, `c:function`,
/// `c:macro`, `py:method`, `py:class`, `py:exception`) render nothing here.
fn render_domain_object_options(html: &mut String, obj: &rusty_sphinx_ast::DomainObjectBody) {
    match obj {
        rusty_sphinx_ast::DomainObjectBody::PyModule {
            platform,
            synopsis,
            deprecated,
            ..
        } => {
            if let Some(platform) = platform {
                let _ = write!(
                    html,
                    "<p class=\"platform\">Platform: {}</p>",
                    html_escape::encode_text(platform)
                );
            }
            if let Some(synopsis) = synopsis {
                let _ = write!(
                    html,
                    "<p class=\"synopsis\">{}</p>",
                    html_escape::encode_text(synopsis)
                );
            }
            if *deprecated {
                let _ = write!(html, "<p class=\"deprecated\">Deprecated.</p>");
            }
        }
        rusty_sphinx_ast::DomainObjectBody::PyData { type_, value, .. } => {
            if let Some(type_) = type_ {
                let _ = write!(
                    html,
                    "<p class=\"type\">Type: {}</p>",
                    html_escape::encode_text(type_)
                );
            }
            if let Some(value) = value {
                let _ = write!(
                    html,
                    "<p class=\"value\">Value: {}</p>",
                    html_escape::encode_text(value)
                );
            }
        }
        rusty_sphinx_ast::DomainObjectBody::PyAttribute {
            type_,
            value,
            canonical,
            ..
        } => {
            if let Some(type_) = type_ {
                let _ = write!(
                    html,
                    "<p class=\"type\">Type: {}</p>",
                    html_escape::encode_text(type_)
                );
            }
            if let Some(value) = value {
                let _ = write!(
                    html,
                    "<p class=\"value\">Value: {}</p>",
                    html_escape::encode_text(value)
                );
            }
            if let Some(canonical) = canonical {
                let _ = write!(
                    html,
                    "<p class=\"canonical\">Canonical: {}</p>",
                    html_escape::encode_text(canonical)
                );
            }
        }
        rusty_sphinx_ast::DomainObjectBody::PyFunction { .. }
        | rusty_sphinx_ast::DomainObjectBody::CFunction { .. }
        | rusty_sphinx_ast::DomainObjectBody::CMacro { .. }
        | rusty_sphinx_ast::DomainObjectBody::CStruct { .. }
        | rusty_sphinx_ast::DomainObjectBody::CUnion { .. }
        | rusty_sphinx_ast::DomainObjectBody::CMember { .. }
        | rusty_sphinx_ast::DomainObjectBody::CType { .. }
        | rusty_sphinx_ast::DomainObjectBody::PyMethod { .. }
        | rusty_sphinx_ast::DomainObjectBody::PyClass { .. }
        | rusty_sphinx_ast::DomainObjectBody::PyException { .. } => {}
    }
}

/// Renders a `glossary` directive as a definition list (`<dl>`).
pub(super) fn render_glossary(
    html: &mut String,
    entries: &[rusty_sphinx_ast::GlossaryEntry],
    ctx: &mut RenderCtx<'_>,
) {
    let _ = writeln!(html, "<dl class=\"glossary\">");
    for entry in entries {
        for term in &entry.terms {
            let term_escaped = html_escape::encode_text(term);
            let id = rusty_sphinx_ast::term_id(term);
            let id_attr = html_escape::encode_double_quoted_attribute(&id);
            let _ = writeln!(html, "  <dt id=\"{id_attr}\">{term_escaped}</dt>");
        }
        let _ = write!(html, "  <dd>");
        super::render_nodes(html, &entry.definition, ctx);
        let _ = writeln!(html, "</dd>");
    }
    let _ = writeln!(html, "</dl>");
}

/// Renders a `.. index::` directive as a bare, invisible anchor at its
/// document position — like `Node::Comment`, it produces no visible content;
/// the genindex page links here via `id`.
pub(super) fn render_index_anchor(html: &mut String, id: &str) {
    let id_attr = html_escape::encode_double_quoted_attribute(id);
    let _ = writeln!(html, "<span id=\"{id_attr}\"></span>");
}

/// The fields `render_list_table` needs, borrowed straight from
/// [`rusty_sphinx_ast::Directive::ListTable`] — grouped into one struct
/// (rather than ten separate parameters) purely to keep the function's
/// arity reasonable.
#[derive(Clone, Copy)]
pub(super) struct ListTableParams<'a> {
    pub title: Option<&'a str>,
    pub header_rows: usize,
    pub stub_columns: usize,
    pub widths: Option<&'a ListTableWidths>,
    pub width: Option<&'a str>,
    pub align: Option<TableAlign>,
    pub classes: &'a [String],
    pub name: Option<&'a TargetName>,
    pub rows: &'a [TableRow],
}

/// Renders a `.. list-table::` directive as HTML, reusing the grid-table
/// row/cell rendering machinery ([`super::render_table_cell`]) so both table
/// forms share one code path for the actual `<td>`/`<th>` output.
pub(super) fn render_list_table(
    html: &mut String,
    params: ListTableParams<'_>,
    ctx: &mut RenderCtx<'_>,
) {
    let ListTableParams {
        title,
        header_rows,
        stub_columns,
        widths,
        width,
        align,
        classes,
        name,
        rows,
    } = params;

    // A `:name:` anchor is emitted exactly like an explicit hyperlink target
    // (`Node::Target` with no `uri`, see `render_nodes`) — reusing that same
    // mechanism rather than inventing a new target-location concept.
    if let Some(target_name) = name {
        let escaped = html_escape::encode_text(target_name.as_str());
        let _ = writeln!(html, "<a id=\"{escaped}\"></a>");
    }

    let mut class_list = vec!["list-table".to_string()];
    class_list.extend(classes.iter().cloned());
    if let Some(align) = align {
        class_list.push(format!("align-{}", align.as_str()));
    }
    let class_string = class_list.join(" ");
    let class_attr = html_escape::encode_double_quoted_attribute(&class_string);
    let _ = write!(html, "<table class=\"{class_attr}\"");
    if let Some(width) = width {
        let width_escaped = html_escape::encode_double_quoted_attribute(width);
        let _ = write!(html, " style=\"width: {width_escaped}\"");
    }
    let _ = writeln!(html, ">");

    if let Some(title) = title {
        let title_escaped = html_escape::encode_text(title);
        let _ = writeln!(html, "<caption>{title_escaped}</caption>");
    }

    render_list_table_colgroup(html, widths);

    // Defensively re-clamp: `header_rows` is already clamped to `rows.len()`
    // at parse time, but nothing at the type level stops a directly
    // constructed `Directive::ListTable` (e.g. loaded from a hand-edited
    // `.ast` file) from violating that, and slicing `rows[..header_rows]`
    // below would otherwise panic.
    let header_rows = header_rows.min(rows.len());

    if header_rows > 0 {
        let _ = writeln!(html, "<thead>");
        for (row_idx, row) in rows[..header_rows].iter().enumerate() {
            render_list_table_row(html, row, row_idx, header_rows, stub_columns, ctx);
        }
        let _ = writeln!(html, "</thead>");
    }
    let _ = writeln!(html, "<tbody>");
    for (offset, row) in rows[header_rows..].iter().enumerate() {
        render_list_table_row(
            html,
            row,
            header_rows + offset,
            header_rows,
            stub_columns,
            ctx,
        );
    }
    let _ = writeln!(html, "</tbody>");
    let _ = writeln!(html, "</table>");
}

/// Renders a `<colgroup>` for `:widths:`'s explicit-integer-list form,
/// normalizing the values as *relative* weights (per the spec) rather than
/// literal percentages. `Auto`/`Grid`/`None` all mean "let the renderer
/// decide" — no `<colgroup>` at all.
fn render_list_table_colgroup(html: &mut String, widths: Option<&ListTableWidths>) {
    let Some(ListTableWidths::Explicit(cols)) = widths else {
        return;
    };
    let total: u32 = cols.iter().sum();
    if total == 0 {
        return;
    }
    let _ = writeln!(html, "<colgroup>");
    for col in cols {
        let pct = f64::from(*col) * 100.0 / f64::from(total);
        let _ = writeln!(html, "<col style=\"width: {pct:.2}%\" />");
    }
    let _ = writeln!(html, "</colgroup>");
}

/// Renders one list-table row, picking `th`/`td` per cell rather than
/// uniformly per row (unlike grid tables): a cell is a header (`th`) when
/// its row is one of the leading `header_rows`, or when its column is one
/// of the leading `stub_columns` — the latter also gets `scope="row"`
/// (skipped for a cell that's already a header via `header_rows`, since its
/// header-ness there is a column header, not a row header).
fn render_list_table_row(
    html: &mut String,
    row: &TableRow,
    row_idx: usize,
    header_rows: usize,
    stub_columns: usize,
    ctx: &mut RenderCtx<'_>,
) {
    let _ = writeln!(html, "<tr>");
    let is_header_row = row_idx < header_rows;
    for (col_idx, cell) in row.cells.iter().enumerate() {
        let is_stub_column = col_idx < stub_columns;
        let tag = if is_header_row || is_stub_column {
            "th"
        } else {
            "td"
        };
        let scope = (!is_header_row && is_stub_column).then_some("row");
        super::render_table_cell(html, cell, tag, scope, ctx);
    }
    let _ = writeln!(html, "</tr>");
}

#[cfg(test)]
mod tests {
    use super::super::RenderCtx;
    use super::*;
    use rusty_sphinx_ast::{
        Directive, Document, InlineNode, Node, NonEmptyVector, TargetName, TargetSearchOrder,
    };
    use rusty_sphinx_index::ProjectIndex;

    fn render_doc(doc: &Document) -> String {
        let index = ProjectIndex::default();
        crate::render(doc, &index, &doc.path).html
    }

    #[test]
    fn test_render_formats_admonition() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::Admonition {
                kind: rusty_sphinx_ast::AdmonitionKind::Note,
                title: None,
                collapsible: None,
                body: vec![Node::Paragraph(vec![InlineNode::Text(
                    "Note body".to_string(),
                )])],
            })],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<div class=\"admonition note\">"));
        assert!(result.contains("<p class=\"admonition-title\">Note</p>"));
        assert!(result.contains("<p>Note body</p>"));
    }

    #[test]
    fn test_render_formats_collapsible_admonition() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::Admonition {
                kind: rusty_sphinx_ast::AdmonitionKind::Warning,
                title: Some("Custom Warning".to_string()),
                collapsible: Some(false),
                body: vec![Node::Paragraph(vec![InlineNode::Text(
                    "Warning body".to_string(),
                )])],
            })],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<details class=\"admonition warning\">"));
        assert!(result.contains("<summary class=\"admonition-title\">Custom Warning</summary>"));
        assert!(result.contains("<p>Warning body</p>"));
    }

    #[test]
    fn test_render_admonition_static() {
        // Given
        let mut html = String::new();
        let kind = rusty_sphinx_ast::AdmonitionKind::Note;
        let title: Option<String> = None;
        let collapsible: Option<bool> = None;
        let body = vec![Node::Paragraph(vec![InlineNode::Text("Body".to_string())])];
        let index = ProjectIndex::default();
        let anon_targets = vec![];
        let mut anon_index = 0;
        let resolver = crate::domain_resolution::DomainObjectResolver::new(&index);
        let mut ctx = RenderCtx {
            index: &index,
            domain_resolver: &resolver,
            doc_path: "test.rst",
            anon_targets: &anon_targets,
            anon_index: &mut anon_index,
            original_doc_path: "test.rst",
            broken_links: &mut Vec::new(),
            object_type_mismatches: &mut Vec::new(),
            scope: rusty_sphinx_scope::Scope::default(),
        };

        // When
        render_admonition(
            &mut html,
            kind,
            title.as_deref(),
            collapsible,
            &body,
            &mut ctx,
        );

        // Then
        assert!(html.contains("<div class=\"admonition note\""));
        assert!(html.contains("<p class=\"admonition-title\">Note</p>"));
        assert!(html.contains("<p>Body</p>"));
    }

    #[test]
    fn test_render_admonition_collapsible_open() {
        // Given
        let mut html = String::new();
        let kind = rusty_sphinx_ast::AdmonitionKind::Warning;
        let title = Some("Custom Title".to_string());
        let collapsible = Some(true);
        let body = vec![];
        let index = ProjectIndex::default();
        let anon_targets = vec![];
        let mut anon_index = 0;
        let resolver = crate::domain_resolution::DomainObjectResolver::new(&index);
        let mut ctx = RenderCtx {
            index: &index,
            domain_resolver: &resolver,
            doc_path: "test.rst",
            anon_targets: &anon_targets,
            anon_index: &mut anon_index,
            original_doc_path: "test.rst",
            broken_links: &mut Vec::new(),
            object_type_mismatches: &mut Vec::new(),
            scope: rusty_sphinx_scope::Scope::default(),
        };

        // When
        render_admonition(
            &mut html,
            kind,
            title.as_deref(),
            collapsible,
            &body,
            &mut ctx,
        );

        // Then
        assert!(html.contains("<details class=\"admonition warning\" open>"));
        assert!(html.contains("<summary class=\"admonition-title\">Custom Title</summary>"));
    }

    #[test]
    fn test_render_versionadded_produces_correct_html() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::VersionChange {
                kind: rusty_sphinx_ast::VersionChangeKind::Added,
                version: "1.0".to_string(),
                body: vec![Node::Paragraph(vec![InlineNode::Text(
                    "Initial release.".to_string(),
                )])],
            })],
        );

        // When
        let result = render_doc(&doc);

        // Then
        let expected = "<div class=\"versionadded\">\n  <p class=\"versionmodified added\"><span class=\"versionmodified-label\">New in version 1.0:</span></p>\n<p>Initial release.</p>\n</div>\n";
        assert_eq!(result, expected);
    }

    #[test]
    fn test_render_deprecated_produces_correct_html() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::VersionChange {
                kind: rusty_sphinx_ast::VersionChangeKind::Deprecated,
                version: "3.0".to_string(),
                body: vec![Node::Paragraph(vec![InlineNode::Text(
                    "Use new API.".to_string(),
                )])],
            })],
        );

        // When
        let result = render_doc(&doc);

        // Then
        let expected = "<div class=\"deprecated\">\n  <p class=\"versionmodified deprecated\"><span class=\"versionmodified-label\">Deprecated since version 3.0:</span></p>\n<p>Use new API.</p>\n</div>\n";
        assert_eq!(result, expected);
    }

    #[test]
    fn test_render_versionchanged_with_empty_body() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::VersionChange {
                kind: rusty_sphinx_ast::VersionChangeKind::Changed,
                version: "2.0".to_string(),
                body: vec![],
            })],
        );

        // When
        let result = render_doc(&doc);

        // Then
        let expected = "<div class=\"versionchanged\">\n  <p class=\"versionmodified changed\"><span class=\"versionmodified-label\">Changed in version 2.0:</span></p>\n</div>\n";
        assert_eq!(result, expected);
    }

    #[test]
    fn test_render_formats_seealso_with_title_and_body() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::SeeAlso {
                body: vec![Node::Paragraph(vec![InlineNode::Text(
                    "The other page.".to_string(),
                )])],
            })],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<div class=\"admonition seealso\">"));
        assert!(result.contains("<p class=\"admonition-title\">See also</p>"));
        assert!(result.contains("<p>The other page.</p>"));
        assert!(result.contains("</div>"));
    }

    #[test]
    fn test_render_formats_seealso_with_definition_list_body() {
        // Given a seealso body containing a definition list, matching the
        // CPython benchmark's `curses` seealso block
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::SeeAlso {
                body: vec![Node::DefinitionList {
                    items: vec![
                        rusty_sphinx_ast::DefinitionListItem {
                            term: vec![
                                InlineNode::Text("Module ".to_string()),
                                InlineNode::DomainObjectReference {
                                    object_type: rusty_sphinx_ast::ObjectType::Py(
                                        rusty_sphinx_ast::PyObjectType::Module,
                                    ),
                                    name: "curses.ascii".to_string(),
                                    display: "curses.ascii".to_string(),
                                    link: true,
                                    search_order: TargetSearchOrder::LeastQualifiedFirst,
                                },
                            ],
                            definition: vec![Node::Paragraph(vec![InlineNode::Text(
                                "Utilities for working with ASCII characters.".to_string(),
                            )])],
                        },
                        rusty_sphinx_ast::DefinitionListItem {
                            term: vec![InlineNode::Reference {
                                display: "curses-howto".to_string(),
                                target: "curses-howto".to_string(),
                            }],
                            definition: vec![Node::Paragraph(vec![InlineNode::Text(
                                "Tutorial material.".to_string(),
                            )])],
                        },
                    ],
                }],
            })],
        );

        // When
        let result = render_doc(&doc);

        // Then the seealso admonition wraps a proper <dl>/<dt>/<dd> structure
        assert!(result.contains("<div class=\"admonition seealso\">"));
        assert!(result.contains("<dl>"));
        assert!(result.contains("<dt>Module "));
        assert!(result.contains("curses.ascii"));
        assert!(result.contains("<dd><p>Utilities for working with ASCII characters.</p>\n</dd>"));
        assert!(result.contains("</dl>"));
    }

    #[test]
    fn test_render_seealso_static() {
        // Given
        let mut html = String::new();
        let body = vec![Node::Paragraph(vec![InlineNode::Text(
            "See related.".to_string(),
        )])];
        let index = ProjectIndex::default();
        let anon_targets = vec![];
        let mut anon_index = 0;
        let resolver = crate::domain_resolution::DomainObjectResolver::new(&index);
        let mut ctx = RenderCtx {
            index: &index,
            domain_resolver: &resolver,
            doc_path: "test.rst",
            anon_targets: &anon_targets,
            anon_index: &mut anon_index,
            original_doc_path: "test.rst",
            broken_links: &mut Vec::new(),
            object_type_mismatches: &mut Vec::new(),
            scope: rusty_sphinx_scope::Scope::default(),
        };

        // When
        render_seealso(&mut html, &body, &mut ctx);

        // Then
        assert!(html.contains("<div class=\"admonition seealso\""));
        assert!(html.contains("<p class=\"admonition-title\">See also</p>"));
        assert!(html.contains("<p>See related.</p>"));
        assert!(html.contains("</div>"));
    }

    #[test]
    fn test_render_literal_block_without_language() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::LiteralBlock {
                language: None,
                content: "def hello():\n    pass".to_string(),
            }],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert_eq!(result, "<pre><code>def hello():\n    pass</code></pre>\n");
    }

    #[test]
    fn test_render_literal_block_with_language() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::LiteralBlock {
                language: Some("python".to_string()),
                content: "x = 1".to_string(),
            }],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert_eq!(
            result,
            "<pre><code class=\"language-python\">x = 1</code></pre>\n"
        );
    }

    #[test]
    fn test_render_literal_block_escapes_html() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::LiteralBlock {
                language: None,
                content: "a < b && b > c".to_string(),
            }],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("a &lt; b &amp;&amp; b &gt; c"));
    }

    #[test]
    fn test_render_index_directive_produces_invisible_anchor() {
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
        let result = render_doc(&doc);

        // Then — a bare anchor, no other visible content
        assert_eq!(result, "<span id=\"index-0\"></span>\n");
    }

    #[test]
    fn test_render_glossary_single_entry() {
        // Given
        let doc = Document::new(
            "glossary.rst".to_string(),
            vec![Node::Directive(Directive::Glossary {
                entries: vec![rusty_sphinx_ast::GlossaryEntry {
                    terms: vec!["environment".to_string()],
                    definition: vec![Node::Paragraph(vec![InlineNode::Text(
                        "A structure.".to_string(),
                    )])],
                }],
                sorted: false,
            })],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<dl class=\"glossary\">"));
        assert!(result.contains("<dt id=\"term-environment\">environment</dt>"));
        assert!(result.contains("<dd>"));
        assert!(result.contains("A structure."));
        assert!(result.contains("</dl>"));
    }

    #[test]
    fn test_render_glossary_multi_term_entry_produces_multiple_dt() {
        // Given
        let doc = Document::new(
            "glossary.rst".to_string(),
            vec![Node::Directive(Directive::Glossary {
                entries: vec![rusty_sphinx_ast::GlossaryEntry {
                    terms: vec!["term 1".to_string(), "term 2".to_string()],
                    definition: vec![Node::Paragraph(vec![InlineNode::Text(
                        "Shared.".to_string(),
                    )])],
                }],
                sorted: false,
            })],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<dt id=\"term-term-1\">term 1</dt>"));
        assert!(result.contains("<dt id=\"term-term-2\">term 2</dt>"));
        assert_eq!(result.matches("<dd>").count(), 1);
    }

    #[test]
    fn test_render_glossary_escapes_html_in_terms() {
        // Given
        let doc = Document::new(
            "glossary.rst".to_string(),
            vec![Node::Directive(Directive::Glossary {
                entries: vec![rusty_sphinx_ast::GlossaryEntry {
                    terms: vec!["a < b".to_string()],
                    definition: vec![],
                }],
                sorted: false,
            })],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("a &lt; b"));
        assert!(!result.contains("a < b"));
    }

    #[test]
    fn test_render_term_reference_resolved_links_to_glossary_doc() {
        // Given
        let doc = Document::new(
            "guide.rst".to_string(),
            vec![Node::Paragraph(vec![InlineNode::TermReference {
                display: "environment".to_string(),
                term: "environment".to_string(),
            }])],
        );
        let mut index = ProjectIndex::default();
        index
            .glossary_terms
            .insert(TargetName::new("environment"), "glossary.rst".to_string());

        // When
        let result = crate::render(&doc, &index, &doc.path).html;

        // Then
        assert!(result.contains("href=\"glossary.html#term-environment\""));
        assert!(result.contains("class=\"xref std std-term\""));
        assert!(result.contains(">environment<"));
    }

    #[test]
    fn test_render_term_reference_with_custom_display_text() {
        // Given
        let doc = Document::new(
            "guide.rst".to_string(),
            vec![Node::Paragraph(vec![InlineNode::TermReference {
                display: "the env".to_string(),
                term: "environment".to_string(),
            }])],
        );
        let mut index = ProjectIndex::default();
        index
            .glossary_terms
            .insert(TargetName::new("environment"), "glossary.rst".to_string());

        // When
        let result = crate::render(&doc, &index, &doc.path).html;

        // Then
        assert!(result.contains("href=\"glossary.html#term-environment\""));
        assert!(result.contains(">the env<"));
    }

    #[test]
    fn test_render_term_reference_broken_link_when_term_not_found() {
        // Given
        let doc = Document::new(
            "guide.rst".to_string(),
            vec![Node::Paragraph(vec![InlineNode::TermReference {
                display: "unknown".to_string(),
                term: "unknown".to_string(),
            }])],
        );
        let index = ProjectIndex::default(); // empty — no glossary terms

        // When
        let result = crate::render(&doc, &index, &doc.path).html;

        // Then
        assert!(result.contains("class=\"broken-link\""));
        assert!(result.contains(">unknown<"));
    }

    #[test]
    fn test_render_formats_py_function_domain_object() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyFunction {
                    is_decorator: false,
                    signatures: NonEmptyVector::single("greet(name)".to_string()),
                    body: vec![Node::Paragraph(vec![InlineNode::Text(
                        "Greets the given name.".to_string(),
                    )])],
                },
            ))],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<dl class=\"py function\">"));
        assert!(result.contains("<dt id=\"py:function:greet\">"));
        assert!(result.contains("<code class=\"sig-name\">greet(name)</code>"));
        assert!(result.contains("<p>Greets the given name.</p>"));
    }

    #[test]
    fn test_render_prefixes_decorator_signature_with_at_sign() {
        // Given — a `.. decorator::`-derived `PyFunction`: real Sphinx's
        // `PyDecoratorFunction` inserts a literal `@`
        // (`desc_addname('@', '@')`) directly before the signature name.
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyFunction {
                    is_decorator: true,
                    signatures: NonEmptyVector::single("classmethod".to_string()),
                    body: vec![],
                },
            ))],
        );

        // When
        let result = render_doc(&doc);

        // Then — the target key/anchor is still the bare name, unaffected by
        // the `@` (which is display-only, fused into the same `<code>` the
        // way `signature_texts()` is already rendered raw).
        assert!(result.contains("<dt id=\"py:function:classmethod\">"));
        assert!(result.contains("<code class=\"sig-name\">@classmethod</code>"));
    }

    #[test]
    fn test_render_prefixes_decoratormethod_signature_with_at_sign() {
        // Given — the `py:method` counterpart.
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyMethod {
                    signatures: NonEmptyVector::single("register(cls)".to_string()),
                    is_classmethod: false,
                    is_staticmethod: false,
                    is_abstractmethod: false,
                    is_async: false,
                    is_decorator: true,
                    body: vec![],
                },
            ))],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<dt id=\"py:method:register\">"));
        assert!(result.contains("<code class=\"sig-name\">@register(cls)</code>"));
    }

    #[test]
    fn test_render_formats_c_function_domain_object() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::CFunction {
                    signatures: NonEmptyVector::single("int add(int a, int b)".into()),
                    body: vec![Node::Paragraph(vec![InlineNode::Text(
                        "Adds two numbers.".to_string(),
                    )])],
                },
            ))],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<dl class=\"c function\">"));
        assert!(result.contains("<dt id=\"c:function:add\">"));
        assert!(result.contains("<code class=\"sig-name\">int add(int a, int b)</code>"));
    }

    #[test]
    fn test_render_formats_c_macro_domain_object() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::CMacro {
                    signatures: NonEmptyVector::single("MAX(a, b)".into()),
                    body: vec![Node::Paragraph(vec![InlineNode::Text(
                        "Expands to whichever of a or b is greater.".to_string(),
                    )])],
                },
            ))],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<dl class=\"c macro\">"));
        assert!(result.contains("<dt id=\"c:macro:max\">"));
        assert!(result.contains("<code class=\"sig-name\">MAX(a, b)</code>"));
    }

    #[test]
    fn test_render_formats_c_struct_domain_object_with_nested_member() {
        // Given — `.. c:member:: int count` nested inside `.. c:struct:: Data`.
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::CStruct {
                    signatures: NonEmptyVector::single("Data".into()),
                    no_index: false,
                    no_index_entry: false,
                    no_contents_entry: false,
                    body: vec![Node::Directive(Directive::DomainObject(
                        rusty_sphinx_ast::DomainObjectBody::CMember {
                            signatures: NonEmptyVector::single("int count".into()),
                            no_index: false,
                            no_index_entry: false,
                            no_contents_entry: false,
                            body: vec![],
                        },
                    ))],
                },
            ))],
        );

        // When
        let result = render_doc(&doc);

        // Then — the struct's own anchor, and the nested member auto-qualified
        // against it, agreeing with what the analyzer would index.
        assert!(result.contains("<dl class=\"c struct\">"));
        assert!(result.contains("<dt id=\"c:struct:data\">"));
        assert!(result.contains("<dl class=\"c member\">"));
        assert!(result.contains("<dt id=\"c:member:data.count\">"));
        assert!(result.contains("<code class=\"sig-name\">int count</code>"));
    }

    #[test]
    fn test_render_formats_c_union_domain_object() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::CUnion {
                    signatures: NonEmptyVector::single("Number".into()),
                    no_index: false,
                    no_index_entry: false,
                    no_contents_entry: false,
                    body: vec![],
                },
            ))],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<dl class=\"c union\">"));
        assert!(result.contains("<dt id=\"c:union:number\">"));
    }

    #[test]
    fn test_render_formats_c_member_domain_object_flat_dotted_signature() {
        // Given — no enclosing `.. c:struct::`, the real CPython-docs shape.
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::CMember {
                    signatures: NonEmptyVector::single("PyObject *PyTypeObject.tp_bases".into()),
                    no_index: false,
                    no_index_entry: false,
                    no_contents_entry: false,
                    body: vec![],
                },
            ))],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<dl class=\"c member\">"));
        assert!(result.contains("<dt id=\"c:member:pytypeobject.tp_bases\">"));
        assert!(result.contains("<code class=\"sig-name\">PyObject *PyTypeObject.tp_bases</code>"));
    }

    #[test]
    fn test_render_formats_c_type_domain_object_with_nested_macro() {
        // Given — the real CPython `c-api/memory.rst` shape (`known_bugs.md`):
        // enum-style `.. c:macro::` constants nested inside `.. c:type::`.
        // `c:macro` now consults `CScope` like every other `c`-domain object
        // (`known_bugs.md` #2's fix), so the nested macro renders qualified
        // by the enclosing type — a known, accepted mismatch against
        // CPython's actual bare-rendered constants, since rusty-sphinx
        // doesn't implement the `.. c:namespace:: NULL` reset real Sphinx
        // uses there (see the analyzer's equivalent test for detail).
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::CType {
                    signatures: NonEmptyVector::single("PyMemAllocatorDomain".into()),
                    no_index: false,
                    no_index_entry: false,
                    no_contents_entry: false,
                    body: vec![Node::Directive(Directive::DomainObject(
                        rusty_sphinx_ast::DomainObjectBody::CMacro {
                            signatures: NonEmptyVector::single("PYMEM_DOMAIN_RAW".into()),
                            body: vec![],
                        },
                    ))],
                },
            ))],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<dl class=\"c type\">"));
        assert!(result.contains("<dt id=\"c:type:pymemallocatordomain\">"));
        assert!(result.contains("<dl class=\"c macro\">"));
        assert!(result.contains("<dt id=\"c:macro:pymemallocatordomain.pymem_domain_raw\">"));
    }

    #[test]
    fn test_render_formats_c_function_nested_in_py_class_is_not_qualified_by_it() {
        // Given — `known_bugs.md` #2's own reproducer: a `c:function`
        // (structurally) nested inside a `py:class` body must render under
        // its own bare name, not qualified by the enclosing Python
        // module+class — real Sphinx's C domain has no concept of an
        // enclosing Python class at all.
        let doc = Document::new(
            "test.rst".to_string(),
            vec![
                Node::Directive(Directive::DomainObject(
                    rusty_sphinx_ast::DomainObjectBody::PyModule {
                        name: "greeter_module".to_string(),
                        platform: None,
                        synopsis: None,
                        deprecated: false,
                        body: vec![],
                    },
                )),
                Node::Directive(Directive::DomainObject(
                    rusty_sphinx_ast::DomainObjectBody::PyClass {
                        signatures: NonEmptyVector::single("Greeter".to_string()),
                        is_final: false,
                        body: vec![Node::Directive(Directive::DomainObject(
                            rusty_sphinx_ast::DomainObjectBody::CFunction {
                                signatures: NonEmptyVector::single("int helper(void)".into()),
                                body: vec![],
                            },
                        ))],
                    },
                )),
            ],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<dt id=\"c:function:helper\">"));
        assert!(!result.contains("greeter_module.greeter.helper"));
    }

    #[test]
    fn test_render_formats_c_type_domain_object_with_nested_member() {
        // Given — `c:member` consults `CScope`, so nesting it under `c:type`
        // still qualifies it against the enclosing type's name, exactly like
        // nesting under `c:struct`/`c:union`.
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::CType {
                    signatures: NonEmptyVector::single("Data".into()),
                    no_index: false,
                    no_index_entry: false,
                    no_contents_entry: false,
                    body: vec![Node::Directive(Directive::DomainObject(
                        rusty_sphinx_ast::DomainObjectBody::CMember {
                            signatures: NonEmptyVector::single("int count".into()),
                            no_index: false,
                            no_index_entry: false,
                            no_contents_entry: false,
                            body: vec![],
                        },
                    ))],
                },
            ))],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<dl class=\"c type\">"));
        assert!(result.contains("<dt id=\"c:type:data\">"));
        assert!(result.contains("<dl class=\"c member\">"));
        assert!(result.contains("<dt id=\"c:member:data.count\">"));
    }

    #[test]
    fn test_render_formats_c_type_domain_object_typedef_alias_signature() {
        // Given — real Sphinx's `type name` typedef-alias form.
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::CType {
                    signatures: NonEmptyVector::single("unsigned long ulong".into()),
                    no_index: false,
                    no_index_entry: false,
                    no_contents_entry: false,
                    body: vec![],
                },
            ))],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<dl class=\"c type\">"));
        assert!(result.contains("<dt id=\"c:type:ulong\">"));
        assert!(result.contains("<code class=\"sig-name\">unsigned long ulong</code>"));
    }

    #[test]
    fn test_render_anchors_a_function_pointer_typedef_at_its_declared_name() {
        // Given — the anchor comes from the declarator inside the `(*…)`
        // group, while the displayed text stays the full declaration.
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::CType {
                    signatures: NonEmptyVector::single(
                        "int (*Py_tracefunc)(PyObject *obj, int what)".into(),
                    ),
                    no_index: false,
                    no_index_entry: false,
                    no_contents_entry: false,
                    body: vec![],
                },
            ))],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<dt id=\"c:type:py_tracefunc\">"));
        assert!(!result.contains("<dt id=\"c:type:int\">"));
        assert!(result.contains(
            "<code class=\"sig-name\">int (*Py_tracefunc)(PyObject *obj, int what)</code>"
        ));
    }

    #[test]
    fn test_render_omits_id_attribute_when_no_index_is_set_for_c_type() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::CType {
                    signatures: NonEmptyVector::single("Hidden".into()),
                    no_index: true,
                    no_index_entry: false,
                    no_contents_entry: false,
                    body: vec![],
                },
            ))],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(!result.contains("id=\"c:type:hidden\""));
        assert!(result.contains("<code class=\"sig-name\">Hidden</code>"));
    }

    #[test]
    fn test_render_omits_id_attribute_when_no_index_is_set() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::CMember {
                    signatures: NonEmptyVector::single("int count".into()),
                    no_index: true,
                    no_index_entry: false,
                    no_contents_entry: false,
                    body: vec![],
                },
            ))],
        );

        // When
        let result = render_doc(&doc);

        // Then — still typeset, just no anchor.
        assert!(result.contains("<dt>"));
        assert!(!result.contains("id=\"c:member:count\""));
        assert!(result.contains("<code class=\"sig-name\">int count</code>"));
    }

    #[test]
    fn test_render_formats_py_module_domain_object() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyModule {
                    name: "greetings".to_string(),
                    platform: None,
                    synopsis: None,
                    deprecated: false,
                    body: vec![Node::Paragraph(vec![InlineNode::Text(
                        "A module of greetings.".to_string(),
                    )])],
                },
            ))],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<dl class=\"py module\">"));
        assert!(result.contains("<dt id=\"py:module:greetings\">"));
        assert!(result.contains("<code class=\"sig-name\">greetings</code>"));
    }

    #[test]
    fn test_render_formats_py_module_platform_synopsis_and_deprecated() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyModule {
                    name: "greetings".to_string(),
                    platform: Some("Unix, Windows".to_string()),
                    synopsis: Some("Greeting utilities.".to_string()),
                    deprecated: true,
                    body: vec![],
                },
            ))],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<p class=\"platform\">Platform: Unix, Windows</p>"));
        assert!(result.contains("<p class=\"synopsis\">Greeting utilities.</p>"));
        assert!(result.contains("<p class=\"deprecated\">Deprecated.</p>"));
    }

    #[test]
    fn test_render_formats_py_method_domain_object() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyMethod {
                    is_decorator: false,
                    signatures: NonEmptyVector::single("greet(self, name)".to_string()),
                    is_classmethod: false,
                    is_staticmethod: false,
                    is_abstractmethod: false,
                    is_async: false,
                    body: vec![Node::Paragraph(vec![InlineNode::Text(
                        "Greets the given name.".to_string(),
                    )])],
                },
            ))],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<dl class=\"py method\">"));
        assert!(result.contains("<dt id=\"py:method:greet\">"));
        assert!(result.contains("<code class=\"sig-name\">greet(self, name)</code>"));
        assert!(!result.contains("class=\"property\""));
    }

    #[test]
    fn test_render_py_method_modifier_prefixes_in_canonical_order() {
        // Given — options written out of canonical order
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyMethod {
                    is_decorator: false,
                    signatures: NonEmptyVector::single("create(cls)".to_string()),
                    is_classmethod: true,
                    is_staticmethod: false,
                    is_abstractmethod: true,
                    is_async: false,
                    body: vec![],
                },
            ))],
        );

        // When
        let result = render_doc(&doc);

        // Then — abstractmethod is rendered before classmethod regardless of
        // struct-field/author order
        let abstractmethod_pos = result.find("abstractmethod").unwrap();
        let classmethod_pos = result.find("classmethod").unwrap();
        assert!(abstractmethod_pos < classmethod_pos);
        assert!(result.contains("<em class=\"property\">abstractmethod</em>"));
        assert!(result.contains("<em class=\"property\">classmethod</em>"));
        assert!(!result.contains("staticmethod"));
        assert!(!result.contains(">async<"));
    }

    #[test]
    fn test_render_formats_py_class_domain_object() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyClass {
                    signatures: NonEmptyVector::single("greeter".to_string()),
                    is_final: false,
                    body: vec![Node::Paragraph(vec![InlineNode::Text(
                        "A greeter.".to_string(),
                    )])],
                },
            ))],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<dl class=\"py class\">"));
        assert!(result.contains("<dt id=\"py:class:greeter\">"));
        assert!(result.contains("<em class=\"property\">class</em>"));
        assert!(result.contains("<code class=\"sig-name\">greeter</code>"));
        assert!(!result.contains("final"));
    }

    #[test]
    fn test_render_py_class_final_prefix_precedes_class_prefix() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyClass {
                    signatures: NonEmptyVector::single("greeter".to_string()),
                    is_final: true,
                    body: vec![],
                },
            ))],
        );

        // When
        let result = render_doc(&doc);

        // Then
        let final_pos = result.find("final").unwrap();
        let class_pos = result.find("class</em>").unwrap();
        assert!(final_pos < class_pos);
        assert!(result.contains("<em class=\"property\">final</em>"));
    }

    #[test]
    fn test_render_qualifies_method_nested_in_class_id() {
        // Given — a `py:method` nested inside a `py:class` body
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyClass {
                    signatures: NonEmptyVector::single("greeter".to_string()),
                    is_final: false,
                    body: vec![Node::Directive(Directive::DomainObject(
                        rusty_sphinx_ast::DomainObjectBody::PyMethod {
                            is_decorator: false,
                            signatures: NonEmptyVector::single("greet(self, name)".to_string()),
                            is_classmethod: false,
                            is_staticmethod: false,
                            is_abstractmethod: false,
                            is_async: false,
                            body: vec![],
                        },
                    ))],
                },
            ))],
        );

        // When
        let result = render_doc(&doc);

        // Then — the nested method's id and signature are qualified/plain
        // respectively, matching the analyzer's index key exactly.
        assert!(result.contains("<dt id=\"py:method:greeter.greet\">"));
        assert!(result.contains("<code class=\"sig-name\">greet(self, name)</code>"));
    }

    #[test]
    fn test_render_qualifies_nested_classes_two_levels_deep() {
        // Given — a class nested inside another class, each containing a method
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyClass {
                    signatures: NonEmptyVector::single("outer".to_string()),
                    is_final: false,
                    body: vec![Node::Directive(Directive::DomainObject(
                        rusty_sphinx_ast::DomainObjectBody::PyClass {
                            signatures: NonEmptyVector::single("inner".to_string()),
                            is_final: false,
                            body: vec![Node::Directive(Directive::DomainObject(
                                rusty_sphinx_ast::DomainObjectBody::PyMethod {
                                    is_decorator: false,
                                    signatures: NonEmptyVector::single("method(self)".to_string()),
                                    is_classmethod: false,
                                    is_staticmethod: false,
                                    is_abstractmethod: false,
                                    is_async: false,
                                    body: vec![],
                                },
                            ))],
                        },
                    ))],
                },
            ))],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<dt id=\"py:class:outer.inner\">"));
        assert!(result.contains("<dt id=\"py:method:outer.inner.method\">"));
    }

    #[test]
    fn test_render_qualifies_sibling_function_after_module_id() {
        // Given — the real-world CPython shape: `py:module` and the
        // `py:function` it documents are siblings, not nested.
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
                    rusty_sphinx_ast::DomainObjectBody::PyFunction {
                        is_decorator: false,
                        signatures: NonEmptyVector::single("coroutine(gen_func)".to_string()),
                        body: vec![],
                    },
                )),
            ],
        );

        // When
        let result = render_doc(&doc);

        // Then — the function's anchor id is module-qualified, matching the
        // analyzer's index key for `:func:`types.coroutine``.
        assert!(result.contains("<dt id=\"py:function:types.coroutine\">"));
        assert!(result.contains("<code class=\"sig-name\">coroutine(gen_func)</code>"));
    }

    #[test]
    fn test_render_does_not_dedup_module_prefix_in_flat_sibling_signature_id() {
        // Given — the class/module conflation bug this change fixes: real
        // CPython's `datetime.rst` documents `.. classmethod::
        // datetime.strptime` as a column-0 sibling of `.. module::
        // datetime`, with no enclosing `.. class::`. The `datetime.` in the
        // signature is the *class* name (there is a separate `.. class::
        // datetime` elsewhere in the same file) — it only coincides with the
        // module name, and must not be mistaken for a repeat of it.
        let doc = Document::new(
            "test.rst".to_string(),
            vec![
                Node::Directive(Directive::DomainObject(
                    rusty_sphinx_ast::DomainObjectBody::PyModule {
                        name: "datetime".to_string(),
                        platform: None,
                        synopsis: None,
                        deprecated: false,
                        body: vec![],
                    },
                )),
                Node::Directive(Directive::DomainObject(
                    rusty_sphinx_ast::DomainObjectBody::PyMethod {
                        is_decorator: false,
                        signatures: NonEmptyVector::single(
                            "datetime.strptime(date_string, format)".to_string(),
                        ),
                        is_classmethod: true,
                        is_staticmethod: false,
                        is_abstractmethod: false,
                        is_async: false,
                        body: vec![],
                    },
                )),
            ],
        );

        // When
        let result = render_doc(&doc);

        // Then — not collapsed to "py:method:datetime.strptime", and matches
        // the analyzer's index key.
        assert!(result.contains("<dt id=\"py:method:datetime.datetime.strptime\">"));
    }

    #[test]
    fn test_render_composes_module_and_class_qualifiers_in_id() {
        // Given — a class documented as a sibling after `py:module`, with a
        // method nested inside the class — both qualifiers must compose.
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
                        signatures: NonEmptyVector::single("DynamicClassAttribute".to_string()),
                        is_final: false,
                        body: vec![Node::Directive(Directive::DomainObject(
                            rusty_sphinx_ast::DomainObjectBody::PyMethod {
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
        let result = render_doc(&doc);

        // Then — id is lowercased, matching `TargetName`'s normalization
        assert!(result.contains("<dt id=\"py:class:types.dynamicclassattribute\">"));
        assert!(result.contains("<dt id=\"py:method:types.dynamicclassattribute.__get__\">"));
    }

    #[test]
    fn test_render_formats_py_exception_domain_object() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyException {
                    signatures: NonEmptyVector::single("greetererror".to_string()),
                    is_final: false,
                    body: vec![Node::Paragraph(vec![InlineNode::Text(
                        "Raised when greeting fails.".to_string(),
                    )])],
                },
            ))],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<dl class=\"py exception\">"));
        assert!(result.contains("<dt id=\"py:exception:greetererror\">"));
        assert!(result.contains("<em class=\"property\">exception</em>"));
        assert!(result.contains("<code class=\"sig-name\">greetererror</code>"));
        assert!(!result.contains("final"));
    }

    #[test]
    fn test_render_py_exception_final_prefix_precedes_exception_prefix() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyException {
                    signatures: NonEmptyVector::single("greetererror".to_string()),
                    is_final: true,
                    body: vec![],
                },
            ))],
        );

        // When
        let result = render_doc(&doc);

        // Then
        let final_pos = result.find("final").unwrap();
        let exception_pos = result.find("exception</em>").unwrap();
        assert!(final_pos < exception_pos);
        assert!(result.contains("<em class=\"property\">final</em>"));
    }

    #[test]
    fn test_render_qualifies_method_nested_in_exception_id() {
        // Given — a `py:method` nested inside a `py:exception` body
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyException {
                    signatures: NonEmptyVector::single("greetererror".to_string()),
                    is_final: false,
                    body: vec![Node::Directive(Directive::DomainObject(
                        rusty_sphinx_ast::DomainObjectBody::PyMethod {
                            is_decorator: false,
                            signatures: NonEmptyVector::single("reason(self)".to_string()),
                            is_classmethod: false,
                            is_staticmethod: false,
                            is_abstractmethod: false,
                            is_async: false,
                            body: vec![],
                        },
                    ))],
                },
            ))],
        );

        // When
        let result = render_doc(&doc);

        // Then — the nested method's id is qualified, matching the
        // analyzer's index key exactly.
        assert!(result.contains("<dt id=\"py:method:greetererror.reason\">"));
        assert!(result.contains("<code class=\"sig-name\">reason(self)</code>"));
    }

    #[test]
    fn test_render_does_not_double_qualify_already_qualified_nested_attribute() {
        // Given — mirrors CPython's `Doc/library/exceptions.rst`, which
        // nests `.. attribute:: StopIteration.value` (already fully
        // qualified) inside `.. exception:: StopIteration`.
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyException {
                    signatures: NonEmptyVector::single("StopIteration".to_string()),
                    is_final: false,
                    body: vec![Node::Directive(Directive::DomainObject(
                        rusty_sphinx_ast::DomainObjectBody::PyAttribute {
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
        let result = render_doc(&doc);

        // Then — not doubled to "py:attribute:stopiteration.stopiteration.value"
        // (`TargetName` lowercases keys, same as every other domain object test).
        assert!(result.contains("<dt id=\"py:attribute:stopiteration.value\">"));
    }

    #[test]
    fn test_render_class_stack_does_not_leak_across_sibling_classes() {
        // Given — two sibling classes, each with a method of the same name;
        // the second class's method must not inherit the first class's
        // qualifier from a stale, un-popped stack entry.
        let doc = Document::new(
            "test.rst".to_string(),
            vec![
                Node::Directive(Directive::DomainObject(
                    rusty_sphinx_ast::DomainObjectBody::PyClass {
                        signatures: NonEmptyVector::single("first".to_string()),
                        is_final: false,
                        body: vec![Node::Directive(Directive::DomainObject(
                            rusty_sphinx_ast::DomainObjectBody::PyMethod {
                                is_decorator: false,
                                signatures: NonEmptyVector::single("run(self)".to_string()),
                                is_classmethod: false,
                                is_staticmethod: false,
                                is_abstractmethod: false,
                                is_async: false,
                                body: vec![],
                            },
                        ))],
                    },
                )),
                Node::Directive(Directive::DomainObject(
                    rusty_sphinx_ast::DomainObjectBody::PyClass {
                        signatures: NonEmptyVector::single("second".to_string()),
                        is_final: false,
                        body: vec![Node::Directive(Directive::DomainObject(
                            rusty_sphinx_ast::DomainObjectBody::PyMethod {
                                is_decorator: false,
                                signatures: NonEmptyVector::single("run(self)".to_string()),
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
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<dt id=\"py:method:first.run\">"));
        assert!(result.contains("<dt id=\"py:method:second.run\">"));
        assert!(!result.contains("py:method:first.second.run"));
    }

    #[test]
    fn test_render_dotted_method_scopes_its_body_without_leaking_to_siblings() {
        // Given — the flat, dotted-signature shape CPython's `zipfile.rst`
        // uses: a method written as a sibling rather than nested in its
        // class. Its own name-prefix scopes its body (here, a nested
        // attribute), but must be popped again before the next sibling.
        let doc = Document::new(
            "test.rst".to_string(),
            vec![
                Node::Directive(Directive::DomainObject(
                    rusty_sphinx_ast::DomainObjectBody::PyMethod {
                        is_decorator: false,
                        signatures: NonEmptyVector::single("ZipFile.open(name)".to_string()),
                        is_classmethod: false,
                        is_staticmethod: false,
                        is_abstractmethod: false,
                        is_async: false,
                        body: vec![Node::Directive(Directive::DomainObject(
                            rusty_sphinx_ast::DomainObjectBody::PyAttribute {
                                signatures: NonEmptyVector::single("mode".to_string()),
                                type_: None,
                                value: None,
                                canonical: None,
                                body: vec![],
                            },
                        ))],
                    },
                )),
                Node::Directive(Directive::DomainObject(
                    rusty_sphinx_ast::DomainObjectBody::PyFunction {
                        is_decorator: false,
                        signatures: NonEmptyVector::single("is_zipfile(filename)".to_string()),
                        body: vec![],
                    },
                )),
            ],
        );

        // When
        let result = render_doc(&doc);

        // Then — the nested attribute is scoped by the method's prefix, and
        // the following sibling is untouched by that scope.
        assert!(result.contains("<dt id=\"py:method:zipfile.open\">"));
        assert!(result.contains("<dt id=\"py:attribute:zipfile.mode\">"));
        assert!(result.contains("<dt id=\"py:function:is_zipfile\">"));
        assert!(!result.contains("py:function:zipfile.is_zipfile"));
    }

    #[test]
    fn test_render_formats_py_data_domain_object() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyData {
                    signatures: NonEmptyVector::single("DEFAULT_TIMEOUT".to_string()),
                    type_: None,
                    value: None,
                    body: vec![Node::Paragraph(vec![InlineNode::Text(
                        "The default timeout in seconds.".to_string(),
                    )])],
                },
            ))],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<dl class=\"py data\">"));
        assert!(result.contains("<dt id=\"py:data:default_timeout\">"));
        assert!(result.contains("<code class=\"sig-name\">DEFAULT_TIMEOUT</code>"));
    }

    #[test]
    fn test_render_formats_py_data_type_and_value() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyData {
                    signatures: NonEmptyVector::single("DEFAULT_TIMEOUT".to_string()),
                    type_: Some("int".to_string()),
                    value: Some("30".to_string()),
                    body: vec![],
                },
            ))],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<p class=\"type\">Type: int</p>"));
        assert!(result.contains("<p class=\"value\">Value: 30</p>"));
    }

    #[test]
    fn test_render_formats_py_attribute_domain_object() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyAttribute {
                    signatures: NonEmptyVector::single("Greeter.name".to_string()),
                    type_: None,
                    value: None,
                    canonical: None,
                    body: vec![Node::Paragraph(vec![InlineNode::Text(
                        "The greeter's name.".to_string(),
                    )])],
                },
            ))],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<dl class=\"py attribute\">"));
        assert!(result.contains("<dt id=\"py:attribute:greeter.name\">"));
        assert!(result.contains("<code class=\"sig-name\">Greeter.name</code>"));
    }

    #[test]
    fn test_render_formats_py_attribute_type_value_and_canonical() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyAttribute {
                    signatures: NonEmptyVector::single("Greeter.name".to_string()),
                    type_: Some("str".to_string()),
                    value: Some("\"anonymous\"".to_string()),
                    canonical: Some("mymodule.MyClass.name".to_string()),
                    body: vec![],
                },
            ))],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<p class=\"type\">Type: str</p>"));
        assert!(result.contains("<p class=\"value\">Value: \"anonymous\"</p>"));
        assert!(result.contains("<p class=\"canonical\">Canonical: mymodule.MyClass.name</p>"));
    }

    #[test]
    fn test_render_domain_object_resolves_nested_anonymous_hyperlink_in_body() {
        // Given — regression test for the collect_anonymous_targets catch-all:
        // an anonymous reference nested inside a DomainObject body must still resolve.
        let doc = Document::new(
            "test.rst".to_string(),
            vec![
                Node::Directive(Directive::DomainObject(
                    rusty_sphinx_ast::DomainObjectBody::PyFunction {
                        is_decorator: false,
                        signatures: NonEmptyVector::single("greet(name)".to_string()),
                        body: vec![Node::Paragraph(vec![InlineNode::AnonymousReference(
                            "See more".to_string(),
                        )])],
                    },
                )),
                Node::AnonymousTarget {
                    uri: "https://example.com".to_string(),
                },
            ],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<a href=\"https://example.com\">See more</a>"));
    }

    #[test]
    fn test_render_term_reference_computes_relative_path_across_directories() {
        // Given — document is in a subdirectory, glossary is at root
        let doc = Document::new(
            "guide/page.rst".to_string(),
            vec![Node::Paragraph(vec![InlineNode::TermReference {
                display: "foo".to_string(),
                term: "foo".to_string(),
            }])],
        );
        let mut index = ProjectIndex::default();
        index
            .glossary_terms
            .insert(TargetName::new("foo"), "glossary.rst".to_string());

        // When
        let result = crate::render(&doc, &index, &doc.path).html;

        // Then — href should traverse up one directory
        assert!(result.contains("href=\"../glossary.html#term-foo\""));
    }

    #[test]
    fn test_domain_object_prefix_labels_orders_method_flags_independent_of_input_order() {
        // Given — flags set in a different order than the canonical output order
        let obj = rusty_sphinx_ast::DomainObjectBody::PyMethod {
            is_decorator: false,
            signatures: NonEmptyVector::single("run()".to_string()),
            is_classmethod: true,
            is_staticmethod: true,
            is_abstractmethod: true,
            is_async: true,
            body: vec![],
        };

        // When
        let labels = domain_object_prefix_labels(&obj);

        // Then
        assert_eq!(
            labels,
            vec!["abstractmethod", "async", "classmethod", "staticmethod"]
        );
    }

    #[test]
    fn test_domain_object_prefix_labels_omits_inactive_method_flags() {
        // Given
        let obj = rusty_sphinx_ast::DomainObjectBody::PyMethod {
            is_decorator: false,
            signatures: NonEmptyVector::single("run()".to_string()),
            is_classmethod: false,
            is_staticmethod: true,
            is_abstractmethod: false,
            is_async: false,
            body: vec![],
        };

        // When
        let labels = domain_object_prefix_labels(&obj);

        // Then
        assert_eq!(labels, vec!["staticmethod"]);
    }

    #[test]
    fn test_domain_object_prefix_labels_includes_final_before_class_label() {
        // Given
        let obj = rusty_sphinx_ast::DomainObjectBody::PyClass {
            signatures: NonEmptyVector::single("Greeter".to_string()),
            is_final: true,
            body: vec![],
        };

        // When
        let labels = domain_object_prefix_labels(&obj);

        // Then
        assert_eq!(labels, vec!["final", "class"]);
    }

    #[test]
    fn test_domain_object_prefix_labels_includes_final_before_exception_label() {
        // Given
        let obj = rusty_sphinx_ast::DomainObjectBody::PyException {
            signatures: NonEmptyVector::single("GreeterError".to_string()),
            is_final: true,
            body: vec![],
        };

        // When
        let labels = domain_object_prefix_labels(&obj);

        // Then
        assert_eq!(labels, vec!["final", "exception"]);
    }

    #[test]
    fn test_class_like_prefix_labels_omits_final_when_not_set() {
        // Given / When
        let labels = class_like_prefix_labels(false, "exception");

        // Then
        assert_eq!(labels, vec!["exception"]);
    }

    #[test]
    fn test_domain_object_prefix_labels_is_empty_for_object_types_without_flags() {
        // Given
        let obj = rusty_sphinx_ast::DomainObjectBody::PyFunction {
            is_decorator: false,
            signatures: NonEmptyVector::single("greet(name)".to_string()),
            body: vec![],
        };

        // When
        let labels = domain_object_prefix_labels(&obj);

        // Then
        assert!(labels.is_empty());
    }

    #[test]
    fn test_is_decorator_signature_true_for_decorator_function() {
        // Given
        let obj = rusty_sphinx_ast::DomainObjectBody::PyFunction {
            signatures: NonEmptyVector::single("classmethod".to_string()),
            is_decorator: true,
            body: vec![],
        };

        // When / Then
        assert!(is_decorator_signature(&obj));
    }

    #[test]
    fn test_is_decorator_signature_true_for_decoratormethod() {
        // Given
        let obj = rusty_sphinx_ast::DomainObjectBody::PyMethod {
            signatures: NonEmptyVector::single("register(cls)".to_string()),
            is_classmethod: false,
            is_staticmethod: false,
            is_abstractmethod: false,
            is_async: false,
            is_decorator: true,
            body: vec![],
        };

        // When / Then
        assert!(is_decorator_signature(&obj));
    }

    #[test]
    fn test_is_decorator_signature_false_for_plain_function() {
        // Given
        let obj = rusty_sphinx_ast::DomainObjectBody::PyFunction {
            signatures: NonEmptyVector::single("greet(name)".to_string()),
            is_decorator: false,
            body: vec![],
        };

        // When / Then
        assert!(!is_decorator_signature(&obj));
    }

    #[test]
    fn test_is_decorator_signature_false_for_object_types_without_the_flag() {
        // Given — e.g. `py:class`, which has no `is_decorator` field at all.
        let obj = rusty_sphinx_ast::DomainObjectBody::PyClass {
            signatures: NonEmptyVector::single("Greeter".to_string()),
            is_final: false,
            body: vec![],
        };

        // When / Then
        assert!(!is_decorator_signature(&obj));
    }

    #[test]
    fn test_render_domain_object_emits_one_dt_per_declared_name() {
        // Given — the confirmed `library/socket.rst` shape.
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyData {
                    signatures: NonEmptyVector::new(
                        "AF_UNIX".to_string(),
                        vec!["AF_INET".to_string(), "AF_INET6".to_string()],
                    ),
                    type_: None,
                    value: None,
                    body: vec![Node::Paragraph(vec![InlineNode::Text(
                        "The address families.".to_string(),
                    )])],
                },
            ))],
        );

        // When
        let result = render_doc(&doc);

        // Then — one anchor per alias, all inside a single `<dl>`/`<dd>` pair.
        assert!(result.contains("<dt id=\"py:data:af_unix\">"));
        assert!(result.contains("<dt id=\"py:data:af_inet\">"));
        assert!(result.contains("<dt id=\"py:data:af_inet6\">"));
        assert_eq!(result.matches("<dt id=").count(), 3);
        assert_eq!(result.matches("<dd>").count(), 1);
        assert_eq!(result.matches("<dl class=").count(), 1);
        // The shared body renders once, under the single `<dd>`.
        assert_eq!(result.matches("The address families.").count(), 1);
    }

    #[test]
    fn test_render_domain_object_shows_each_signature_as_its_own_dt_text() {
        // Given — display text keeps the full signature, while the anchor
        // uses only the extracted name.
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyFunction {
                    is_decorator: false,
                    signatures: NonEmptyVector::new(
                        "spawnl(mode, file)".to_string(),
                        vec!["spawnle(mode, file, env)".to_string()],
                    ),
                    body: vec![],
                },
            ))],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<dt id=\"py:function:spawnl\">"));
        assert!(result.contains("<code class=\"sig-name\">spawnl(mode, file)</code>"));
        assert!(result.contains("<dt id=\"py:function:spawnle\">"));
        assert!(result.contains("<code class=\"sig-name\">spawnle(mode, file, env)</code>"));
    }

    #[test]
    fn test_render_domain_object_repeats_prefix_labels_on_every_dt() {
        // Given — flags such as `classmethod` belong to the directive as a
        // whole, so every alias's `<dt>` carries them.
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyMethod {
                    is_decorator: false,
                    signatures: NonEmptyVector::new(
                        "from_bytes(cls, data)".to_string(),
                        vec!["from_buffer(cls, buf)".to_string()],
                    ),
                    is_classmethod: true,
                    is_staticmethod: false,
                    is_abstractmethod: false,
                    is_async: false,
                    body: vec![],
                },
            ))],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert_eq!(
            result
                .matches("<em class=\"property\">classmethod</em>")
                .count(),
            2
        );
    }

    #[test]
    fn test_render_domain_object_qualifies_every_alias_by_the_current_module() {
        // Given — the renderer's anchors must match the keys the analyzer
        // registered, for every alias and not just the primary.
        let doc = Document::new(
            "test.rst".to_string(),
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
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<dt id=\"py:data:socket.af_unix\">"));
        assert!(result.contains("<dt id=\"py:data:socket.af_inet\">"));
    }

    #[test]
    fn test_render_domain_object_options_renders_nothing_for_py_function() {
        // Given
        let obj = rusty_sphinx_ast::DomainObjectBody::PyFunction {
            is_decorator: false,
            signatures: NonEmptyVector::single("greet(name)".to_string()),
            body: vec![],
        };
        let mut html = String::new();

        // When
        render_domain_object_options(&mut html, &obj);

        // Then
        assert!(html.is_empty());
    }

    #[test]
    fn test_render_domain_object_options_renders_nothing_for_py_exception() {
        // Given
        let obj = rusty_sphinx_ast::DomainObjectBody::PyException {
            signatures: NonEmptyVector::single("GreeterError".to_string()),
            is_final: false,
            body: vec![],
        };
        let mut html = String::new();

        // When
        render_domain_object_options(&mut html, &obj);

        // Then
        assert!(html.is_empty());
    }

    fn list_table_row(cells: &[&str]) -> TableRow {
        rusty_sphinx_ast::TableRow {
            cells: cells
                .iter()
                .map(|text| rusty_sphinx_ast::TableCell {
                    colspan: 1,
                    rowspan: 1,
                    content: vec![Node::Paragraph(vec![InlineNode::Text((*text).to_string())])],
                })
                .collect(),
        }
    }

    #[test]
    fn test_render_list_table_basic_two_by_two() {
        // Given
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
                rows: vec![
                    list_table_row(&["Fruit", "Colour"]),
                    list_table_row(&["Apple", "Red"]),
                ],
            })],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<table class=\"list-table\">"));
        assert!(!result.contains("<thead>"));
        assert!(result.contains("<td>"));
        assert!(result.contains("<p>Fruit</p>"));
    }

    #[test]
    fn test_render_list_table_header_rows_produces_thead() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::ListTable {
                title: None,
                header_rows: 1,
                stub_columns: 0,
                widths: None,
                width: None,
                align: None,
                classes: vec![],
                name: None,
                rows: vec![
                    list_table_row(&["Fruit", "Colour"]),
                    list_table_row(&["Apple", "Red"]),
                ],
            })],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<thead>"));
        assert!(result.contains("<th>"));
        assert!(result.contains("<tbody>"));
    }

    #[test]
    fn test_render_list_table_stub_columns_produces_mixed_th_td_in_body_row() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::ListTable {
                title: None,
                header_rows: 0,
                stub_columns: 1,
                widths: None,
                width: None,
                align: None,
                classes: vec![],
                name: None,
                rows: vec![list_table_row(&["Stub", "Cell"])],
            })],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<th scope=\"row\">"));
        assert!(result.contains("<td>"));
    }

    #[test]
    fn test_render_list_table_title_produces_caption() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::ListTable {
                title: Some("Fruit".to_string()),
                header_rows: 0,
                stub_columns: 0,
                widths: None,
                width: None,
                align: None,
                classes: vec![],
                name: None,
                rows: vec![list_table_row(&["Cell"])],
            })],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<caption>Fruit</caption>"));
    }

    #[test]
    fn test_render_list_table_explicit_widths_produces_colgroup() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::ListTable {
                title: None,
                header_rows: 0,
                stub_columns: 0,
                widths: Some(ListTableWidths::Explicit(vec![30, 70])),
                width: None,
                align: None,
                classes: vec![],
                name: None,
                rows: vec![list_table_row(&["A", "B"])],
            })],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<colgroup>"));
        assert!(result.contains("<col style=\"width: 30.00%\" />"));
        assert!(result.contains("<col style=\"width: 70.00%\" />"));
    }

    #[test]
    fn test_render_list_table_width_option_produces_inline_style() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::ListTable {
                title: None,
                header_rows: 0,
                stub_columns: 0,
                widths: None,
                width: Some("50%".to_string()),
                align: None,
                classes: vec![],
                name: None,
                rows: vec![list_table_row(&["Cell"])],
            })],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("style=\"width: 50%\""));
    }

    #[test]
    fn test_render_list_table_align_option_produces_class() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::ListTable {
                title: None,
                header_rows: 0,
                stub_columns: 0,
                widths: None,
                width: None,
                align: Some(TableAlign::Center),
                classes: vec![],
                name: None,
                rows: vec![list_table_row(&["Cell"])],
            })],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("class=\"list-table align-center\""));
    }

    #[test]
    fn test_render_list_table_class_option_appends_extra_classes() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::ListTable {
                title: None,
                header_rows: 0,
                stub_columns: 0,
                widths: None,
                width: None,
                align: None,
                classes: vec!["custom".to_string()],
                name: None,
                rows: vec![list_table_row(&["Cell"])],
            })],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("class=\"list-table custom\""));
    }

    #[test]
    fn test_render_list_table_name_option_produces_anchor() {
        // Given
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
                rows: vec![list_table_row(&["Cell"])],
            })],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<a id=\"fruit-table\"></a>"));
    }

    #[test]
    fn test_render_list_table_colgroup_skipped_for_auto_and_grid_widths() {
        // Given
        let mut html = String::new();

        // When
        render_list_table_colgroup(&mut html, Some(&ListTableWidths::Auto));
        render_list_table_colgroup(&mut html, Some(&ListTableWidths::Grid));
        render_list_table_colgroup(&mut html, None);

        // Then
        assert!(html.is_empty());
    }

    #[test]
    fn test_render_table_cell_extraction_matches_grid_table_output_byte_for_byte() {
        // Given — the same grid-table shape `test_render_table_with_header`
        // exercises, verifying the `render_table_cell` extraction changed
        // nothing about grid-table rendering.
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Table {
                header_rows: vec![list_table_row(&["Fruit", "Colour"])],
                body_rows: vec![list_table_row(&["Apple", "Red"])],
            }],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert_eq!(
            result,
            "<table>\n\
             <thead>\n<tr>\n<th><p>Fruit</p>\n</th>\n<th><p>Colour</p>\n</th>\n</tr>\n</thead>\n\
             <tbody>\n<tr>\n<td><p>Apple</p>\n</td>\n<td><p>Red</p>\n</td>\n</tr>\n</tbody>\n\
             </table>\n"
        );
    }
}

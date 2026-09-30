//! `c`-domain object rendering: the shared `render_domain_object`
//! dispatcher (in `super::domain_object_directive`) handles every
//! `c:*` object type generically; only `.. option::`/`.. cmdoption::`
//! (the `std` domain) needs its own renderer, since it doesn't fit
//! that function's one-`<dt>`-per-name loop.

use rinx_ast::Node;
use std::fmt::Write as _;

use crate::RenderCtx;

/// Renders a `.. option::`/`.. cmdoption::` definition.
///
/// Split out of [`render_domain_object`] because `StdCmdoption` doesn't fit
/// that function's one-`<dt>`-per-name loop: real Sphinx renders every
/// comma-separated spec on one raw signature *line* together, in a single
/// `<dt>` (e.g. `.. option:: -c, --compress` → one `<dt>` reading
/// `-c, --compress`), even though each spec is independently
/// cross-referenceable. Each declared line still gets its own `<dt>`,
/// sharing one `<dd>` below — matching every other multi-signature object
/// type — but *within* one line's `<dt>`, only the first spec's anchor
/// becomes the `<dt>`'s own `id`; every additional spec on that line gets an
/// invisible zero-width `<a id="...">` anchor placed inside the same `<dt>`,
/// so each flag still resolves to its own fragment id on one shared visible
/// block (the same technique docutils' own HTML writer uses for a node
/// registered under multiple ids).
///
/// Qualification (`ctx.scope.program.qualify`) mirrors the analyzer's
/// `index_domain_object` exactly, so anchor `id`s never drift from index
/// keys. Options never nest (`deduce_local_scope` is empty for `StdCmdoption`),
/// so the shared body renders with no scope push/pop.
pub(super) fn render_cmdoption(
    html: &mut String,
    signatures: &rinx_ast::NonEmptyVector<String>,
    body: &[Node],
    ctx: &mut RenderCtx<'_>,
) {
    let object_type = rinx_ast::ObjectType::Std(rinx_ast::StdObjectType::Cmdoption);

    let _ = writeln!(html, "<dl class=\"std cmdoption\">");
    for line in signatures.as_slice() {
        let specs = rinx_ast::split_option_line_specs(line);
        let line_escaped = html_escape::encode_text(line);
        let mut dt_open = String::from("  <dt");
        let mut secondary_anchors = String::new();
        for (spec_index, spec) in specs.iter().enumerate() {
            let optname = rinx_ast::extract_option_name(spec);
            let qualified_name = ctx.scope.program.qualify(&optname);
            let key = rinx_ast::build_domain_object_key(object_type, &qualified_name);
            let id_attr = html_escape::encode_double_quoted_attribute(key.as_str());
            if spec_index == 0 {
                let _ = write!(dt_open, " id=\"{id_attr}\"");
            } else {
                let _ = write!(secondary_anchors, "<a id=\"{id_attr}\"></a>");
            }
        }
        let _ = writeln!(
            html,
            "{dt_open}>{secondary_anchors}<code class=\"sig-name\">{line_escaped}</code></dt>"
        );
    }
    let _ = write!(html, "  <dd>");
    crate::render_nodes(html, body, ctx);
    let _ = writeln!(html, "</dd>");
    let _ = writeln!(html, "</dl>");
}

#[cfg(test)]
mod tests {
    use rinx_ast::{Directive, Document, InlineNode, Node, NonEmptyVector};
    use rinx_index::ProjectIndex;

    fn render_doc(doc: &Document) -> String {
        let index = ProjectIndex::default();
        crate::render(doc, &index, &doc.path).html
    }

    #[test]
    fn test_render_formats_c_function_domain_object() {
        // Given
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rinx_ast::DomainObjectBody::CFunction {
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
                rinx_ast::DomainObjectBody::CMacro {
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
                rinx_ast::DomainObjectBody::CStruct {
                    signatures: NonEmptyVector::single("Data".into()),
                    no_index: false,
                    no_index_entry: false,
                    no_contents_entry: false,
                    body: vec![Node::Directive(Directive::DomainObject(
                        rinx_ast::DomainObjectBody::CMember {
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
                rinx_ast::DomainObjectBody::CUnion {
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
                rinx_ast::DomainObjectBody::CMember {
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
        // Given — the real CPython `c-api/memory.rst` shape (`docs/dev/known_bugs.md`):
        // enum-style `.. c:macro::` constants nested inside `.. c:type::`.
        // `c:macro` now consults `CScope` like every other `c`-domain object
        // (`docs/dev/known_bugs.md` #2's fix), so the nested macro renders qualified
        // by the enclosing type — a known, accepted mismatch against
        // CPython's actual bare-rendered constants, since rinx
        // doesn't implement the `.. c:namespace:: NULL` reset real Sphinx
        // uses there (see the analyzer's equivalent test for detail).
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rinx_ast::DomainObjectBody::CType {
                    signatures: NonEmptyVector::single("PyMemAllocatorDomain".into()),
                    no_index: false,
                    no_index_entry: false,
                    no_contents_entry: false,
                    body: vec![Node::Directive(Directive::DomainObject(
                        rinx_ast::DomainObjectBody::CMacro {
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
        // Given — `docs/dev/known_bugs.md` #2's own reproducer: a `c:function`
        // (structurally) nested inside a `py:class` body must render under
        // its own bare name, not qualified by the enclosing Python
        // module+class — real Sphinx's C domain has no concept of an
        // enclosing Python class at all.
        let doc = Document::new(
            "test.rst".to_string(),
            vec![
                Node::Directive(Directive::DomainObject(
                    rinx_ast::DomainObjectBody::PyModule {
                        name: "greeter_module".to_string(),
                        options: rinx_ast::ModuleOptions::default(),
                        body: vec![],
                    },
                )),
                Node::Directive(Directive::DomainObject(
                    rinx_ast::DomainObjectBody::PyClass {
                        module: None,
                        signatures: NonEmptyVector::single("Greeter".to_string()),
                        is_final: false,
                        body: vec![Node::Directive(Directive::DomainObject(
                            rinx_ast::DomainObjectBody::CFunction {
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
                rinx_ast::DomainObjectBody::CType {
                    signatures: NonEmptyVector::single("Data".into()),
                    no_index: false,
                    no_index_entry: false,
                    no_contents_entry: false,
                    body: vec![Node::Directive(Directive::DomainObject(
                        rinx_ast::DomainObjectBody::CMember {
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
                rinx_ast::DomainObjectBody::CType {
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
                rinx_ast::DomainObjectBody::CType {
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
                rinx_ast::DomainObjectBody::CType {
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
}

#[cfg(test)]
mod pipeline_tests {
    use crate::render;
    use rinx_ast::{Directive, Document, InlineNode, Node, NonEmptyVector};
    use rinx_index::ProjectIndex;

    #[test]
    fn test_render_cmdoption_domain_object_produces_dt_and_dd() {
        // Given
        let doc = Document::new(
            "cmdline.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rinx_ast::DomainObjectBody::StdCmdoption {
                    signatures: NonEmptyVector::single("-m <module-name>".to_string()),
                    body: vec![Node::Paragraph(vec![InlineNode::Text(
                        "Run a module.".to_string(),
                    )])],
                },
            ))],
        );
        let index = ProjectIndex::default();

        // When
        let output = render(&doc, &index, &doc.path);

        // Then
        assert!(output.html.contains("<dl class=\"std cmdoption\">"));
        assert!(output.html.contains(
            "<dt id=\"std:cmdoption:-m\"><code class=\"sig-name\">-m &lt;module-name&gt;</code></dt>"
        ));
        assert!(output.html.contains("<dd><p>Run a module.</p>\n</dd>"));
    }
    #[test]
    fn test_render_cmdoption_comma_separated_flags_share_one_dt_with_two_anchors() {
        // Given — real Sphinx's `.. option:: -c, --compress` shape.
        let doc = Document::new(
            "zipapp.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rinx_ast::DomainObjectBody::StdCmdoption {
                    signatures: NonEmptyVector::single("-c, --compress".to_string()),
                    body: vec![],
                },
            ))],
        );
        let index = ProjectIndex::default();

        // When
        let output = render(&doc, &index, &doc.path);

        // Then — one shared `<dt>`, the first flag's id, and the second
        // flag as an invisible anchor inside the same `<dt>`.
        assert!(output.html.contains(
            "<dt id=\"std:cmdoption:-c\"><a id=\"std:cmdoption:--compress\"></a><code class=\"sig-name\">-c, --compress</code></dt>"
        ));
    }
    #[test]
    fn test_render_cmdoption_qualifies_anchor_by_ambient_program() {
        // Given
        let doc = Document::new(
            "dis.rst".to_string(),
            vec![
                Node::Directive(Directive::StdProgram {
                    name: Some("dis".to_string()),
                }),
                Node::Directive(Directive::DomainObject(
                    rinx_ast::DomainObjectBody::StdCmdoption {
                        signatures: NonEmptyVector::single("-O".to_string()),
                        body: vec![],
                    },
                )),
            ],
        );
        let index = ProjectIndex::default();

        // When
        let output = render(&doc, &index, &doc.path);

        // Then
        assert!(output.html.contains("<dt id=\"std:cmdoption:dis.-o\">"));
    }
}

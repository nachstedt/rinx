//! The domain-object directive dispatcher (`.. py:function::`,
//! `.. c:function::`, etc.) and its type-specific option rendering.

use std::fmt::Write as _;

use super::c::render_cmdoption;
use super::labels::{domain_object_prefix_labels, is_decorator_signature};
use crate::RenderCtx;

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
pub(crate) fn render_domain_object(
    html: &mut String,
    obj: &rusty_sphinx_ast::DomainObjectBody,
    ctx: &mut RenderCtx<'_>,
) {
    if let rusty_sphinx_ast::DomainObjectBody::StdCmdoption { signatures, body } = obj {
        render_cmdoption(html, signatures, body, ctx);
        return;
    }

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
    // The `:module:` option: overrides `ctx.scope.python`'s current module
    // for the duration of this whole call — this object's own (and its
    // aliases') qualification below, and its nested body — restored at the
    // very end, mirroring the analyzer's `index_domain_object` exactly (see
    // there for why this is what real Sphinx's `before_content()`/
    // `after_content()` do) so anchor `id`s never drift from index keys.
    let restore_module = obj
        .module_override()
        .map(|module| ctx.scope.python.push_module_override(module));
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
        crate::render_nodes(html, obj.body(), ctx);
        ctx.scope.c.restore_containers(saved);
    } else {
        let depth = ctx.scope.python.push_classes(&lend);
        crate::render_nodes(html, obj.body(), ctx);
        ctx.scope.python.truncate_classes(depth);
    }
    let _ = writeln!(html, "</dd>");
    let _ = writeln!(html, "</dl>");

    if let Some(previous) = restore_module {
        ctx.scope.python.restore_module(previous);
    }
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
        | rusty_sphinx_ast::DomainObjectBody::PyException { .. }
        | rusty_sphinx_ast::DomainObjectBody::StdCmdoption { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_sphinx_ast::{Directive, Document, InlineNode, Node, NonEmptyVector};
    use rusty_sphinx_index::ProjectIndex;

    fn render_doc(doc: &Document) -> String {
        let index = ProjectIndex::default();
        crate::render(doc, &index, &doc.path).html
    }

    #[test]
    fn test_render_domain_object_repeats_prefix_labels_on_every_dt() {
        // Given — flags such as `classmethod` belong to the directive as a
        // whole, so every alias's `<dt>` carries them.
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyMethod {
                    module: None,
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
    fn test_render_domain_object_emits_one_dt_per_declared_name() {
        // Given — the confirmed `library/socket.rst` shape.
        let doc = Document::new(
            "test.rst".to_string(),
            vec![Node::Directive(Directive::DomainObject(
                rusty_sphinx_ast::DomainObjectBody::PyData {
                    module: None,
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
                    module: None,
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
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<dt id=\"py:data:socket.af_unix\">"));
        assert!(result.contains("<dt id=\"py:data:socket.af_inet\">"));
    }
    #[test]
    fn test_render_domain_object_module_option_overrides_the_anchor_id() {
        // Given — the exact `known_bugs.md` shape: the anchor `id` must use
        // the `:module:` override, matching the analyzer's index key so the
        // cross-reference actually resolves to this anchor.
        let doc = Document::new(
            "test.rst".to_string(),
            vec![
                Node::Directive(Directive::DomainObject(
                    rusty_sphinx_ast::DomainObjectBody::PyModule {
                        name: "multiprocessing.shared_memory".to_string(),
                        platform: None,
                        synopsis: None,
                        deprecated: false,
                        body: vec![],
                    },
                )),
                Node::Directive(Directive::DomainObject(
                    rusty_sphinx_ast::DomainObjectBody::PyClass {
                        module: Some("multiprocessing.managers".to_string()),
                        signatures: NonEmptyVector::single("SharedMemoryManager".to_string()),
                        is_final: false,
                        body: vec![Node::Directive(Directive::DomainObject(
                            rusty_sphinx_ast::DomainObjectBody::PyMethod {
                                module: None,
                                is_decorator: false,
                                signatures: NonEmptyVector::single("get_server()".to_string()),
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
                    rusty_sphinx_ast::DomainObjectBody::PyFunction {
                        module: None,
                        is_decorator: false,
                        signatures: NonEmptyVector::single("track(size)".to_string()),
                        body: vec![],
                    },
                )),
            ],
        );

        // When
        let result = render_doc(&doc);

        // Then — the class and its nested method render under the override...
        assert!(
            result.contains("<dt id=\"py:class:multiprocessing.managers.sharedmemorymanager\">")
        );
        assert!(result.contains(
            "<dt id=\"py:method:multiprocessing.managers.sharedmemorymanager.get_server\">"
        ));
        // ...but the later sibling, with no override of its own, is restored
        // to the enclosing module rather than sticking with the override.
        assert!(result.contains("<dt id=\"py:function:multiprocessing.shared_memory.track\">"));
    }
    #[test]
    fn test_render_domain_object_options_renders_nothing_for_py_function() {
        // Given
        let obj = rusty_sphinx_ast::DomainObjectBody::PyFunction {
            module: None,
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
            module: None,
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
}

#[cfg(test)]
mod python_scoping_tests;
#[cfg(test)]
mod python_tests;

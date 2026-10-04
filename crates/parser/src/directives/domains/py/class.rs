use super::super::body::{ObjectOptions, parse_object_body};
use super::dispatch::read_module_option;
use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::directives::options::OptionLine;
use crate::headings::Adornment;
use rinx_ast::{DomainObjectBody, NonEmptyVector};

/// Parses a `.. py:class::` body: its option block, then its content. Any
/// nested domain object directives (e.g. `.. py:method::`) in the body are
/// parsed through the same recursive `parse_blocks` call every other domain
/// object uses — qualifying their cross-reference names by this class is the
/// analyzer/renderer's job, not the parser's.
pub(crate) fn parse_py_class(
    signatures: NonEmptyVector<String>,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> DomainObjectBody {
    let parsed = parse_object_body::<ClassOptions>(
        "py:class",
        body_lines,
        adornment_order,
        diagnostics,
        ctx,
    );
    DomainObjectBody::PyClass {
        flags: parsed.flags,
        module: parsed.options.module,
        signatures,
        is_final: parsed.options.is_final,
        body: parsed.content,
    }
}

/// Parses a `.. py:exception::` body. Shares [`ClassOptions`] with
/// `.. py:class::`, since both directives have the same option set; only the
/// object type (and thus the produced [`DomainObjectBody`] variant) differs.
pub(crate) fn parse_py_exception(
    signatures: NonEmptyVector<String>,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> DomainObjectBody {
    let parsed = parse_object_body::<ClassOptions>(
        "py:exception",
        body_lines,
        adornment_order,
        diagnostics,
        ctx,
    );
    DomainObjectBody::PyException {
        flags: parsed.flags,
        module: parsed.options.module,
        signatures,
        is_final: parsed.options.is_final,
        body: parsed.content,
    }
}

/// The options `.. py:class::`/`.. py:exception::` take beyond the
/// object-description flags: `:final:`, and the `:module:` every `py` object
/// takes.
#[derive(Debug, Default, PartialEq, Eq)]
struct ClassOptions {
    is_final: bool,
    module: Option<String>,
}

impl ObjectOptions for ClassOptions {
    fn read(&mut self, line: &OptionLine) -> bool {
        match line.name.as_str() {
            "final" => {
                self.is_final = true;
                true
            }
            _ => read_module_option(line, &mut self.module),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::body::test_support::read_options;
    use super::*;
    use crate::parse;
    use rinx_ast::Domain;
    use rinx_ast::{Directive, Node};

    #[test]
    fn test_class_options_read_final_and_module_in_any_order() {
        // Given — the `docs/dev/known_bugs.md` motivating shape: CPython's
        // `multiprocessing.shared_memory.rst` documents `SharedMemoryManager`
        // under a different module via `:module:`.
        let body = [
            ":module: multiprocessing.managers",
            ":final:",
            "",
            "A subclass.",
        ];

        // When
        let (options, _, unrecognized) = read_options::<ClassOptions>(&body);

        // Then
        assert_eq!(
            options,
            ClassOptions {
                is_final: true,
                module: Some("multiprocessing.managers".to_string()),
            }
        );
        assert!(unrecognized.is_empty());
    }
    #[test]
    fn test_class_options_read_nothing_from_a_plain_body() {
        // Given
        let body = ["A greeter."];

        // When
        let (options, _, unrecognized) = read_options::<ClassOptions>(&body);

        // Then
        assert_eq!(options, ClassOptions::default());
        assert!(unrecognized.is_empty());
    }
    #[test]
    fn test_class_options_read_a_bare_module_as_empty() {
        // Given — real Sphinx's falsy-`modname` check: a bare `:module:`
        // deliberately un-qualifies the object.
        let body = [":module:", "A greeter."];

        // When
        let (options, _, _) = read_options::<ClassOptions>(&body);

        // Then
        assert_eq!(options.module.as_deref(), Some(""));
    }
    #[test]
    fn test_parse_py_class_reads_noindex_instead_of_showing_it() {
        // Given — `functions.rst` documents `bytearray` again under
        // `:noindex:`, which used to be rendered as a paragraph of text.
        let input = ".. class:: bytearray(source=b'')\n           bytearray(source, encoding)\n   :noindex:\n\n   Return a new array of bytes.";

        // When
        let doc = parse("test.rst", input);

        // Then
        let Some(Node::Directive(Directive::DomainObject(
            object @ DomainObjectBody::PyClass {
                signatures, body, ..
            },
        ))) = doc.nodes.first()
        else {
            panic!("Expected PyClass, got {:?}", doc.nodes);
        };
        assert!(object.no_index());
        assert_eq!(signatures.as_slice().len(), 2);
        assert_eq!(
            body,
            &[Node::Paragraph(vec![rinx_ast::InlineNode::Text(
                "Return a new array of bytes.".to_string()
            )])]
        );
    }
    #[test]
    fn test_parse_creates_py_class_domain_object() {
        // Given
        let input = ".. py:class:: Greeter\n\n   A greeter.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyClass {
            signatures,
            is_final,
            body,
            ..
        })) = &doc.nodes[0]
        {
            assert_eq!(signatures.as_slice(), ["Greeter"]);
            assert!(!is_final);
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected PyClass, got {:?}", doc.nodes[0]);
        }
    }
    #[test]
    fn test_parse_py_class_with_final_option_and_base_class_signature() {
        // Given
        let input = ".. py:class:: Greeter(Base)\n   :final:\n\n   A greeter.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyClass {
            signatures,
            is_final,
            body,
            ..
        })) = &doc.nodes[0]
        {
            assert_eq!(signatures.as_slice(), ["Greeter(Base)"]);
            assert!(*is_final);
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected PyClass, got {:?}", doc.nodes[0]);
        }
    }
    #[test]
    fn test_parse_py_class_with_module_option_reproduces_known_bugs_shape() {
        // Given — the exact `docs/dev/known_bugs.md` shape:
        // `Doc/library/multiprocessing.shared_memory.rst` documents
        // `SharedMemoryManager` under a different module than the enclosing
        // `.. module::` via `:module:`.
        let input = ".. class:: SharedMemoryManager([address[, authkey]])\n   :module: multiprocessing.managers\n\n   A subclass of BaseManager.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyClass {
            flags: _,
            signatures,
            is_final,
            module,
            body,
        })) = &doc.nodes[0]
        {
            assert_eq!(
                signatures.as_slice(),
                ["SharedMemoryManager([address[, authkey]])"]
            );
            assert!(!is_final);
            assert_eq!(module.as_deref(), Some("multiprocessing.managers"));
            // Symptom #2 from `docs/dev/known_bugs.md`: the `:module:` option line
            // must be stripped as an option, not fall through to become the
            // docstring's first paragraph.
            assert_eq!(
                body,
                &[Node::Paragraph(vec![rinx_ast::InlineNode::Text(
                    "A subclass of BaseManager.".to_string()
                )])]
            );
        } else {
            panic!("Expected PyClass, got {:?}", doc.nodes[0]);
        }
    }
    #[test]
    fn test_parse_py_class_with_no_options_and_no_body() {
        // Given
        let input = ".. py:class:: Greeter";

        // When
        let doc = parse("test.rst", input);

        // Then
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyClass {
            is_final,
            body,
            ..
        })) = &doc.nodes[0]
        {
            assert!(!is_final);
            assert!(body.is_empty());
        } else {
            panic!("Expected PyClass, got {:?}", doc.nodes[0]);
        }
    }
    #[test]
    fn test_parse_creates_py_exception_domain_object() {
        // Given
        let input = ".. py:exception:: GreeterError\n\n   Raised when greeting fails.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyException {
            signatures,
            is_final,
            body,
            ..
        })) = &doc.nodes[0]
        {
            assert_eq!(signatures.as_slice(), ["GreeterError"]);
            assert!(!is_final);
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected PyException, got {:?}", doc.nodes[0]);
        }
    }
    #[test]
    fn test_parse_py_exception_with_final_option_and_base_class_signature() {
        // Given
        let input = ".. py:exception:: InvalidNameError(GreeterError)\n   :final:\n\n   Raised for an invalid name.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyException {
            signatures,
            is_final,
            body,
            ..
        })) = &doc.nodes[0]
        {
            assert_eq!(signatures.as_slice(), ["InvalidNameError(GreeterError)"]);
            assert!(*is_final);
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected PyException, got {:?}", doc.nodes[0]);
        }
    }
    #[test]
    fn test_parse_py_exception_with_no_options_and_no_body() {
        // Given
        let input = ".. py:exception:: GreeterError";

        // When
        let doc = parse("test.rst", input);

        // Then
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyException {
            is_final,
            body,
            ..
        })) = &doc.nodes[0]
        {
            assert!(!is_final);
            assert!(body.is_empty());
        } else {
            panic!("Expected PyException, got {:?}", doc.nodes[0]);
        }
    }
    #[test]
    fn test_parse_bare_exception_directive_resolves_via_default_domain() {
        // Given
        let input = ".. exception:: GreeterError\n\n   Raised when greeting fails.";

        // When
        let doc = crate::parse_with_domain("test.rst", input, Domain::Py);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        assert!(matches!(
            &doc.nodes[0],
            Node::Directive(Directive::DomainObject(
                DomainObjectBody::PyException { .. }
            ))
        ));
    }
    #[test]
    fn test_parse_bare_class_directive_resolves_via_default_domain() {
        // Given
        let input = ".. class:: Greeter\n\n   A greeter.";

        // When
        let doc = crate::parse_with_domain("test.rst", input, Domain::Py);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        assert!(matches!(
            &doc.nodes[0],
            Node::Directive(Directive::DomainObject(DomainObjectBody::PyClass { .. }))
        ));
    }
}

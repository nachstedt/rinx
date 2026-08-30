use super::dispatch::parse_module_option_line;
use crate::blocks::parse_blocks;
use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::headings::Adornment;
use crate::indent::unindent_body_lines;
use rusty_sphinx_ast::{DomainObjectBody, NonEmptyVector};

/// Parses a `.. py:class::` body: strips a leading `:final:` flag line off
/// the front before parsing the rest as the docstring body. Any nested
/// domain object directives (e.g. `.. py:method::`) in the body are parsed
/// through the same recursive `parse_blocks` call every other domain object
/// uses — qualifying their cross-reference names by this class is the
/// analyzer/renderer's job, not the parser's.
pub(crate) fn parse_py_class(
    signatures: NonEmptyVector<String>,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> DomainObjectBody {
    let unindented_lines = unindent_body_lines(body_lines);
    let (is_final, module, options_consumed) = extract_class_options(&unindented_lines);

    let body_content: Vec<&str> = unindented_lines[options_consumed..]
        .iter()
        .map(String::as_str)
        .collect();
    let body = parse_blocks(&body_content, adornment_order, diagnostics, ctx);

    DomainObjectBody::PyClass {
        module,
        signatures,
        is_final,
        body,
    }
}

/// Parses a `.. py:exception::` body: strips a leading `:final:` flag line
/// off the front before parsing the rest as the docstring body. Shares
/// `extract_class_options` with `.. py:class::` since both directives have
/// the same option set; only the object type (and thus the produced
/// [`DomainObjectBody`] variant) differs.
pub(crate) fn parse_py_exception(
    signatures: NonEmptyVector<String>,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> DomainObjectBody {
    let unindented_lines = unindent_body_lines(body_lines);
    let (is_final, module, options_consumed) = extract_class_options(&unindented_lines);

    let body_content: Vec<&str> = unindented_lines[options_consumed..]
        .iter()
        .map(String::as_str)
        .collect();
    let body = parse_blocks(&body_content, adornment_order, diagnostics, ctx);

    DomainObjectBody::PyException {
        module,
        signatures,
        is_final,
        body,
    }
}

/// Extracts `.. py:class::`/`.. py:exception::`-specific options: the
/// `:final:` flag plus `:module:` (shared with every other `py:*`
/// object-description directive) from the leading lines of a domain object's
/// body.
///
/// Scans from the start and stops at the first line that isn't one of these
/// recognized options (e.g. a blank line, a nested directive, or the start
/// of the docstring body), returning how many leading lines were consumed as
/// options so the caller can slice them off before parsing the remaining
/// body content.
fn extract_class_options(lines: &[String]) -> (bool, Option<String>, usize) {
    let mut is_final = false;
    let mut module = None;
    let mut consumed = 0;

    for line in lines {
        let trimmed = line.trim();
        if let Some(module_value) = parse_module_option_line(trimmed) {
            module = Some(module_value);
        } else if trimmed == ":final:" {
            is_final = true;
        } else {
            break;
        }
        consumed += 1;
    }

    (is_final, module, consumed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use rusty_sphinx_ast::Domain;
    use rusty_sphinx_ast::{Directive, Node};

    #[test]
    fn test_extract_class_options_parses_final_flag() {
        // Given
        let lines = vec![
            ":final:".to_string(),
            String::new(),
            "A greeter.".to_string(),
        ];

        // When
        let (is_final, module, consumed) = extract_class_options(&lines);

        // Then
        assert!(is_final);
        assert_eq!(module, None);
        assert_eq!(consumed, 1);
    }
    #[test]
    fn test_extract_class_options_returns_defaults_when_no_options_present() {
        // Given
        let lines = vec!["A greeter.".to_string()];

        // When
        let (is_final, module, consumed) = extract_class_options(&lines);

        // Then
        assert!(!is_final);
        assert_eq!(module, None);
        assert_eq!(consumed, 0);
    }
    #[test]
    fn test_extract_class_options_parses_module_option_alongside_final_flag() {
        // Given — the `known_bugs.md` motivating shape: CPython's
        // `multiprocessing.shared_memory.rst` documents `SharedMemoryManager`
        // under a different module via `:module:`.
        let lines = vec![
            ":module: multiprocessing.managers".to_string(),
            ":final:".to_string(),
            String::new(),
            "A subclass of BaseManager.".to_string(),
        ];

        // When
        let (is_final, module, consumed) = extract_class_options(&lines);

        // Then — order-independent, like the other options.
        assert!(is_final);
        assert_eq!(module.as_deref(), Some("multiprocessing.managers"));
        assert_eq!(consumed, 2);
    }
    #[test]
    fn test_extract_class_options_parses_module_option_with_empty_value() {
        // Given — real Sphinx's falsy-`modname` check: a bare `:module:`
        // deliberately un-qualifies the object.
        let lines = vec![":module:".to_string(), "A greeter.".to_string()];

        // When
        let (is_final, module, consumed) = extract_class_options(&lines);

        // Then
        assert!(!is_final);
        assert_eq!(module.as_deref(), Some(""));
        assert_eq!(consumed, 1);
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
            module: _,
            body,
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
            module: _,
            body,
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
        // Given — the exact `known_bugs.md` shape:
        // `Doc/library/multiprocessing.shared_memory.rst` documents
        // `SharedMemoryManager` under a different module than the enclosing
        // `.. module::` via `:module:`.
        let input = ".. class:: SharedMemoryManager([address[, authkey]])\n   :module: multiprocessing.managers\n\n   A subclass of BaseManager.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::PyClass {
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
            // Symptom #2 from `known_bugs.md`: the `:module:` option line
            // must be stripped as an option, not fall through to become the
            // docstring's first paragraph.
            assert_eq!(
                body,
                &[Node::Paragraph(vec![rusty_sphinx_ast::InlineNode::Text(
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
            module: _,
            body,
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
            module: _,
            body,
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

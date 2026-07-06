use super::blocks::parse_blocks;
use super::headings::Adornment;
use crate::ast::{Directive, Domain, ObjectType};

/// Parses a domain object directive body (e.g. `.. py:function::`,
/// `.. c:function::`) into a `Directive::DomainObject` node.
pub(super) fn parse_domain_object(
    object_type: ObjectType,
    signature: String,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
    default_domain: Domain,
) -> Directive {
    if let Some(first) = body_lines.iter().find(|l| !l.trim().is_empty()) {
        let indent = first.chars().take_while(|c| c.is_whitespace()).count();
        let unindented_lines: Vec<String> = body_lines
            .iter()
            .map(|l| {
                if l.len() >= indent {
                    l[indent..].to_string()
                } else {
                    l.trim().to_string()
                }
            })
            .collect();

        let body_content: Vec<&str> = unindented_lines.iter().map(String::as_str).collect();
        let body_nodes = parse_blocks(&body_content, adornment_order, diagnostics, default_domain);

        Directive::DomainObject {
            object_type,
            signature,
            body: body_nodes,
        }
    } else {
        Directive::DomainObject {
            object_type,
            signature,
            body: vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{CObjectType, Node, PyObjectType};
    use crate::parser::parse;

    #[test]
    fn test_parse_domain_object_with_paragraph_body() {
        // Given
        let object_type = ObjectType::Py(PyObjectType::Function);
        let signature = "greet(name)".to_string();
        let body_lines = vec!["   Greets the given name."];
        let mut adornment_order = Vec::new();
        let mut diagnostics = Vec::new();

        // When
        let directive = parse_domain_object(
            object_type,
            signature.clone(),
            &body_lines,
            &mut adornment_order,
            &mut diagnostics,
            Domain::Py,
        );

        // Then
        if let Directive::DomainObject {
            object_type,
            signature: sig,
            body,
        } = directive
        {
            assert_eq!(object_type, ObjectType::Py(PyObjectType::Function));
            assert_eq!(sig, signature);
            assert_eq!(body.len(), 1);
            assert!(matches!(body[0], Node::Paragraph(_)));
        } else {
            panic!("Expected DomainObject directive");
        }
    }

    #[test]
    fn test_parse_domain_object_with_bullet_list_body() {
        // Given
        let object_type = ObjectType::C(CObjectType::Function);
        let signature = "int add(int a, int b)".to_string();
        let body_lines = vec!["   * Adds two numbers.", "   * Returns their sum."];
        let mut adornment_order = Vec::new();
        let mut diagnostics = Vec::new();

        // When
        let directive = parse_domain_object(
            object_type,
            signature,
            &body_lines,
            &mut adornment_order,
            &mut diagnostics,
            Domain::C,
        );

        // Then
        if let Directive::DomainObject { body, .. } = directive {
            assert_eq!(body.len(), 1);
            assert!(matches!(body[0], Node::BulletList { .. }));
        } else {
            panic!("Expected DomainObject directive");
        }
    }

    #[test]
    fn test_parse_domain_object_with_empty_body() {
        // Given
        let object_type = ObjectType::Py(PyObjectType::Function);
        let signature = "greet(name)".to_string();
        let body_lines: Vec<&str> = vec![];
        let mut adornment_order = Vec::new();
        let mut diagnostics = Vec::new();

        // When
        let directive = parse_domain_object(
            object_type,
            signature,
            &body_lines,
            &mut adornment_order,
            &mut diagnostics,
            Domain::Py,
        );

        // Then
        if let Directive::DomainObject { body, .. } = directive {
            assert!(body.is_empty());
        } else {
            panic!("Expected DomainObject directive");
        }
    }

    #[test]
    fn test_parse_domain_object_strips_common_indentation() {
        // Given
        let object_type = ObjectType::Py(PyObjectType::Function);
        let signature = "greet(name)".to_string();
        let body_lines = vec!["     Indented more than needed."];
        let mut adornment_order = Vec::new();
        let mut diagnostics = Vec::new();

        // When
        let directive = parse_domain_object(
            object_type,
            signature,
            &body_lines,
            &mut adornment_order,
            &mut diagnostics,
            Domain::Py,
        );

        // Then
        if let Directive::DomainObject { body, .. } = directive {
            if let Node::Paragraph(inlines) = &body[0] {
                assert_eq!(
                    inlines[0],
                    crate::ast::InlineNode::Text("Indented more than needed.".to_string())
                );
            } else {
                panic!("Expected Paragraph, got {:?}", body[0]);
            }
        } else {
            panic!("Expected DomainObject directive");
        }
    }

    #[test]
    fn test_parse_creates_py_function_domain_object() {
        // Given
        let input = ".. py:function:: greet(name)\n\n   Greets the given name.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject {
            object_type,
            signature,
            body,
        }) = &doc.nodes[0]
        {
            assert_eq!(object_type, &ObjectType::Py(PyObjectType::Function));
            assert_eq!(signature, "greet(name)");
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected DomainObject directive, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_creates_c_function_domain_object() {
        // Given
        let input = ".. c:function:: int add(int a, int b)\n\n   Adds two numbers.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject {
            object_type,
            signature,
            ..
        }) = &doc.nodes[0]
        {
            assert_eq!(object_type, &ObjectType::C(CObjectType::Function));
            assert_eq!(signature, "int add(int a, int b)");
        } else {
            panic!("Expected DomainObject directive, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_bare_function_directive_resolves_via_default_domain() {
        // Given
        let input = ".. function:: greet(name)\n\n   Greets the given name.";

        // When
        let doc = crate::parser::parse_with_domain("test.rst", input, Domain::C);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::Directive(Directive::DomainObject { object_type, .. }) = &doc.nodes[0] {
            assert_eq!(object_type, &ObjectType::C(CObjectType::Function));
        } else {
            panic!("Expected DomainObject directive, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_unknown_domain_prefix_falls_through_to_unknown() {
        // Given
        let input = ".. rust:function:: greet(name)\n\n   Body.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        assert!(
            matches!(&doc.nodes[0], Node::Directive(Directive::Unknown { .. })),
            "Expected Unknown directive, got {:?}",
            doc.nodes[0]
        );
    }

    #[test]
    fn test_parse_unknown_object_type_in_known_domain_falls_through_to_unknown() {
        // Given
        let input = ".. py:class:: Greeter\n\n   Body.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        assert!(
            matches!(&doc.nodes[0], Node::Directive(Directive::Unknown { .. })),
            "Expected Unknown directive, got {:?}",
            doc.nodes[0]
        );
    }
}

//! Hyperlink target parsing: the named `.. _label: destination` form and the
//! anonymous `.. __: destination` form, either of which may carry its
//! destination on the following indented line instead of inline. A
//! destination is a URI, or another target's name (`.. _a: b_`).

use rinx_ast::{Node, TargetName};

use crate::context::ParseCtx;
use crate::inline::read_target_destination;

/// Parses the hyperlink target at `i`, if one starts there. A named target
/// records the span of its own line, which its label's definition is.
pub(super) fn try_parse_target(
    lines: &[&str],
    i: usize,
    ctx: &ParseCtx<'_>,
) -> Option<(usize, Node)> {
    let line = lines[i].trim();
    if let Some(rest) = line.strip_prefix(".. __:") {
        let mut uri = rest.trim().to_string();
        let mut consumed = 1;

        if uri.is_empty() && i + 1 < lines.len() {
            let next_line = lines[i + 1];
            if next_line.starts_with(' ') || next_line.starts_with('\t') {
                uri = next_line.trim().to_string();
                consumed = 2;
            }
        }

        if !uri.is_empty() {
            return Some((
                consumed,
                Node::AnonymousTarget {
                    destination: read_target_destination(&uri),
                },
            ));
        }
    }

    if !line.starts_with(".. _") {
        return None;
    }

    // Try to find the colon that ends the target name
    if let Some(colon_pos) = line[4..].find(':') {
        let absolute_colon_pos = 4 + colon_pos;
        let name_str = &line[4..absolute_colon_pos].trim();
        if name_str.is_empty() {
            return None;
        }

        let mut uri = line[absolute_colon_pos + 1..].trim().to_string();
        let mut consumed = 1;

        // If URI is empty on the same line, check the next line for an indented block
        if uri.is_empty() && i + 1 < lines.len() {
            let next_line = lines[i + 1];
            if next_line.starts_with(' ') || next_line.starts_with('\t') {
                uri = next_line.trim().to_string();
                consumed = 2;
            }
        }

        // An empty destination makes an internal target, labelling what
        // follows; anything else leads to a URI or to another target.
        let destination = (!uri.is_empty()).then(|| read_target_destination(&uri));

        return Some((
            consumed,
            Node::Target {
                name: TargetName::new(name_str),
                destination,
                span: ctx.line_span(i, lines[i].trim_end()),
            },
        ));
    }

    None
}

#[cfg(test)]
mod tests {
    use crate::parse;
    use rinx_ast::{Node, Position, Span};

    /// The span of every named target in `nodes`, in document order.
    fn target_spans(nodes: &[Node]) -> Vec<Option<Span>> {
        let mut spans = Vec::new();
        rinx_ast::walk_nodes(nodes, &mut |node| {
            if let Node::Target { span, .. } = node {
                spans.push(*span);
            }
        });
        spans
    }

    #[test]
    fn test_parse_records_an_internal_targets_line() {
        // When
        let doc = parse("test.rst", "Intro\n\n.. _install:\n\nText.\n");

        // Then
        assert_eq!(
            target_spans(&doc.nodes),
            vec![Some(Span::new(Position::new(3, 1), Position::new(3, 13)))]
        );
    }

    #[test]
    fn test_parse_records_a_target_with_its_destination_on_the_next_line_at_its_name() {
        // When
        let doc = parse("test.rst", ".. _docs:\n   https://example.org\n");

        // Then
        assert_eq!(
            target_spans(&doc.nodes),
            vec![Some(Span::new(Position::new(1, 1), Position::new(1, 10)))]
        );
    }

    #[test]
    fn test_parse_records_a_nested_targets_position_in_the_document() {
        // Given — a target inside a note's body, indented by three columns
        let input = ".. note::\n\n   .. _inside:\n\n   Text.\n";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(
            target_spans(&doc.nodes),
            vec![Some(Span::new(Position::new(3, 4), Position::new(3, 15)))]
        );
    }
}

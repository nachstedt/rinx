//! Hyperlink target parsing: the named `.. _label: destination` form and the
//! anonymous `.. __: destination` form, either of which may carry its
//! destination on the following indented line instead of inline. A
//! destination is a URI, or another target's name (`.. _a: b_`).

use rinx_ast::{Node, TargetName};

use crate::inline::read_target_destination;

pub(super) fn try_parse_target(lines: &[&str], i: usize) -> Option<(usize, Node)> {
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
            },
        ));
    }

    None
}

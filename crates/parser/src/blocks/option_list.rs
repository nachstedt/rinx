use std::sync::LazyLock;

use regex::Regex;

use crate::blocks::parse_blocks;
use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::headings::Adornment;
use crate::indent::{indent_width, strip_indent};
use rusty_sphinx_ast::{Node, OptionArgument, OptionArgumentDelimiter, OptionListItem, OptionSpec};

/// The docutils `option_marker` transition pattern (`parsers/rst/states.py`),
/// ported character-for-character from its `pats` fragments:
///
/// ```text
/// alphanum     = [a-zA-Z0-9]
/// alphanumplus = [a-zA-Z0-9_-]
/// optname      = alphanum alphanumplus*
/// optarg       = (alpha alphanumplus* | <[^<>]+>)
/// shortopt     = (-|\+) alphanum ( ?optarg)?
/// longopt      = (--|/) optname ([ =]optarg)?
/// option       = (shortopt|longopt)
/// option_marker = option(, option)*(  +| ?$)
/// ```
///
/// The final `(  +| ?$)` is *not* anchored to the end of the line on its
/// first branch — real docutils only needs to know *where* the marker ends,
/// not that it consumes the whole line, since arbitrary description text can
/// follow a `  +` gap. Only the second branch (no gap at all) requires the
/// match to reach the end of the line, which is what distinguishes "the
/// description starts right here" from "the description starts on the next
/// line".
static OPTION_MARKER_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    let optname = "[a-zA-Z0-9][a-zA-Z0-9_-]*";
    let optarg = "(?:[a-zA-Z][a-zA-Z0-9_-]*|<[^<>]+>)";
    let shortopt = format!(r"(?:-|\+)[a-zA-Z0-9](?: ?{optarg})?");
    let longopt = format!("(?:--|/){optname}(?:[ =]{optarg})?");
    let option = format!("(?:{shortopt}|{longopt})");
    Regex::new(&format!(
        "^(?P<marker>{option}(?:, {option})*)(?:(?P<gap> {{2,}})|(?: )?$)"
    ))
    .expect("option marker pattern is a fixed, tested regex")
});

/// Detects whether `line` opens an option-list item, returning the item's
/// indent, its parsed option synonyms, and where the description begins:
/// `Some(column)` when it starts on this same line (right after the `  +`
/// gap), or `None` when it starts entirely on the next line.
pub(super) fn detect_option_marker(line: &str) -> Option<(usize, Vec<OptionSpec>, Option<usize>)> {
    let item_indent = indent_width(line);
    let rest = strip_indent(line, item_indent);
    let caps = OPTION_MARKER_REGEX.captures(rest)?;
    let marker_text = caps.name("marker")?.as_str();
    let options = parse_option_specs(marker_text)?;

    let same_line_desc_col = caps.name("gap").map(|_| {
        let end_byte = caps.get(0).map_or(0, |m| m.end());
        item_indent + rest[..end_byte].chars().count()
    });

    Some((item_indent, options, same_line_desc_col))
}

/// Splits a matched option-marker's text into its comma-separated specs,
/// then parses each. Returns `None` if any spec fails to reduce to a
/// flag-plus-optional-argument pair — this should not happen for text that
/// already matched [`OPTION_MARKER_REGEX`], but falling through to "not an
/// option list" is safer than emitting a broken node.
fn parse_option_specs(marker_text: &str) -> Option<Vec<OptionSpec>> {
    split_option_specs(marker_text)
        .iter()
        .map(|spec| parse_one_option_spec(spec))
        .collect()
}

/// Splits on `", "`, except inside a `<...>` bracketed argument (which may
/// itself contain a comma or space) — mirrors docutils'
/// `re.split(r', (?![^<]*>)', ...)`, implemented by hand since the `regex`
/// crate has no lookahead.
fn split_option_specs(marker_text: &str) -> Vec<String> {
    let mut specs = Vec::new();
    let mut current = String::new();
    let mut depth: i32 = 0;
    let mut chars = marker_text.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '<' => {
                depth += 1;
                current.push(c);
            }
            '>' => {
                depth -= 1;
                current.push(c);
            }
            ',' if depth <= 0 && chars.peek() == Some(&' ') => {
                chars.next();
                specs.push(std::mem::take(&mut current));
            }
            _ => current.push(c),
        }
    }
    specs.push(current);
    specs
}

/// Parses one comma-separated spec (`"-o FILE"`, `"--output=FILE"`, `"-h"`,
/// ...) into an [`OptionSpec`], mirroring docutils' `parse_option_marker`
/// token-shuffling exactly: an `=` in the first token always means
/// equals-delimited; otherwise a short flag longer than two characters means
/// the adjacent form (`-oFILE`); a `<...>` argument that got split into
/// several whitespace tokens is rejoined into one.
fn parse_one_option_spec(spec: &str) -> Option<OptionSpec> {
    let mut tokens: Vec<String> = spec.split_whitespace().map(str::to_string).collect();
    if tokens.is_empty() {
        return None;
    }

    let mut delimiter = OptionArgumentDelimiter::Space;

    if let Some((flag, arg)) = tokens[0].split_once('=') {
        let (flag, arg) = (flag.to_string(), arg.to_string());
        tokens.splice(0..1, [flag, arg]);
        delimiter = OptionArgumentDelimiter::Equals;
    } else if tokens[0].len() > 2
        && ((tokens[0].starts_with('-') && !tokens[0].starts_with("--"))
            || tokens[0].starts_with('+'))
    {
        let (flag, arg) = tokens[0].split_at(2);
        let (flag, arg) = (flag.to_string(), arg.to_string());
        tokens.splice(0..1, [flag, arg]);
        delimiter = OptionArgumentDelimiter::Adjacent;
    }

    if tokens.len() > 1
        && tokens[1].starts_with('<')
        && tokens.last().is_some_and(|t| t.ends_with('>'))
    {
        let rejoined = tokens[1..].join(" ");
        tokens.truncate(1);
        tokens.push(rejoined);
    }

    match tokens.len() {
        1 => Some(OptionSpec {
            flag: tokens.into_iter().next()?,
            argument: None,
        }),
        2 => {
            let mut iter = tokens.into_iter();
            let flag = iter.next()?;
            let text = iter.next()?;
            Some(OptionSpec {
                flag,
                argument: Some(OptionArgument { text, delimiter }),
            })
        }
        _ => None,
    }
}

pub(super) fn try_parse_option_list(
    lines: &[&str],
    start_i: usize,
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<(usize, Node)> {
    let mut items = Vec::new();
    let mut i = start_i;
    let mut list_indent = None;

    while i < lines.len() {
        let line = lines[i].trim_end();
        if line.trim().is_empty() {
            break;
        }

        let Some((item_indent, options, same_line_desc_col)) = detect_option_marker(line) else {
            break;
        };

        match list_indent {
            None => list_indent = Some(item_indent),
            Some(indent) if indent == item_indent => {}
            Some(_) => break,
        }

        // Where the description's first line sits in the *original*
        // document, for position rebasing: the marker's own line for Case A
        // (content follows the gap on the same line), or the next line for
        // Case B (content starts on its own, more-indented, line) — mirrors
        // `bullet_list`'s `item_start` and `definition_list`'s
        // `definition_start` respectively.
        let description_start_line;
        let body_indent;
        let mut body_lines: Vec<String> = Vec::new();

        if let Some(col) = same_line_desc_col {
            description_start_line = i;
            body_indent = col;
            let first_line = if line.chars().count() > col {
                strip_indent(line, col).to_string()
            } else {
                String::new()
            };
            body_lines.push(first_line);
            i += 1;
        } else {
            let Some(next_line) = lines.get(i + 1) else {
                break;
            };
            let next_line_trimmed = next_line.trim_end();
            if next_line_trimmed.trim().is_empty() {
                break;
            }
            let next_indent = indent_width(next_line_trimmed);
            if next_indent <= item_indent {
                break;
            }
            description_start_line = i + 1;
            body_indent = next_indent;
            i += 1;
        }

        while i < lines.len() {
            let next_line = lines[i].trim_end();
            if next_line.trim().is_empty() {
                body_lines.push(String::new());
                i += 1;
                continue;
            }

            let next_indent = indent_width(next_line);
            if next_indent >= body_indent {
                body_lines.push(strip_indent(next_line, body_indent).to_string());
                i += 1;
            } else {
                break;
            }
        }

        while body_lines.last().is_some_and(String::is_empty) {
            body_lines.pop();
        }

        let body_refs: Vec<&str> = body_lines.iter().map(String::as_str).collect();
        let item_ctx = ctx.nested(description_start_line, body_indent);
        let description = parse_blocks(&body_refs, adornment_order, diagnostics, &item_ctx);

        items.push(OptionListItem {
            options,
            description,
        });
    }

    if items.is_empty() {
        None
    } else {
        Some((i - start_i, Node::OptionList { items }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use rusty_sphinx_ast::InlineNode;

    #[test]
    fn test_detect_option_marker_rejects_a_bullet_item() {
        // Given / When / Then — "- text" has a space right after the bullet
        // char, so it is not a valid short option (no alphanumeric follows).
        assert!(detect_option_marker("- text").is_none());
    }

    #[test]
    fn test_detect_option_marker_rejects_a_plain_paragraph() {
        // Given / When / Then
        assert!(detect_option_marker("Just a sentence.").is_none());
    }

    #[test]
    fn test_parse_option_list_single_short_flag_no_argument() {
        // Given
        let input = "-h  Show this help message.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::OptionList { items } = &doc.nodes[0] {
            assert_eq!(items.len(), 1);
            assert_eq!(items[0].options.len(), 1);
            assert_eq!(items[0].options[0].flag, "-h");
            assert!(items[0].options[0].argument.is_none());
            if let Node::Paragraph(inlines) = &items[0].description[0] {
                assert_eq!(
                    inlines[0],
                    InlineNode::Text("Show this help message.".to_string())
                );
            } else {
                panic!("Expected Paragraph, got {:?}", items[0].description[0]);
            }
        } else {
            panic!("Expected OptionList, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_option_list_multiple_synonyms_comma_separated() {
        // Given
        let input = "-v, --verbose  Increase verbosity.";

        // When
        let doc = parse("test.rst", input);

        // Then
        if let Node::OptionList { items } = &doc.nodes[0] {
            assert_eq!(items[0].options.len(), 2);
            assert_eq!(items[0].options[0].flag, "-v");
            assert_eq!(items[0].options[1].flag, "--verbose");
        } else {
            panic!("Expected OptionList, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_option_list_space_delimited_argument() {
        // Given
        let input = "-o FILE  Write output to FILE.";

        // When
        let doc = parse("test.rst", input);

        // Then
        if let Node::OptionList { items } = &doc.nodes[0] {
            let arg = items[0].options[0]
                .argument
                .as_ref()
                .expect("expected an argument");
            assert_eq!(arg.text, "FILE");
            assert_eq!(arg.delimiter, OptionArgumentDelimiter::Space);
        } else {
            panic!("Expected OptionList, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_option_list_equals_delimited_argument() {
        // Given
        let input = "--output=FILE  Write output to FILE.";

        // When
        let doc = parse("test.rst", input);

        // Then
        if let Node::OptionList { items } = &doc.nodes[0] {
            let arg = items[0].options[0]
                .argument
                .as_ref()
                .expect("expected an argument");
            assert_eq!(items[0].options[0].flag, "--output");
            assert_eq!(arg.text, "FILE");
            assert_eq!(arg.delimiter, OptionArgumentDelimiter::Equals);
        } else {
            panic!("Expected OptionList, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_option_list_adjacent_short_argument() {
        // Given
        let input = "-oFILE  Write output to FILE.";

        // When
        let doc = parse("test.rst", input);

        // Then
        if let Node::OptionList { items } = &doc.nodes[0] {
            assert_eq!(items[0].options[0].flag, "-o");
            let arg = items[0].options[0]
                .argument
                .as_ref()
                .expect("expected an argument");
            assert_eq!(arg.text, "FILE");
            assert_eq!(arg.delimiter, OptionArgumentDelimiter::Adjacent);
        } else {
            panic!("Expected OptionList, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_option_list_bracketed_argument_with_embedded_space() {
        // Given
        let input = "-o <value1 value2>  A bracketed placeholder.";

        // When
        let doc = parse("test.rst", input);

        // Then
        if let Node::OptionList { items } = &doc.nodes[0] {
            let arg = items[0].options[0]
                .argument
                .as_ref()
                .expect("expected an argument");
            assert_eq!(arg.text, "<value1 value2>");
        } else {
            panic!("Expected OptionList, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_option_list_dos_and_plus_markers() {
        // Given
        let input = "/Wall  Enable all warnings.\n+x  Old GNU-style option.";

        // When
        let doc = parse("test.rst", input);

        // Then
        if let Node::OptionList { items } = &doc.nodes[0] {
            assert_eq!(items.len(), 2);
            assert_eq!(items[0].options[0].flag, "/Wall");
            assert_eq!(items[1].options[0].flag, "+x");
        } else {
            panic!("Expected OptionList, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_option_list_description_starts_on_next_line() {
        // Given — no text follows the marker on its own line.
        let input = "--long\n    A long option whose description starts below.";

        // When
        let doc = parse("test.rst", input);

        // Then
        if let Node::OptionList { items } = &doc.nodes[0] {
            assert_eq!(items.len(), 1);
            if let Node::Paragraph(inlines) = &items[0].description[0] {
                assert_eq!(
                    inlines[0],
                    InlineNode::Text("A long option whose description starts below.".to_string())
                );
            } else {
                panic!("Expected Paragraph, got {:?}", items[0].description[0]);
            }
        } else {
            panic!("Expected OptionList, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_option_list_multi_line_description_continuation() {
        // Given — a same-line description that wraps onto a further,
        // aligned line.
        let input = "-o FILE  Write output to FILE,\n         overwriting any existing file.";

        // When
        let doc = parse("test.rst", input);

        // Then
        if let Node::OptionList { items } = &doc.nodes[0] {
            assert_eq!(items.len(), 1);
            if let Node::Paragraph(inlines) = &items[0].description[0] {
                assert_eq!(
                    inlines[0],
                    InlineNode::Text(
                        "Write output to FILE,\noverwriting any existing file.".to_string()
                    )
                );
            } else {
                panic!("Expected Paragraph, got {:?}", items[0].description[0]);
            }
        } else {
            panic!("Expected OptionList, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_option_list_multi_paragraph_description() {
        // Given
        let input = "--long  Para 1.\n\n        Para 2.";

        // When
        let doc = parse("test.rst", input);

        // Then
        if let Node::OptionList { items } = &doc.nodes[0] {
            assert_eq!(items.len(), 1);
            assert_eq!(items[0].description.len(), 2);
            assert!(matches!(items[0].description[0], Node::Paragraph(_)));
            assert!(matches!(items[0].description[1], Node::Paragraph(_)));
        } else {
            panic!("Expected OptionList, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_option_list_consecutive_items_without_blank_line() {
        // Given — blank lines between entries are optional.
        let input = "-h  Show help.\n-v  Show version.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        if let Node::OptionList { items } = &doc.nodes[0] {
            assert_eq!(items.len(), 2);
            assert_eq!(items[0].options[0].flag, "-h");
            assert_eq!(items[1].options[0].flag, "-v");
        } else {
            panic!("Expected OptionList, got {:?}", doc.nodes[0]);
        }
    }

    #[test]
    fn test_parse_option_list_ends_before_dedented_paragraph() {
        // Given
        let input = "-h  Show help.\n\nNot part of the list.";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 2);
        assert!(matches!(doc.nodes[0], Node::OptionList { .. }));
        assert!(matches!(doc.nodes[1], Node::Paragraph(_)));
    }

    #[test]
    fn test_parse_option_list_does_not_fire_on_plain_paragraph() {
        // Given — two ordinary lines, neither shaped like an option marker.
        let input = "Line one\nLine two";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        assert!(matches!(doc.nodes[0], Node::Paragraph(_)));
    }

    #[test]
    fn test_parse_option_list_does_not_misdetect_a_real_bullet_item() {
        // Given — a genuine bullet list, which must still parse as one.
        let input = "- Item 1\n- Item 2";

        // When
        let doc = parse("test.rst", input);

        // Then
        assert_eq!(doc.nodes.len(), 1);
        assert!(matches!(doc.nodes[0], Node::BulletList { .. }));
    }
}

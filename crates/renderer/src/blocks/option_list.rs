//! Renders RST option lists (`-h, --help  Show this help message.`) as
//! `<dl class="option-list">`, matching the markup the modern (HTML5)
//! docutils/Sphinx writer produces — not the legacy `html4css1` writer's
//! `<table class="option-list">`.

use std::fmt::Write as _;

use rusty_sphinx_ast::{OptionArgumentDelimiter, OptionListItem, OptionSpec};

use crate::RenderCtx;

pub(super) fn render_option_list(
    html: &mut String,
    items: &[OptionListItem],
    ctx: &mut RenderCtx<'_>,
) {
    let _ = writeln!(html, "<dl class=\"option-list\">");
    for item in items {
        let _ = write!(html, "<dt><kbd>");
        for (index, option) in item.options.iter().enumerate() {
            if index > 0 {
                html.push_str(", ");
            }
            render_option_spec(html, option);
        }
        let _ = writeln!(html, "</kbd></dt>");
        let _ = write!(html, "<dd>");
        super::render_nodes(html, &item.description, ctx);
        let _ = writeln!(html, "</dd>");
    }
    let _ = writeln!(html, "</dl>");
}

/// Renders one option synonym as `<span class="option">flag</span>`, with
/// the argument (if any) joined by its recorded delimiter and wrapped in
/// `<var>`, e.g. `<span class="option">-o <var>FILE</var></span>` or
/// `<span class="option">--output=<var>FILE</var></span>`.
fn render_option_spec(html: &mut String, option: &OptionSpec) {
    let _ = write!(
        html,
        "<span class=\"option\">{}",
        html_escape::encode_text(&option.flag)
    );
    if let Some(argument) = &option.argument {
        let delimiter = match argument.delimiter {
            OptionArgumentDelimiter::Space => " ",
            OptionArgumentDelimiter::Equals => "=",
            OptionArgumentDelimiter::Adjacent => "",
        };
        let _ = write!(
            html,
            "{delimiter}<var>{}</var>",
            html_escape::encode_text(&argument.text)
        );
    }
    html.push_str("</span>");
}

#[cfg(test)]
mod tests {
    use rusty_sphinx_ast::{Document, InlineNode, Node, OptionArgument};
    use rusty_sphinx_index::ProjectIndex;

    use super::*;

    fn render_doc(doc: &Document) -> String {
        let index = ProjectIndex::default();
        crate::render(doc, &index, &doc.path).html
    }

    fn option(flag: &str, argument: Option<OptionArgument>) -> OptionSpec {
        OptionSpec {
            flag: flag.to_string(),
            argument,
        }
    }

    #[test]
    fn test_render_option_list_single_flag_no_argument() {
        // Given
        let doc = Document::new(
            "guide.rst".to_string(),
            vec![Node::OptionList {
                items: vec![OptionListItem {
                    options: vec![option("-h", None)],
                    description: vec![Node::Paragraph(vec![InlineNode::Text(
                        "Show this help message.".to_string(),
                    )])],
                }],
            }],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<dl class=\"option-list\">"));
        assert!(result.contains("<dt><kbd><span class=\"option\">-h</span></kbd></dt>"));
        assert!(result.contains("<dd><p>Show this help message.</p>\n</dd>"));
        assert!(result.contains("</dl>"));
    }

    #[test]
    fn test_render_option_list_multiple_synonyms_are_comma_separated() {
        // Given
        let doc = Document::new(
            "guide.rst".to_string(),
            vec![Node::OptionList {
                items: vec![OptionListItem {
                    options: vec![option("-v", None), option("--verbose", None)],
                    description: vec![],
                }],
            }],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains(
            "<kbd><span class=\"option\">-v</span>, <span class=\"option\">--verbose</span></kbd>"
        ));
    }

    #[test]
    fn test_render_option_list_space_delimited_argument() {
        // Given
        let doc = Document::new(
            "guide.rst".to_string(),
            vec![Node::OptionList {
                items: vec![OptionListItem {
                    options: vec![option(
                        "-o",
                        Some(OptionArgument {
                            text: "FILE".to_string(),
                            delimiter: OptionArgumentDelimiter::Space,
                        }),
                    )],
                    description: vec![],
                }],
            }],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<span class=\"option\">-o <var>FILE</var></span>"));
    }

    #[test]
    fn test_render_option_list_equals_delimited_argument() {
        // Given
        let doc = Document::new(
            "guide.rst".to_string(),
            vec![Node::OptionList {
                items: vec![OptionListItem {
                    options: vec![option(
                        "--output",
                        Some(OptionArgument {
                            text: "FILE".to_string(),
                            delimiter: OptionArgumentDelimiter::Equals,
                        }),
                    )],
                    description: vec![],
                }],
            }],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<span class=\"option\">--output=<var>FILE</var></span>"));
    }

    #[test]
    fn test_render_option_list_adjacent_delimited_argument() {
        // Given
        let doc = Document::new(
            "guide.rst".to_string(),
            vec![Node::OptionList {
                items: vec![OptionListItem {
                    options: vec![option(
                        "-o",
                        Some(OptionArgument {
                            text: "FILE".to_string(),
                            delimiter: OptionArgumentDelimiter::Adjacent,
                        }),
                    )],
                    description: vec![],
                }],
            }],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("<span class=\"option\">-o<var>FILE</var></span>"));
    }

    #[test]
    fn test_render_option_list_escapes_html_in_flag_and_argument() {
        // Given
        let doc = Document::new(
            "guide.rst".to_string(),
            vec![Node::OptionList {
                items: vec![OptionListItem {
                    options: vec![option(
                        "--a<b",
                        Some(OptionArgument {
                            text: "<c>".to_string(),
                            delimiter: OptionArgumentDelimiter::Space,
                        }),
                    )],
                    description: vec![],
                }],
            }],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert!(result.contains("--a&lt;b"));
        assert!(result.contains("&lt;c&gt;"));
        assert!(!result.contains("--a<b"));
    }

    #[test]
    fn test_render_option_list_multiple_items_each_get_their_own_dt_dd_pair() {
        // Given
        let doc = Document::new(
            "guide.rst".to_string(),
            vec![Node::OptionList {
                items: vec![
                    OptionListItem {
                        options: vec![option("-h", None)],
                        description: vec![],
                    },
                    OptionListItem {
                        options: vec![option("-v", None)],
                        description: vec![],
                    },
                ],
            }],
        );

        // When
        let result = render_doc(&doc);

        // Then
        assert_eq!(result.matches("<dt>").count(), 2);
        assert_eq!(result.matches("<dd>").count(), 2);
    }
}

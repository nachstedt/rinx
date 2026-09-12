//! `.. dropdown::` rendering — sphinx-design's collapsible card.
//!
//! The element this writes is `DropdownHtmlTransform`'s output, class for
//! class, so a site already styled for sphinx-design keeps its appearance:
//!
//! ```html
//! <details class="sd-sphinx-override sd-dropdown sd-card sd-mb-3">
//!   <summary class="sd-summary-title sd-card-header">
//!     <span class="sd-summary-icon">…</span>
//!     <span class="sd-summary-text">…title…</span>
//!     <span class="sd-summary-state-marker sd-summary-chevron-right">…</span>
//!   </summary>
//!   <div class="sd-summary-content sd-card-body">…body…</div>
//! </details>
//! ```
//!
//! Two details are easy to get wrong and are therefore spelled out here. A
//! dropdown with no argument does not render an empty summary: sphinx-design
//! draws a `kebab-horizontal` octicon in place of the title, which is what
//! makes a titleless dropdown clickable at all. And the body stamps
//! `sd-card-text` on its *direct child* paragraphs only — see
//! [`super::dispatch::render_nodes_with_paragraph_class`].

use std::fmt::Write as _;

use rusty_sphinx_ast::{Dropdown, SpacingKind};

use crate::RenderCtx;
use crate::inline::render_inline;
use crate::octicon::{ICON_HEIGHT_EM, MARKER_HEIGHT_EM, render_octicon};

use super::dispatch::render_nodes_with_paragraph_class;

/// Renders a `.. dropdown::` directive.
pub(super) fn render_dropdown(html: &mut String, dropdown: &Dropdown, ctx: &mut RenderCtx<'_>) {
    let open_attr = if dropdown.open { " open=\"open\"" } else { "" };
    let id_attr = dropdown.name.as_ref().map_or_else(String::new, |name| {
        format!(
            " id=\"{}\"",
            html_escape::encode_double_quoted_attribute(name.as_str())
        )
    });

    let _ = writeln!(
        html,
        "<details class=\"{}\"{id_attr}{open_attr}>",
        class_attribute(&container_classes(dropdown))
    );
    render_summary(html, dropdown, ctx);
    let _ = writeln!(
        html,
        "<div class=\"{}\">",
        class_attribute(&body_classes(dropdown))
    );
    render_nodes_with_paragraph_class(html, &dropdown.body, Some("sd-card-text"), ctx);
    let _ = writeln!(html, "</div>");
    let _ = writeln!(html, "</details>");
}

/// Renders the `<summary>`: the optional icon, the title, and the state marker.
fn render_summary(html: &mut String, dropdown: &Dropdown, ctx: &mut RenderCtx<'_>) {
    let _ = writeln!(
        html,
        "<summary class=\"{}\">",
        class_attribute(&title_classes(dropdown))
    );

    if let Some(icon) = &dropdown.icon
        && let Some(svg) = render_octicon(icon.as_str(), ICON_HEIGHT_EM, &[])
    {
        let _ = write!(html, "<span class=\"sd-summary-icon\">{svg}</span>");
    }

    let _ = write!(html, "<span class=\"sd-summary-text\">");
    if dropdown.has_title() {
        for inline in &dropdown.title {
            render_inline(html, inline, ctx);
        }
    } else if let Some(svg) = render_octicon("kebab-horizontal", MARKER_HEIGHT_EM, &["no-title"]) {
        // No argument was written. sphinx-design draws a placeholder rather
        // than an empty bar, so the dropdown still reads as something to open.
        let _ = write!(html, "{svg}");
    }
    let _ = writeln!(html, "</span>");

    let marker = dropdown.chevron.marker_class_suffix();
    let _ = write!(
        html,
        "<span class=\"sd-summary-state-marker sd-summary-{marker}\">"
    );
    if let Some(svg) = render_octicon(dropdown.chevron.octicon(), MARKER_HEIGHT_EM, &[]) {
        let _ = write!(html, "{svg}");
    }
    let _ = writeln!(html, "</span>");

    let _ = writeln!(html, "</summary>");
}

/// The `<details>` classes: the fixed three, then the margin, the author's
/// own container classes, and the animation — in sphinx-design's order.
fn container_classes(dropdown: &Dropdown) -> Vec<String> {
    let mut classes = vec![
        "sd-sphinx-override".to_string(),
        "sd-dropdown".to_string(),
        "sd-card".to_string(),
    ];
    match dropdown.margin {
        // An omitted `:margin:` is not "no margin": sphinx-design defaults the
        // container to a bottom margin so consecutive dropdowns do not touch.
        None => classes.extend(
            Dropdown::DEFAULT_MARGIN_CLASSES
                .iter()
                .map(|&class| class.to_string()),
        ),
        Some(margin) => classes.extend(margin.css_classes(SpacingKind::Margin)),
    }
    classes.extend(dropdown.class_container.iter().cloned());
    if let Some(animation) = dropdown.animate {
        let class = animation.css_class().to_string();
        // sphinx-design appends the animation only if the container does not
        // already carry it, so writing it by hand in `:class-container:` does
        // not duplicate it.
        if !classes.contains(&class) {
            classes.push(class);
        }
    }
    classes
}

/// The `<summary>` classes: the fixed two, the author's own title classes,
/// then the colour pair.
fn title_classes(dropdown: &Dropdown) -> Vec<String> {
    let mut classes = vec!["sd-summary-title".to_string(), "sd-card-header".to_string()];
    classes.extend(dropdown.class_title.iter().cloned());
    if let Some(color) = dropdown.color {
        classes.push(format!("sd-bg-{color}"));
        classes.push(format!("sd-bg-text-{color}"));
    }
    classes
}

/// The content `<div>`'s classes: the fixed two, then the author's own.
fn body_classes(dropdown: &Dropdown) -> Vec<String> {
    let mut classes = vec!["sd-summary-content".to_string(), "sd-card-body".to_string()];
    classes.extend(dropdown.class_body.iter().cloned());
    classes
}

/// Joins classes into an escaped `class` attribute value.
fn class_attribute(classes: &[String]) -> String {
    html_escape::encode_double_quoted_attribute(&classes.join(" ")).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::render_test_support::render_directive_html;
    use rusty_sphinx_ast::{
        Animation, Chevron, Directive, InlineNode, Node, OcticonName, SemanticColor, Spacing,
        SpacingValue, TargetName,
    };
    use rusty_sphinx_index::ProjectIndex;

    fn dropdown_html(dropdown: Dropdown) -> String {
        render_directive_html(
            &Directive::Dropdown(Box::new(dropdown)),
            &ProjectIndex::default(),
            "index",
        )
    }

    fn titled(title: &str) -> Dropdown {
        Dropdown {
            title: vec![InlineNode::Text(title.to_string())],
            ..Dropdown::new()
        }
    }

    #[test]
    fn test_renders_the_sphinx_design_element_structure() {
        // Given
        let dropdown = titled("Details");

        // When
        let html = dropdown_html(dropdown);

        // Then
        assert!(
            html.contains("<details class=\"sd-sphinx-override sd-dropdown sd-card sd-mb-3\">")
        );
        assert!(html.contains("<summary class=\"sd-summary-title sd-card-header\">"));
        assert!(html.contains("<span class=\"sd-summary-text\">Details</span>"));
        assert!(html.contains("<div class=\"sd-summary-content sd-card-body\">"));
        assert!(html.contains("</details>"));
    }

    #[test]
    fn test_renders_the_title_as_inline_markup() {
        // Given — a literal in the argument, which an admonition's plain-text
        // title could not carry
        let dropdown = Dropdown {
            title: vec![
                InlineNode::Text("See ".to_string()),
                InlineNode::Literal("config.toml".to_string()),
            ],
            ..Dropdown::new()
        };

        // When
        let html = dropdown_html(dropdown);

        // Then
        assert!(html.contains("<span class=\"sd-summary-text\">See <code"));
        assert!(html.contains("config.toml"));
    }

    #[test]
    fn test_a_closed_dropdown_has_no_open_attribute() {
        // Given
        let dropdown = titled("Details");

        // When
        let html = dropdown_html(dropdown);

        // Then
        assert!(!html.contains("open=\"open\""));
    }

    #[test]
    fn test_open_renders_the_open_attribute() {
        // Given
        let dropdown = Dropdown {
            open: true,
            ..titled("Details")
        };

        // When
        let html = dropdown_html(dropdown);

        // Then
        assert!(html.contains("open=\"open\">"));
    }

    #[test]
    fn test_color_adds_both_background_classes_to_the_summary() {
        // Given
        let dropdown = Dropdown {
            color: Some(SemanticColor::Success),
            ..titled("Details")
        };

        // When
        let html = dropdown_html(dropdown);

        // Then
        assert!(html.contains(
            "<summary class=\"sd-summary-title sd-card-header sd-bg-success sd-bg-text-success\">"
        ));
    }

    #[test]
    fn test_icon_renders_an_octicon_before_the_title() {
        // Given
        let dropdown = Dropdown {
            icon: Some(OcticonName::new("light-bulb").expect("a slug is valid")),
            ..titled("Details")
        };

        // When
        let html = dropdown_html(dropdown);

        // Then
        assert!(html.contains("<span class=\"sd-summary-icon\"><svg"));
        assert!(html.contains("sd-octicon-light-bulb"));
        let icon_at = html.find("sd-summary-icon").expect("the icon was rendered");
        let title_at = html
            .find("sd-summary-text")
            .expect("the title was rendered");
        assert!(icon_at < title_at, "the icon precedes the title");
    }

    #[test]
    fn test_an_unknown_icon_renders_no_icon_rather_than_a_broken_one() {
        // Given — the parser reports this; the renderer must not emit an empty
        // element for it
        let dropdown = Dropdown {
            icon: Some(OcticonName::new("not-an-octicon").expect("a slug is valid")),
            ..titled("Details")
        };

        // When
        let html = dropdown_html(dropdown);

        // Then
        assert!(!html.contains("sd-summary-icon"));
    }

    #[test]
    fn test_the_state_marker_defaults_to_the_right_chevron() {
        // Given
        let dropdown = titled("Details");

        // When
        let html = dropdown_html(dropdown);

        // Then
        assert!(
            html.contains("<span class=\"sd-summary-state-marker sd-summary-chevron-right\"><svg")
        );
        assert!(html.contains("sd-octicon-chevron-right"));
    }

    #[test]
    fn test_down_up_draws_the_down_chevron() {
        // Given
        let dropdown = Dropdown {
            chevron: Chevron::DownUp,
            ..titled("Details")
        };

        // When
        let html = dropdown_html(dropdown);

        // Then
        assert!(html.contains("sd-summary-chevron-down"));
        assert!(html.contains("sd-octicon-chevron-down"));
    }

    #[test]
    fn test_a_titleless_dropdown_draws_the_placeholder_icon() {
        // Given
        let dropdown = Dropdown::new();

        // When
        let html = dropdown_html(dropdown);

        // Then — a placeholder, not an empty summary
        assert!(html.contains("sd-octicon-kebab-horizontal no-title"));
        assert!(!html.contains("<span class=\"sd-summary-text\"></span>"));
    }

    #[test]
    fn test_animate_adds_its_class_to_the_container() {
        // Given
        let dropdown = Dropdown {
            animate: Some(Animation::FadeInSlideDown),
            ..titled("Details")
        };

        // When
        let html = dropdown_html(dropdown);

        // Then
        assert!(html.contains("sd-fade-in-slide-down"));
    }

    #[test]
    fn test_animate_is_not_repeated_when_written_by_hand() {
        // Given
        let dropdown = Dropdown {
            animate: Some(Animation::FadeIn),
            class_container: vec!["sd-fade-in".to_string()],
            ..titled("Details")
        };

        // When
        let html = dropdown_html(dropdown);

        // Then
        assert_eq!(html.matches("sd-fade-in").count(), 1);
    }

    #[test]
    fn test_margin_replaces_the_default_bottom_margin() {
        // Given
        let dropdown = Dropdown {
            margin: Some(Spacing::All(SpacingValue::Auto)),
            ..titled("Details")
        };

        // When
        let html = dropdown_html(dropdown);

        // Then
        assert!(html.contains("sd-m-auto"));
        assert!(!html.contains("sd-mb-3"));
    }

    #[test]
    fn test_the_three_class_options_land_on_their_own_elements() {
        // Given
        let dropdown = Dropdown {
            class_container: vec!["mine-container".to_string()],
            class_title: vec!["mine-title".to_string()],
            class_body: vec!["mine-body".to_string()],
            ..titled("Details")
        };

        // When
        let html = dropdown_html(dropdown);

        // Then
        assert!(html.contains("sd-card sd-mb-3 mine-container\""));
        assert!(html.contains("sd-card-header mine-title\""));
        assert!(html.contains("sd-card-body mine-body\""));
    }

    #[test]
    fn test_name_becomes_the_details_element_id() {
        // Given
        let dropdown = Dropdown {
            name: Some(TargetName::new("My Dropdown")),
            ..titled("Details")
        };

        // When
        let html = dropdown_html(dropdown);

        // Then — the anchor a `:ref:` lands on
        assert!(html.contains("id=\"my dropdown\""));
    }

    #[test]
    fn test_direct_child_paragraphs_are_card_text() {
        // Given
        let dropdown = Dropdown {
            body: vec![Node::Paragraph(vec![InlineNode::Text("Body".to_string())])],
            ..titled("Details")
        };

        // When
        let html = dropdown_html(dropdown);

        // Then
        assert!(html.contains("<p class=\"sd-card-text\">Body</p>"));
    }

    #[test]
    fn test_a_paragraph_nested_deeper_is_not_card_text() {
        // Given — a paragraph inside a list item, one level below the body
        let dropdown = Dropdown {
            body: vec![Node::BulletList {
                bullet: '*',
                items: vec![rusty_sphinx_ast::ListItem {
                    nodes: vec![Node::Paragraph(vec![InlineNode::Text("Item".to_string())])],
                }],
            }],
            ..titled("Details")
        };

        // When
        let html = dropdown_html(dropdown);

        // Then — sphinx-design stamps direct children only
        assert!(html.contains("Item"));
        assert!(!html.contains("<p class=\"sd-card-text\">Item</p>"));
    }

    #[test]
    fn test_container_classes_order_matches_sphinx_design() {
        // Given
        let dropdown = Dropdown {
            margin: Some(Spacing::All(SpacingValue::Two)),
            class_container: vec!["mine".to_string()],
            animate: Some(Animation::FadeIn),
            ..titled("Details")
        };

        // When
        let classes = container_classes(&dropdown);

        // Then
        assert_eq!(
            classes,
            vec![
                "sd-sphinx-override",
                "sd-dropdown",
                "sd-card",
                "sd-m-2",
                "mine",
                "sd-fade-in",
            ]
        );
    }

    #[test]
    fn test_title_classes_put_the_colour_after_the_authors_own() {
        // Given
        let dropdown = Dropdown {
            class_title: vec!["mine".to_string()],
            color: Some(SemanticColor::Danger),
            ..titled("Details")
        };

        // When
        let classes = title_classes(&dropdown);

        // Then
        assert_eq!(
            classes,
            vec![
                "sd-summary-title",
                "sd-card-header",
                "mine",
                "sd-bg-danger",
                "sd-bg-text-danger",
            ]
        );
    }

    #[test]
    fn test_body_classes_are_the_card_body_plus_the_authors_own() {
        // Given
        let dropdown = Dropdown {
            class_body: vec!["mine".to_string()],
            ..titled("Details")
        };

        // When
        let classes = body_classes(&dropdown);

        // Then
        assert_eq!(classes, vec!["sd-summary-content", "sd-card-body", "mine"]);
    }

    #[test]
    fn test_class_attribute_escapes_a_quote() {
        // Given — a class list an author could write into `:class-body:`
        let classes = vec!["a\"b".to_string()];

        // When
        let attribute = class_attribute(&classes);

        // Then
        assert_eq!(attribute, "a&quot;b");
    }
}

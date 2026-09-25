use rinx_ast::{
    ButtonFlag, ButtonLink, ButtonTarget, Directive, InlineNode, SemanticColor, TextAlign,
};
use rinx_index::{ProjectIndex, TargetLocation};

use crate::blocks::render_test_support::render_directive_html;

/// A button pointing at one URL, which every test below starts from.
fn button() -> ButtonLink {
    ButtonLink::new(ButtonTarget::Url("https://example.com".to_string()))
}

/// Renders a button against an empty project index.
fn render(button: &ButtonLink) -> String {
    render_directive_html(
        &Directive::ButtonLink(Box::new(button.clone())),
        &ProjectIndex::default(),
        "test.rst",
    )
}

#[test]
fn test_renders_the_element_structure_sphinx_design_builds() {
    // Given
    let mut link = button();
    link.label = vec![InlineNode::Text("Read the docs".to_string())];

    // When
    let html = render(&link);

    // Then
    assert!(
        html.contains(
            "<a class=\"sd-sphinx-override sd-btn sd-text-wrap\" \
             href=\"https://example.com\"><span>Read the docs</span></a>"
        ),
        "{html}"
    );
    assert!(html.contains("<p>"), "{html}");
}

#[test]
fn test_a_button_with_no_label_shows_its_target() {
    // Given
    let link = button();

    // When
    let html = render(&link);

    // Then
    assert!(html.contains("<span>https://example.com</span>"), "{html}");
}

#[test]
fn test_a_color_paints_the_button() {
    // Given
    let mut link = button();
    link.color = Some(SemanticColor::Primary);

    // When
    let html = render(&link);

    // Then
    assert!(
        html.contains("sd-btn sd-text-wrap sd-btn-primary"),
        "{html}"
    );
}

#[test]
fn test_outline_with_a_color_draws_the_outline_variant() {
    // Given
    let mut link = button();
    link.color = Some(SemanticColor::Danger);
    link.flags.insert(ButtonFlag::Outline);

    // When
    let html = render(&link);

    // Then
    assert!(html.contains("sd-btn-outline-danger"), "{html}");
    assert!(!html.contains("sd-btn-danger\""), "{html}");
}

#[test]
fn test_outline_without_a_color_adds_no_class() {
    // Given — sphinx-design names no colour to outline, so it adds nothing;
    // the parser has already reported the pair
    let mut link = button();
    link.flags.insert(ButtonFlag::Outline);

    // When
    let html = render(&link);

    // Then
    assert!(!html.contains("sd-btn-outline"), "{html}");
    assert!(
        html.contains("class=\"sd-sphinx-override sd-btn sd-text-wrap\""),
        "{html}"
    );
}

#[test]
fn test_click_parent_and_shadow_each_add_their_class_in_order() {
    // Given
    let mut link = button();
    link.color = Some(SemanticColor::Success);
    link.flags.insert(ButtonFlag::ClickParent);
    link.flags.insert(ButtonFlag::Shadow);
    link.class = vec!["my-button".to_string()];

    // When
    let html = render(&link);

    // Then — sphinx-design's own order: colour, click-parent, shadow, author's
    assert!(
        html.contains(
            "sd-sphinx-override sd-btn sd-text-wrap sd-btn-success \
             sd-stretched-link sd-shadow-sm my-button"
        ),
        "{html}"
    );
}

#[test]
fn test_align_lands_on_the_containing_paragraph() {
    // Given
    let mut link = button();
    link.align = Some(TextAlign::Center);

    // When
    let html = render(&link);

    // Then
    assert!(html.contains("<p class=\"sd-text-center\">"), "{html}");
    assert!(!html.contains("sd-text-center\" href"), "{html}");
}

#[test]
fn test_expand_wraps_the_button_in_a_grid_span() {
    // Given
    let mut link = button();
    link.flags.insert(ButtonFlag::Expand);

    // When
    let html = render(&link);

    // Then
    assert!(html.contains("<span class=\"sd-d-grid\"><a "), "{html}");
    assert!(html.contains("</a></span>"), "{html}");
}

#[test]
fn test_a_tooltip_becomes_a_title_attribute() {
    // Given
    let mut link = button();
    link.tooltip = Some("Opens the online editor".to_string());

    // When
    let html = render(&link);

    // Then
    assert!(
        html.contains(" title=\"Opens the online editor\">"),
        "{html}"
    );
}

#[test]
fn test_markup_in_the_label_is_rendered() {
    // Given
    let mut link = button();
    link.label = vec![
        InlineNode::Text("Read ".to_string()),
        InlineNode::Literal("conf.py".to_string()),
    ];

    // When
    let html = render(&link);

    // Then
    assert!(html.contains("Read <code"), "{html}");
    assert!(html.contains("conf.py"), "{html}");
}

#[test]
fn test_a_reference_in_the_label_is_flattened_to_its_text() {
    // Given — a resolvable target, so the flattening is this renderer's doing
    // and not a broken link
    let mut index = ProjectIndex::default();
    index.targets.insert(
        rinx_ast::TargetName::new("the-guide"),
        TargetLocation::Internal("guide.rst".to_string()),
    );
    let mut link = button();
    link.label = vec![
        InlineNode::Text("See ".to_string()),
        InlineNode::Reference {
            display: Some("the guide".to_string()),
            target: "the-guide".to_string(),
            span: None,
            inventory: rinx_ast::InventorySelector::Any,
        },
    ];

    // When
    let html = render_directive_html(&Directive::ButtonLink(Box::new(link)), &index, "test.rst");

    // Then — one `<a>`, the button's own; the reference is its text alone
    assert_eq!(html.matches("<a ").count(), 1, "{html}");
    assert!(html.contains("<span>See the guide</span>"), "{html}");
}

#[test]
fn test_a_url_and_a_label_are_escaped() {
    // Given
    let mut link = ButtonLink::new(ButtonTarget::Url(
        "https://example.com/?a=1&b=\"2\"".to_string(),
    ));
    link.label = vec![InlineNode::Text("Fish & <chips>".to_string())];
    link.tooltip = Some("a \"quoted\" tip".to_string());

    // When
    let html = render(&link);

    // Then
    assert!(
        html.contains("href=\"https://example.com/?a=1&amp;b=&quot;2&quot;\""),
        "{html}"
    );
    assert!(
        html.contains("<span>Fish &amp; &lt;chips&gt;</span>"),
        "{html}"
    );
    assert!(
        html.contains("title=\"a &quot;quoted&quot; tip\""),
        "{html}"
    );
}

#[test]
fn test_the_corpus_shape_renders_a_shadowed_primary_button() {
    // Given — the shape every `.. button-link::` in the benchmark corpus has
    let mut link = ButtonLink::new(ButtonTarget::Url(
        "https://gitpod.io/#https://github.com/useblocks/demo".to_string(),
    ));
    link.color = Some(SemanticColor::Primary);
    link.flags.insert(ButtonFlag::Shadow);
    link.label = vec![InlineNode::Text(
        "Open this playground in Gitpod!".to_string(),
    )];

    // When
    let html = render(&link);

    // Then
    assert!(
        html.contains("sd-btn-primary sd-shadow-sm\" href=\"https://gitpod.io/"),
        "{html}"
    );
    assert!(
        html.contains("<span>Open this playground in Gitpod!</span>"),
        "{html}"
    );
}

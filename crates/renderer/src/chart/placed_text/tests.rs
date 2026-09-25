use plotters::style::RGBColor;

use super::*;

const BLACK: RGBColor = RGBColor(0, 0, 0);

fn label(text: &str, angle: u16, side: Side) -> PlacedText {
    PlacedText::new(text.to_string(), (10, 20), angle, side, 12, BLACK)
}

#[test]
fn test_an_unturned_label_below_an_axis_hangs_centred_under_its_tick() {
    // Given / When
    let placed = placement(0, Side::Below);

    // Then
    assert_eq!(placed, (Anchor::Middle, Baseline::Hanging));
}

#[test]
fn test_a_label_below_an_axis_turned_upwards_ends_at_its_tick() {
    // Given — 45° counter-clockwise runs up and to the right, so the text must
    // end at the tick for its body to hang below the axis
    let angle = 45;

    // When
    let placed = placement(angle, Side::Below);

    // Then
    assert_eq!(placed, (Anchor::End, Baseline::Middle));
}

#[test]
fn test_a_label_below_an_axis_turned_downwards_starts_at_its_tick() {
    // Given
    let angle = 270;

    // When
    let placed = placement(angle, Side::Below);

    // Then
    assert_eq!(placed, (Anchor::Start, Baseline::Middle));
}

#[test]
fn test_an_upside_down_label_below_an_axis_uses_its_baseline() {
    // Given — the text runs across the side, so no end can put it below
    let angle = 180;

    // When
    let placed = placement(angle, Side::Below);

    // Then
    assert_eq!(placed, (Anchor::Middle, Baseline::Alphabetic));
}

#[test]
fn test_an_unturned_label_left_of_an_axis_ends_at_its_tick() {
    // Given / When
    let placed = placement(0, Side::Left);

    // Then
    assert_eq!(placed, (Anchor::End, Baseline::Middle));
}

#[test]
fn test_a_label_left_of_an_axis_turned_upright_sits_on_its_baseline() {
    // Given — read bottom to top, the glyphs' tops point left
    let angle = 90;

    // When
    let placed = placement(angle, Side::Left);

    // Then
    assert_eq!(placed, (Anchor::Middle, Baseline::Alphabetic));
}

#[test]
fn test_a_label_left_of_an_axis_turned_the_other_way_hangs() {
    // Given — read top to bottom, the glyphs' tops point right
    let angle = 270;

    // When
    let placed = placement(angle, Side::Left);

    // Then
    assert_eq!(placed, (Anchor::Middle, Baseline::Hanging));
}

#[test]
fn test_an_unturned_value_above_a_bar_sits_on_its_baseline() {
    // Given / When
    let placed = placement(0, Side::Above);

    // Then
    assert_eq!(placed, (Anchor::Middle, Baseline::Alphabetic));
}

#[test]
fn test_a_value_past_a_horizontal_bar_starts_at_its_end() {
    // Given / When
    let placed = placement(0, Side::Right);

    // Then
    assert_eq!(placed, (Anchor::Start, Baseline::Middle));
}

#[test]
fn test_a_centred_value_stays_centred_at_any_angle() {
    // Given
    let angles = [0, 45, 90, 180, 300];

    // When
    let placed: Vec<_> = angles
        .iter()
        .map(|angle| placement(*angle, Side::Centre))
        .collect();

    // Then
    assert!(
        placed
            .iter()
            .all(|p| *p == (Anchor::Middle, Baseline::Middle))
    );
}

#[test]
fn test_an_unturned_text_carries_no_transform() {
    // Given
    let text = label("Peter", 0, Side::Below);

    // When
    let svg = text.to_svg();

    // Then
    assert!(!svg.contains("transform"), "{svg}");
    assert!(svg.starts_with("<text x=\"10\" y=\"20\""), "{svg}");
    assert!(svg.ends_with(">Peter</text>"), "{svg}");
}

#[test]
fn test_a_turned_text_is_rotated_clockwise_by_the_negated_angle_about_its_anchor() {
    // Given — matplotlib's angles run counter-clockwise, SVG's clockwise
    let text = label("Peter", 45, Side::Below);

    // When
    let svg = text.to_svg();

    // Then
    assert!(svg.contains("transform=\"rotate(-45, 10, 20)\""), "{svg}");
}

#[test]
fn test_text_is_escaped() {
    // Given
    let text = label("<b> & co", 0, Side::Below);

    // When
    let svg = text.to_svg();

    // Then
    assert!(svg.contains("&lt;b&gt; &amp; co"), "{svg}");
}

#[test]
fn test_the_font_size_is_scaled_as_plotters_scales_it() {
    // Given — so a bar chart's 12 reads like a pie's 12
    let text = label("x", 0, Side::Below);

    // When
    let svg = text.to_svg();

    // Then
    assert!(svg.contains("font-size=\"9.68\""), "{svg}");
}

#[test]
fn test_the_colour_is_written_as_hex() {
    // Given
    let text = PlacedText::new(
        "x".to_string(),
        (0, 0),
        0,
        Side::Below,
        12,
        RGBColor(0x12, 0xab, 0xff),
    );

    // When
    let svg = text.to_svg();

    // Then
    assert!(svg.contains("fill=\"#12ABFF\""), "{svg}");
}

#[test]
fn test_an_unturned_extent_is_the_box_itself() {
    // Given / When
    let extent = rotated_extent((40, 10), 0);

    // Then
    assert_eq!(extent, (40, 10));
}

#[test]
fn test_a_quarter_turn_swaps_the_extent() {
    // Given / When
    let extent = rotated_extent((40, 10), 90);

    // Then
    assert_eq!(extent, (10, 40));
}

#[test]
fn test_a_diagonal_extent_covers_both_projections() {
    // Given — 40·cos45 + 10·sin45 ≈ 35.4 in both directions
    let angle = 45;

    // When
    let extent = rotated_extent((40, 10), angle);

    // Then
    assert_eq!(extent, (36, 36));
}

#[test]
fn test_texts_are_spliced_in_before_the_document_closes() {
    // Given
    let svg = "<svg>\n<rect/>\n</svg>\n";

    // When
    let spliced = splice_into(svg, &[label("a", 0, Side::Below)]).unwrap();

    // Then
    assert!(spliced.starts_with("<svg>\n<rect/>\n<text"), "{spliced}");
    assert!(spliced.ends_with("</text>\n</svg>\n"), "{spliced}");
}

#[test]
fn test_a_document_without_a_closing_tag_is_refused() {
    // Given
    let svg = "<svg>";

    // When
    let spliced = splice_into(svg, &[]);

    // Then
    assert_eq!(spliced, None);
}

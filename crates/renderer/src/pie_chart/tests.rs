use super::*;

fn wedge(label: &str, count: u64) -> PieWedge {
    PieWedge {
        label: label.to_string(),
        count,
    }
}

/// A chart of `wedges` with no options.
fn drawn(wedges: &[PieWedge]) -> Option<String> {
    render_pie_svg(&PieSpec {
        wedges,
        colors: &[],
        text_color: None,
        legend: false,
    })
}

#[test]
fn test_a_chart_is_drawn_as_an_svg_element() {
    // Given
    let wedges = [wedge("Requirements", 24), wedge("Tests", 12)];

    // When
    let svg = drawn(&wedges).expect("expected a chart");

    // Then
    assert!(svg.trim_start().starts_with("<svg"), "{svg}");
    assert!(svg.trim_end().ends_with("</svg>"), "{svg}");
}

#[test]
fn test_the_same_counts_draw_the_same_bytes() {
    // Given — a rendered page is a build artefact cached on its inputs, so an
    // unchanged chart must not change the page
    let wedges = [wedge("Requirements", 24), wedge("Tests", 12)];

    // When
    let first = drawn(&wedges).expect("expected a chart");
    let second = drawn(&wedges).expect("expected a chart");

    // Then
    assert_eq!(first, second);
}

#[test]
fn test_different_counts_draw_different_pictures() {
    // Given
    let even = [wedge("A", 1), wedge("B", 1)];
    let lopsided = [wedge("A", 9), wedge("B", 1)];

    // When
    let even_svg = drawn(&even).expect("expected a chart");
    let lopsided_svg = drawn(&lopsided).expect("expected a chart");

    // Then
    assert_ne!(even_svg, lopsided_svg);
}

#[test]
fn test_a_chart_whose_every_wedge_is_zero_is_not_drawn() {
    // Given — a pie of nothing has no geometry at all, so the caller reports
    // it rather than placing an empty picture
    let wedges = [wedge("Requirements", 0), wedge("Tests", 0)];

    // When
    let svg = drawn(&wedges);

    // Then
    assert!(svg.is_none());
}

#[test]
fn test_a_chart_with_no_wedges_at_all_is_not_drawn() {
    // Given
    let wedges: [PieWedge; 0] = [];

    // When
    let svg = drawn(&wedges);

    // Then
    assert!(svg.is_none());
}

#[test]
fn test_one_zero_wedge_among_others_still_draws() {
    // Given — only the *total* has to be non-zero; an empty wedge simply has
    // no area
    let wedges = [wedge("Requirements", 5), wedge("Tests", 0)];

    // When
    let svg = drawn(&wedges);

    // Then
    assert!(svg.is_some());
}

#[test]
fn test_the_written_colours_are_the_ones_drawn() {
    // Given
    let wedges = [wedge("A", 1), wedge("B", 1)];
    let colors = [
        ChartColor::parse("#ff0000").unwrap(),
        ChartColor::parse("#00ff00").unwrap(),
    ];

    // When
    let svg = render_pie_svg(&PieSpec {
        wedges: &wedges,
        colors: &colors,
        text_color: None,
        legend: false,
    })
    .expect("expected a chart");

    // Then
    assert!(svg.contains("#FF0000"), "no red wedge drawn");
    assert!(svg.contains("#00FF00"), "no green wedge drawn");
}

#[test]
fn test_fewer_colours_than_wedges_repeats_rather_than_leaving_one_uncoloured() {
    // Given — the palette wraps too, so the two cases need no separate
    // handling
    let wedges = [wedge("A", 1), wedge("B", 1), wedge("C", 1)];
    let colors = [ChartColor::parse("#ff0000").unwrap()];

    // When
    let svg = render_pie_svg(&PieSpec {
        wedges: &wedges,
        colors: &colors,
        text_color: None,
        legend: false,
    })
    .expect("expected a chart");

    // Then — every wedge took the one colour given
    assert_eq!(svg.matches("#FF0000").count(), 3);
}

#[test]
fn test_the_legend_names_each_wedge_with_its_count() {
    // Given
    let wedges = [wedge("Requirements", 24), wedge("Tests", 12)];

    // When
    let svg = render_pie_svg(&PieSpec {
        wedges: &wedges,
        colors: &[],
        text_color: None,
        legend: true,
    })
    .expect("expected a chart");

    // Then
    assert!(svg.contains("Requirements"), "no legend entry drawn");
    assert!(svg.contains("24"), "no count drawn beside the name");
}

#[test]
fn test_the_default_palette_is_used_when_no_colours_are_written() {
    // Given
    let wedges = [wedge("A", 1)];

    // When
    let svg = drawn(&wedges).expect("expected a chart");

    // Then — the first palette entry, so an unchanged chart keeps its colours
    assert!(svg.contains("#4C72B0"), "{svg}");
}

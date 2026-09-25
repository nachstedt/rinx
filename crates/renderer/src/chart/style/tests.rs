use super::*;

#[test]
fn test_without_written_colours_the_palette_is_used_in_order() {
    // Given
    let colors: [ChartColor; 0] = [];

    // When
    let third = series_color(&colors, 2);

    // Then
    let (red, green, blue) = PALETTE[2];
    assert_eq!(third, RGBColor(red, green, blue));
}

#[test]
fn test_the_palette_wraps_round_past_its_end() {
    // Given
    let colors: [ChartColor; 0] = [];

    // When
    let wrapped = series_color(&colors, PALETTE.len());

    // Then
    assert_eq!(wrapped, series_color(&colors, 0));
}

#[test]
fn test_written_colours_are_used_and_repeat_when_too_few() {
    // Given
    let colors = [
        ChartColor::parse("red").unwrap(),
        ChartColor::parse("blue").unwrap(),
    ];

    // When
    let drawn = (series_color(&colors, 0), series_color(&colors, 3));

    // Then
    assert_eq!(drawn, (RGBColor(0xff, 0, 0), RGBColor(0, 0, 0xff)));
}

#[test]
fn test_a_colour_converts_channel_for_channel() {
    // Given
    let color = ChartColor::parse("#102030").unwrap();

    // When
    let converted = to_rgb(color);

    // Then
    assert_eq!(converted, RGBColor(0x10, 0x20, 0x30));
}

#[test]
fn test_a_count_is_measured_exactly() {
    // Given
    let count = 12;

    // When
    let size = as_size(count);

    // Then
    assert!((size - 12.0).abs() < f64::EPSILON);
}

#[test]
fn test_an_enormous_count_saturates_rather_than_losing_precision() {
    // Given
    let count = u64::MAX;

    // When
    let size = as_size(count);

    // Then
    assert!((size - f64::from(u32::MAX)).abs() < f64::EPSILON);
}

#[test]
fn test_a_measurement_rounds_to_the_nearest_pixel() {
    // Given
    let measures = [12.4, 12.6, -3.5];

    // When
    let pixels: Vec<i32> = measures.iter().map(|m| to_pixel(*m)).collect();

    // Then
    assert_eq!(pixels, [12, 13, -4]);
}

#[test]
fn test_an_absurd_measurement_saturates_rather_than_wrapping() {
    // Given
    let measures = [1e12, -1e12, f64::NAN];

    // When
    let pixels: Vec<i32> = measures.iter().map(|m| to_pixel(*m)).collect();

    // Then
    assert_eq!(pixels, [i32::MAX, i32::MIN, 0]);
}

#[test]
fn test_a_bar_series_past_the_written_colours_takes_the_palette_from_its_start() {
    // Given — needbar's rule: the written list, then the default cycle
    let colors = [ChartColor::parse("red").unwrap()];

    // When
    let drawn = (
        extended_series_color(&colors, 0),
        extended_series_color(&colors, 1),
    );

    // Then
    let (red, green, blue) = PALETTE[0];
    assert_eq!(drawn, (RGBColor(0xff, 0, 0), RGBColor(red, green, blue)));
}

#[test]
fn test_the_extended_list_wraps_round_as_a_whole() {
    // Given
    let colors = [ChartColor::parse("red").unwrap()];

    // When
    let wrapped = extended_series_color(&colors, 1 + PALETTE.len());

    // Then
    assert_eq!(wrapped, RGBColor(0xff, 0, 0));
}

#[test]
fn test_without_written_colours_a_bar_chart_uses_the_palette_alone() {
    // Given
    let colors: [ChartColor; 0] = [];

    // When
    let second = extended_series_color(&colors, 1);

    // Then
    assert_eq!(second, series_color(&colors, 1));
}

use super::*;

fn shape(series: usize, categories: usize, stacked: bool) -> GridShape {
    GridShape {
        series,
        categories,
        stacked,
    }
}

fn close(left: f64, right: f64) -> bool {
    (left - right).abs() < 1e-9
}

#[test]
fn test_side_by_side_bars_are_grouped_with_one_gap_between_groups() {
    // Given — three series, so a group is three bars and a gap: 0,1,2 then 4,5,6
    let grid = shape(3, 2, false);

    // When
    let positions: Vec<f64> = (0..3).map(|series| grid.bar_position(series, 1)).collect();

    // Then
    assert_eq!(positions, [4.0, 5.0, 6.0]);
}

#[test]
fn test_stacked_bars_share_their_categorys_position() {
    // Given
    let grid = shape(3, 2, true);

    // When
    let positions: Vec<f64> = (0..3).map(|series| grid.bar_position(series, 1)).collect();

    // Then
    assert_eq!(positions, [1.0, 1.0, 1.0]);
}

#[test]
fn test_a_category_is_centred_under_its_group() {
    // Given
    let grid = shape(3, 2, false);

    // When
    let centre = grid.category_centre(1);

    // Then
    assert!(close(centre, 5.0), "{centre}");
}

#[test]
fn test_the_category_range_leaves_a_margin_past_the_outer_bars() {
    // Given
    let grid = shape(2, 3, false);

    // When — the last bar is series 1 of category 2: 1 + 2·3 = 7
    let (low, high) = grid.category_range();

    // Then
    assert!(close(low, -0.6) && close(high, 7.6), "{low}..{high}");
}

#[test]
fn test_the_value_axis_rounds_up_past_the_tallest_bar() {
    // Given — 24 plus a tenth of headroom is 27, and 27 in at most eight steps
    // is steps of 5
    let extent = 24;

    // When
    let axis = ValueAxis::fitting(extent).unwrap();

    // Then
    assert_eq!(axis, ValueAxis { top: 30, step: 5 });
}

#[test]
fn test_a_small_extent_still_gets_headroom_and_whole_ticks() {
    // Given
    let extent = 3;

    // When
    let axis = ValueAxis::fitting(extent).unwrap();

    // Then
    assert_eq!(axis, ValueAxis { top: 4, step: 1 });
    assert_eq!(axis.ticks().collect::<Vec<_>>(), [0, 1, 2, 3, 4]);
}

#[test]
fn test_a_large_extent_counts_in_round_steps() {
    // Given
    let extent = 1234;

    // When
    let axis = ValueAxis::fitting(extent).unwrap();

    // Then
    assert_eq!(
        axis,
        ValueAxis {
            top: 1400,
            step: 200
        }
    );
}

#[test]
fn test_an_extent_of_zero_has_no_axis() {
    // Given / When
    let axis = ValueAxis::fitting(0);

    // Then
    assert_eq!(axis, None);
}

#[test]
fn test_an_enormous_extent_does_not_overflow() {
    // Given
    let extent = u64::MAX;

    // When
    let axis = ValueAxis::fitting(extent).unwrap();

    // Then
    assert!(axis.top >= axis.step);
}

#[test]
fn test_side_by_side_the_extent_is_the_tallest_bar() {
    // Given
    let values = vec![vec![1, 7], vec![4, 2]];

    // When
    let extent = value_extent(&values, false);

    // Then
    assert_eq!(extent, 7);
}

#[test]
fn test_stacked_the_extent_is_the_tallest_stack() {
    // Given
    let values = vec![vec![1, 7], vec![4, 2]];

    // When
    let extent = value_extent(&values, true);

    // Then
    assert_eq!(extent, 9);
}

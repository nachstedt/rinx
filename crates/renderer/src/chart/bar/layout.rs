//! Where a bar chart's bars go, in the chart's own units — before anything is
//! turned into pixels.
//!
//! Two coordinates. Along the **category** axis a bar sits at the position
//! sphinx-needs' `needbar` gives it, so a migrated chart groups its bars the
//! same way: side by side a series' bar is at `series + category × (series
//! count + 1)` — one gap the width of a bar between groups — and stacked every
//! series of a category shares the category's own index. Along the **value**
//! axis the scale runs from zero to a round number above the tallest bar or
//! stack, with ticks at whole steps: the values are counts of entities, so a
//! tick at 2.5 would name a number no chart can hold.

/// The width of one bar, in category-axis units — matplotlib's default, which
/// is what sphinx-needs draws with.
pub(super) const BAR_WIDTH: f64 = 0.8;

/// How far the category axis reaches past the outermost bars' centres, so the
/// first and last bars do not touch the frame.
const CATEGORY_MARGIN: f64 = 0.6;

/// The most ticks the value axis carries; the step grows until it fits.
const MAX_VALUE_TICKS: u64 = 8;

/// The shape of a chart's grid, which is all its geometry depends on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct GridShape {
    pub series: usize,
    pub categories: usize,
    pub stacked: bool,
}

impl GridShape {
    /// The category-axis position of `series`' bar in `category`.
    pub(super) fn bar_position(self, series: usize, category: usize) -> f64 {
        if self.stacked {
            return as_units(category);
        }
        as_units(series) + as_units(category) * as_units(self.series + 1)
    }

    /// The category-axis position a category's tick and label sit at: the
    /// middle of its group of bars.
    pub(super) fn category_centre(self, category: usize) -> f64 {
        let first = self.bar_position(0, category);
        let last = self.bar_position(self.series.saturating_sub(1), category);
        f64::midpoint(first, last)
    }

    /// The category axis' extent, from before the first bar to after the last.
    pub(super) fn category_range(self) -> (f64, f64) {
        let last = self.bar_position(
            self.series.saturating_sub(1),
            self.categories.saturating_sub(1),
        );
        (-CATEGORY_MARGIN, last + CATEGORY_MARGIN)
    }
}

/// The value axis: from zero to `top`, with a tick every `step`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ValueAxis {
    pub top: u64,
    pub step: u64,
}

impl ValueAxis {
    /// A value axis with room for `extent`, the tallest bar or stack.
    ///
    /// The headroom — a tenth, and at least one — leaves space for a value
    /// written past the tallest bar's end, which is what `:show_top_sum:`
    /// puts there. `None` for an extent of zero: a chart of nothing has no
    /// scale, and the caller reports it rather than drawing an empty frame.
    pub(super) fn fitting(extent: u64) -> Option<Self> {
        if extent == 0 {
            return None;
        }
        let wanted = extent.saturating_add(extent.div_ceil(10).max(1));
        let step = nice_step(wanted);
        Some(Self {
            top: wanted.div_ceil(step).saturating_mul(step),
            step,
        })
    }

    /// Every tick, from zero to the top inclusive.
    pub(super) fn ticks(self) -> impl Iterator<Item = u64> {
        (0..=self.top / self.step).map(move |at| at * self.step)
    }
}

/// The smallest step of the form 1, 2 or 5 × 10ⁿ that divides `0..=top` into
/// at most [`MAX_VALUE_TICKS`] intervals — the steps a reader counts in.
fn nice_step(top: u64) -> u64 {
    let mut magnitude: u64 = 1;
    loop {
        for factor in [1, 2, 5] {
            let step = magnitude.saturating_mul(factor);
            if top.div_ceil(step) <= MAX_VALUE_TICKS {
                return step;
            }
        }
        magnitude = magnitude.saturating_mul(10);
    }
}

/// The tallest thing the value axis has to hold: the largest bar side by side,
/// or the largest stack.
pub(super) fn value_extent(values: &[Vec<u64>], stacked: bool) -> u64 {
    if stacked {
        let categories = values.first().map_or(0, Vec::len);
        return (0..categories)
            .map(|category| {
                values
                    .iter()
                    .map(|row| row.get(category).copied().unwrap_or(0))
                    .fold(0u64, u64::saturating_add)
            })
            .max()
            .unwrap_or(0);
    }
    values.iter().flatten().copied().max().unwrap_or(0)
}

/// An index as a coordinate.
///
/// Through `u32` for the reason `style::as_size` gives: exact for every grid a
/// document can hold, saturating past that rather than losing precision.
fn as_units(index: usize) -> f64 {
    f64::from(u32::try_from(index).unwrap_or(u32::MAX))
}

#[cfg(test)]
mod tests;

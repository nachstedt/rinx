//! The one place GitHub's octicons are drawn.
//!
//! `octicons_pack` is contained here the same way `syntect` is contained in
//! [`crate::highlight`] and `math-core` in [`crate::math`]: the icon set is a
//! *backend*, its vocabulary changes when the dependency is upgraded, and it
//! must never reach a `.ast` file — which carries only the name an author
//! wrote (`rusty_sphinx_ast::OcticonName`). The parser checks that name
//! against the same set while reading the option, so by the time a page is
//! drawn an unresolvable name has already been reported and is simply not
//! drawn.
//!
//! The emitted element matches what sphinx-design's `get_octicon` builds, so a
//! page styled for sphinx-design keeps working:
//!
//! ```html
//! <svg version="1.1" width="1.0em" height="1.0em"
//!      class="sd-octicon sd-octicon-light-bulb" viewBox="0 0 16 16"
//!      aria-hidden="true">…</svg>
//! ```
//!
//! The crate ships each icon as a complete `<svg>` with its `width`/`height`
//! stripped and its `viewBox` kept, so this module unwraps that element and
//! writes its own, rather than splicing attributes into someone else's tag.

use std::fmt::Write as _;

/// The heights an octicon is drawn at, in `em`.
///
/// Two constants rather than a free parameter because this build has exactly
/// two call sites — a dropdown's `:icon:` and its state marker — and the
/// second is what makes the 24px artwork get picked. See
/// [`original_height_for`].
pub(crate) const ICON_HEIGHT_EM: f64 = 1.0;
pub(crate) const MARKER_HEIGHT_EM: f64 = 1.5;

/// Renders one octicon as an inline `<svg>` element.
///
/// `classes` are appended after the two sphinx-design always writes
/// (`sd-octicon` and `sd-octicon-<name>`). Returns `None` when no icon by that
/// name exists, which is the caller's cue to report it.
pub(crate) fn render_octicon(name: &str, height_em: f64, classes: &[&str]) -> Option<String> {
    let svg = lookup(name, original_height_for(height_em))?;
    let view_box = view_box_of(svg)?;
    let content = inner_content_of(svg)?;
    let width_em = width_for(view_box, height_em)?;

    let mut class_attr = format!("sd-octicon sd-octicon-{name}");
    for class in classes {
        class_attr.push(' ');
        class_attr.push_str(class);
    }

    let mut element = String::new();
    let _ = write!(
        element,
        "<svg version=\"1.1\" width=\"{}em\" height=\"{}em\" class=\"{}\" viewBox=\"{}\" aria-hidden=\"true\">{content}</svg>",
        format_em(width_em),
        format_em(height_em),
        html_escape::encode_double_quoted_attribute(&class_attr),
        html_escape::encode_double_quoted_attribute(view_box),
    );
    Some(element)
}

/// Which artwork a requested height should be drawn from.
///
/// An octicon's 16- and 24-pixel versions are different drawings rather than
/// one scaled twice. sphinx-design switches to the 24px artwork at 1.5em and
/// above, which is why a dropdown's state marker — the one thing it draws at
/// 1.5em — comes from the larger set.
fn original_height_for(height_em: f64) -> u32 {
    if height_em >= 1.5 { 24 } else { 16 }
}

/// Every artwork height the icon set ships, smallest first.
///
/// Mostly 16 and 24 — but nine icons add a 12, and a couple exist only at 48
/// or 96, so a search that knew about two sizes would report a handful of
/// perfectly good names as unknown (`no-entry-fill` ships at 12 alone).
const ARTWORK_HEIGHTS: &[u32] = &[12, 16, 24, 48, 96];

/// Finds an icon's SVG, preferring the given artwork height.
///
/// Falls back to every other height rather than failing, in the order above:
/// refusing to draw an icon because the *preferred* size is missing would
/// report a valid name as unknown. sphinx-design does the same, taking
/// whichever height its metadata happens to list first.
fn lookup(name: &str, preferred_height: u32) -> Option<&'static str> {
    let fallbacks = ARTWORK_HEIGHTS
        .iter()
        .copied()
        .filter(|&height| height != preferred_height);
    for height in std::iter::once(preferred_height).chain(fallbacks) {
        if let Some(icon) = octicons_pack::get_icon(&format!("{name}-{height}")) {
            return Some(icon.svg);
        }
    }
    None
}

/// The `viewBox` attribute value of a `<svg …>` element.
fn view_box_of(svg: &'static str) -> Option<&'static str> {
    let after_name = svg.split_once("viewBox=\"")?.1;
    let (value, _) = after_name.split_once('"')?;
    Some(value)
}

/// Everything between a `<svg …>` element's tags — the paths that draw it.
fn inner_content_of(svg: &'static str) -> Option<&'static str> {
    let (_, after_open_tag) = svg.split_once('>')?;
    after_open_tag.strip_suffix("</svg>")
}

/// The width that keeps an icon's aspect ratio at the requested height.
///
/// Read from the `viewBox` rather than from metadata, since that is the only
/// place the crate records an icon's own proportions — and most octicons are
/// square, but not all of them are.
fn width_for(view_box: &str, height_em: f64) -> Option<f64> {
    let mut values = view_box.split_whitespace().skip(2);
    let width: f64 = values.next()?.parse().ok()?;
    let height: f64 = values.next()?.parse().ok()?;
    if height == 0.0 {
        return None;
    }
    Some(width * height_em / height)
}

/// Formats a measurement the way sphinx-design's Python does: rounded to three
/// decimals — ties to even, as both languages round — with trailing zeros
/// dropped but never the whole fraction, so `1` prints as `1.0`.
fn format_em(value: f64) -> String {
    let rounded = format!("{value:.3}");
    let trimmed = rounded.trim_end_matches('0');
    if trimmed.ends_with('.') {
        format!("{trimmed}0")
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_renders_a_known_icon_as_an_svg_element() {
        // Given
        let name = "light-bulb";

        // When
        let svg = render_octicon(name, ICON_HEIGHT_EM, &[]).expect("light-bulb is an octicon");

        // Then
        assert!(svg.starts_with("<svg version=\"1.1\" width=\"1.0em\" height=\"1.0em\""));
        assert!(svg.contains("class=\"sd-octicon sd-octicon-light-bulb\""));
        assert!(svg.contains("aria-hidden=\"true\""));
        assert!(svg.ends_with("</svg>"));
        assert!(svg.contains("<path"));
    }

    #[test]
    fn test_appends_the_callers_own_classes() {
        // Given
        let classes = ["no-title"];

        // When
        let svg = render_octicon("kebab-horizontal", ICON_HEIGHT_EM, &classes)
            .expect("kebab-horizontal is an octicon");

        // Then
        assert!(svg.contains("class=\"sd-octicon sd-octicon-kebab-horizontal no-title\""));
    }

    #[test]
    fn test_reports_an_unknown_name_as_none() {
        // Given
        let name = "not-an-octicon";

        // When
        let svg = render_octicon(name, ICON_HEIGHT_EM, &[]);

        // Then
        assert_eq!(svg, None);
    }

    #[test]
    fn test_draws_the_24_pixel_artwork_at_one_and_a_half_em() {
        // Given / When
        let marker = render_octicon("chevron-right", MARKER_HEIGHT_EM, &[])
            .expect("chevron-right is an octicon");
        let icon = render_octicon("chevron-right", ICON_HEIGHT_EM, &[])
            .expect("chevron-right is an octicon");

        // Then — the state marker is the larger drawing, not the small one scaled
        assert!(marker.contains("viewBox=\"0 0 24 24\""));
        assert!(marker.contains("height=\"1.5em\""));
        assert!(icon.contains("viewBox=\"0 0 16 16\""));
    }

    #[test]
    fn test_falls_back_to_the_only_height_an_icon_ships_in() {
        // Given — `no-entry-fill` is drawn at 12 pixels and nothing else
        let name = "no-entry-fill";

        // When
        let svg = render_octicon(name, ICON_HEIGHT_EM, &[]).expect("no-entry-fill is an octicon");

        // Then — drawn from the size it does have, rather than reported missing
        assert!(svg.contains("viewBox=\"0 0 12 12\""));
    }

    #[test]
    fn test_prefers_the_requested_height_over_the_fallbacks() {
        // Given — `bookmark-fill` ships at 24 only, `beaker` at both
        // When
        let only_24 = render_octicon("bookmark-fill", ICON_HEIGHT_EM, &[])
            .expect("bookmark-fill is an octicon");
        let both = render_octicon("beaker", ICON_HEIGHT_EM, &[]).expect("beaker is an octicon");

        // Then — the fallback is taken only where the preferred size is absent
        assert!(only_24.contains("viewBox=\"0 0 24 24\""));
        assert!(both.contains("viewBox=\"0 0 16 16\""));
    }

    #[test]
    fn test_original_height_switches_at_one_and_a_half_em() {
        // Given / When / Then
        assert_eq!(original_height_for(1.0), 16);
        assert_eq!(original_height_for(1.499), 16);
        assert_eq!(original_height_for(1.5), 24);
        assert_eq!(original_height_for(2.0), 24);
    }

    #[test]
    fn test_view_box_of_reads_the_attribute() {
        // Given
        let svg = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 16 16\"><path/></svg>";

        // When
        let view_box = view_box_of(svg);

        // Then
        assert_eq!(view_box, Some("0 0 16 16"));
    }

    #[test]
    fn test_inner_content_of_keeps_only_what_draws_the_icon() {
        // Given
        let svg = "<svg viewBox=\"0 0 16 16\"><path d=\"M0 0\"/></svg>";

        // When
        let content = inner_content_of(svg);

        // Then
        assert_eq!(content, Some("<path d=\"M0 0\"/>"));
    }

    #[test]
    fn test_width_keeps_a_square_icons_proportions() {
        // Given
        let view_box = "0 0 16 16";

        // When
        let width = width_for(view_box, 1.0);

        // Then
        assert_eq!(width, Some(1.0));
    }

    #[test]
    fn test_width_widens_a_non_square_icon() {
        // Given — a 24-wide, 16-high drawing
        let view_box = "0 0 24 16";

        // When
        let width = width_for(view_box, 1.0);

        // Then
        assert_eq!(width, Some(1.5));
    }

    #[test]
    fn test_width_refuses_a_zero_height_view_box() {
        // Given
        let view_box = "0 0 24 0";

        // When
        let width = width_for(view_box, 1.0);

        // Then
        assert_eq!(width, None);
    }

    #[test]
    fn test_format_em_keeps_one_decimal_for_a_whole_number() {
        // Given / When / Then — matching Python's `str(round(1.0, 3))`
        assert_eq!(format_em(1.0), "1.0");
        assert_eq!(format_em(1.5), "1.5");
    }

    #[test]
    fn test_format_em_rounds_to_three_decimals() {
        // Given / When / Then
        // A 45-wide icon at 1em. Ties round to even, as Python's `round`
        // does, so this is 2.812 rather than 2.813 in both implementations.
        assert_eq!(format_em(45.0 / 16.0), "2.812");
        assert_eq!(format_em(1.23456), "1.235");
    }
}

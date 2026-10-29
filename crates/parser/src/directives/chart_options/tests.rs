use rinx_ast::{Diagnostic, DiagnosticCode, Domain, ImageAlign};

use super::*;

/// Distinct codes per failure, so each test can tell which one fired.
const CODES: ChartCodes = ChartCodes {
    invalid_color: DiagnosticCode::EntityBarInvalidColor,
    invalid_align: DiagnosticCode::EntityBarInvalidAlign,
    invalid_scale: DiagnosticCode::EntityBarInvalidScale,
    invalid_width: DiagnosticCode::EntityBarInvalidWidth,
    unusable_scale: DiagnosticCode::EntityBarUnusableScale,
};

const OWNER: ChartOwner<'static> = ChartOwner {
    directive: "needbar",
    codes: CODES,
};

fn option(name: &str, value: &str) -> OptionLine {
    OptionLine {
        name: name.to_string(),
        value: value.to_string(),
        raw: format!(":{name}: {value}"),
        line_index: 0,
        continued: false,
    }
}

/// Runs `read` against a fresh collector, returning its result and what it
/// reported.
fn run<T>(read: impl FnOnce(&mut Diagnostics, &ParseCtx<'_>) -> T) -> (T, Vec<Diagnostic>) {
    let ctx = ParseCtx::with_domain(Domain::Py);
    let mut diagnostics = Diagnostics::default();
    let result = read(&mut diagnostics, &ctx);
    (result, diagnostics.iter().cloned().collect())
}

#[test]
fn test_a_figure_option_is_read_into_its_value() {
    // Given
    let line = option("align", "center");

    // When
    let (read, reported) =
        run(|d, ctx| read_figure_option(&ChartFigureOption::Align, &line, OWNER, d, ctx));

    // Then
    assert_eq!(read, Some(FigureOption::Align(ImageAlign::Center)));
    assert!(reported.is_empty());
}

#[test]
fn test_classes_are_split_on_whitespace() {
    // Given
    let line = option("class", "wide  framed");

    // When
    let (read, _) =
        run(|d, ctx| read_figure_option(&ChartFigureOption::Class, &line, OWNER, d, ctx));

    // Then
    assert_eq!(
        read,
        Some(FigureOption::Classes(vec![
            "wide".to_string(),
            "framed".to_string()
        ]))
    );
}

#[test]
fn test_a_bad_figure_value_is_reported_under_the_callers_code() {
    // Given
    let line = option("scale", "big");

    // When
    let (read, reported) =
        run(|d, ctx| read_figure_option(&ChartFigureOption::Scale, &line, OWNER, d, ctx));

    // Then
    assert_eq!(read, None);
    assert_eq!(reported[0].code, DiagnosticCode::EntityBarInvalidScale);
    assert!(reported[0].message.starts_with("needbar: :scale:"));
}

#[test]
fn test_an_empty_name_is_reported_rather_than_registered() {
    // Given
    let line = option("name", "");

    // When
    let (read, reported) =
        run(|d, ctx| read_figure_option(&ChartFigureOption::Name, &line, OWNER, d, ctx));

    // Then
    assert_eq!(read, None);
    assert_eq!(reported[0].code, DiagnosticCode::DirectiveEmptyOptionValue);
}

#[test]
fn test_a_bad_colour_is_dropped_and_the_rest_kept() {
    // Given
    let line = option("colors", "red, mauve, #00f");

    // When
    let (colors, reported) = run(|d, ctx| read_colors(&line, OWNER, d, ctx));

    // Then
    assert_eq!(colors.len(), 2);
    assert_eq!(reported.len(), 1);
    assert_eq!(reported[0].code, DiagnosticCode::EntityBarInvalidColor);
    assert!(
        reported[0].message.contains("'mauve'"),
        "{}",
        reported[0].message
    );
}

#[test]
fn test_an_unusable_scale_is_reported_under_the_callers_code() {
    // Given
    let lines = [option("scale", "50")];

    // When
    let ((), reported) = run(|d, ctx| report_unusable_scale(&lines, OWNER, d, ctx));

    // Then
    assert_eq!(reported[0].code, DiagnosticCode::EntityBarUnusableScale);
}

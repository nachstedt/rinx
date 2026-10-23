//! What an option's value is read into, written once for every directive.
//!
//! A row of `directive_options!` may name the type its value is read into —
//! `Align(ImageAlign) => (["align"], EntityFlowInvalidAlign, "…")` — and the
//! variant then carries the value, parsed. The type does two things through
//! [`OptionValueType`]: it reads a written value, or says why it is none, and
//! it says what completion offers, so what is offered and what is accepted
//! cannot drift apart. `classify_options` reports a value that fails to read
//! on its line, under the row's code, and hands the interpreter only values
//! that read.
//!
//! A value whose check needs more than its own line — `:widths:` against the
//! content's column count, a `:scale:` against a `:width:` — is read into its
//! type here and checked by the interpreter, which [`Located`] gives the line
//! to report on.

use std::num::{NonZeroU32, NonZeroUsize};

use rinx_ast::{
    AttributeValue, Diagnostic, DiagnosticCode, ImageAlign, LengthOrPercentage, Spacing,
    SpacingKind, Span, TableAlign, TargetName,
};
use rinx_entity::{AttributeType, parse_attribute_value};

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;

use super::classes::split_classes;
use super::options::{OptionLine, parse_percentage};
use super::spec::OptionValue;

/// What an option's value is read into.
pub(in crate::directives) trait OptionValueType: Sized {
    /// What completion offers for it.
    const VALUE: OptionValue;
    /// The value written on `line`, or why it is none.
    fn read(line: &OptionLine, ctx: &ParseCtx<'_>) -> Result<Self, String>;
}

/// Reports a value that failed to read, on its line: as a missing value when
/// nothing was written — one mistake, one code, whichever option it is —
/// and otherwise under `code`, saying `problem`.
pub(in crate::directives) fn report_unread_value(
    line: &OptionLine,
    directive: &str,
    code: DiagnosticCode,
    problem: &str,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) {
    if line.value.trim().is_empty() {
        report_missing_value(line, directive, diagnostics, ctx);
    } else {
        report_invalid_option(line, directive, code, problem, diagnostics, ctx);
    }
}

/// Reports an option written without the value it needs, as
/// `directive.empty-option-value`.
pub(in crate::directives) fn report_missing_value(
    line: &OptionLine,
    directive: &str,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) {
    diagnostics.push(Diagnostic::at(
        DiagnosticCode::DirectiveEmptyOptionValue,
        format!(
            "{directive}: :{}: needs a value, so the option was ignored",
            line.name
        ),
        ctx.line_span(line.line_index, &line.raw),
    ));
}

/// Reports a value that was written and is none the option takes, under
/// `code`, saying `problem`.
pub(in crate::directives) fn report_invalid_option(
    line: &OptionLine,
    directive: &str,
    code: DiagnosticCode,
    problem: &str,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) {
    diagnostics.push(Diagnostic::at(
        code,
        format!("{directive}: :{}: {problem}", line.name),
        ctx.line_span(line.line_index, &line.raw),
    ));
}

/// Free text, kept as written.
impl OptionValueType for String {
    const VALUE: OptionValue = OptionValue::Text;

    fn read(line: &OptionLine, _ctx: &ParseCtx<'_>) -> Result<Self, String> {
        Ok(line.value.clone())
    }
}

/// A non-negative whole number.
impl OptionValueType for usize {
    const VALUE: OptionValue = OptionValue::Integer;

    fn read(line: &OptionLine, _ctx: &ParseCtx<'_>) -> Result<Self, String> {
        line.value
            .parse()
            .map_err(|_| format!("value '{}' is not a nonnegative integer", line.value))
    }
}

/// A whole number, which may be negative: `:tab-width:`, where docutils
/// spells "leave tabs alone" as a negative width.
impl OptionValueType for i32 {
    const VALUE: OptionValue = OptionValue::Integer;

    fn read(line: &OptionLine, _ctx: &ParseCtx<'_>) -> Result<Self, String> {
        line.value
            .trim()
            .parse()
            .map_err(|_| format!("needs an integer, found '{}'", line.value))
    }
}

/// A positive whole number.
impl OptionValueType for NonZeroU32 {
    const VALUE: OptionValue = OptionValue::Integer;

    fn read(line: &OptionLine, _ctx: &ParseCtx<'_>) -> Result<Self, String> {
        line.value
            .trim()
            .parse()
            .map_err(|_| format!("needs a positive integer, found '{}'", line.value))
    }
}

/// A depth limit: a positive number limits the depth, and Sphinx's documented
/// spelling of "unlimited" — zero or a negative number — limits nothing, so
/// the AST can use `NonZeroUsize` and make the meaningless state
/// unrepresentable downstream. Any other value is reported: silently switching
/// to unlimited would show far more than the author asked for.
///
/// `.. toctree::`'s `:maxdepth:` and `.. contents::`'s and `.. sectnum::`'s
/// `:depth:` agree on this exact ambiguity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::directives) struct DepthLimit(pub(in crate::directives) Option<NonZeroUsize>);

impl OptionValueType for DepthLimit {
    const VALUE: OptionValue = OptionValue::Integer;

    fn read(line: &OptionLine, _ctx: &ParseCtx<'_>) -> Result<Self, String> {
        line.value
            .trim()
            .parse::<isize>()
            .map(|value| Self(usize::try_from(value).ok().and_then(NonZeroUsize::new)))
            .map_err(|_| format!("needs a whole number, found '{}'", line.value))
    }
}

/// A label to reference the directive by, which must not be empty.
impl OptionValueType for TargetName {
    const VALUE: OptionValue = OptionValue::Text;

    fn read(line: &OptionLine, ctx: &ParseCtx<'_>) -> Result<Self, String> {
        NonEmptyText::read(line, ctx).map(|text| TargetName::new(&text.0))
    }
}

/// A length, or a percentage of the line's.
impl OptionValueType for LengthOrPercentage {
    const VALUE: OptionValue = OptionValue::Length;

    fn read(line: &OptionLine, _ctx: &ParseCtx<'_>) -> Result<Self, String> {
        LengthOrPercentage::new(&line.value).map_err(|problem| problem.to_string())
    }
}

/// Text that must not be empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::directives) struct NonEmptyText(pub(in crate::directives) String);

impl OptionValueType for NonEmptyText {
    const VALUE: OptionValue = OptionValue::Text;

    fn read(line: &OptionLine, _ctx: &ParseCtx<'_>) -> Result<Self, String> {
        if line.value.is_empty() {
            Err("needs a value, so the option was ignored".to_string())
        } else {
            Ok(Self(line.value.clone()))
        }
    }
}

/// Space-separated class names.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(in crate::directives) struct Classes(pub(in crate::directives) Vec<String>);

impl OptionValueType for Classes {
    const VALUE: OptionValue = OptionValue::Classes;

    fn read(line: &OptionLine, _ctx: &ParseCtx<'_>) -> Result<Self, String> {
        Ok(Self(split_classes(&line.value)))
    }
}

/// A percentage, `50` or `50%` alike, as docutils' `directives.percentage`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::directives) struct Percentage(pub(in crate::directives) u32);

impl OptionValueType for Percentage {
    const VALUE: OptionValue = OptionValue::Integer;

    fn read(line: &OptionLine, _ctx: &ParseCtx<'_>) -> Result<Self, String> {
        parse_percentage(&line.value)
            .map(Self)
            .ok_or_else(|| format!("expects a non-negative percentage, found '{}'", line.value))
    }
}

/// `true` or `false`, as an entity's boolean attribute is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::directives) struct Boolean(pub(in crate::directives) bool);

impl OptionValueType for Boolean {
    const VALUE: OptionValue = OptionValue::Choice(|| vec!["true", "false"]);

    fn read(line: &OptionLine, _ctx: &ParseCtx<'_>) -> Result<Self, String> {
        match parse_attribute_value(&AttributeType::Bool, &line.value) {
            Ok(AttributeValue::Bool(value)) => Ok(Self(value)),
            _ => Err(format!("expects true or false, found '{}'", line.value)),
        }
    }
}

/// A value together with the line it was written on, for a check the
/// interpreter makes later — against the content, or another option — to
/// report where the author wrote it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::directives) struct Located<T> {
    pub(in crate::directives) value: T,
    line_index: usize,
    raw: String,
}

impl<T> Located<T> {
    /// The span of the line the value was written on.
    pub(in crate::directives) fn span(&self, ctx: &ParseCtx<'_>) -> Option<Span> {
        ctx.line_span(self.line_index, &self.raw)
    }
}

impl<T: OptionValueType> OptionValueType for Located<T> {
    const VALUE: OptionValue = T::VALUE;

    fn read(line: &OptionLine, ctx: &ParseCtx<'_>) -> Result<Self, String> {
        Ok(Self {
            value: T::read(line, ctx)?,
            line_index: line.line_index,
            raw: line.raw.clone(),
        })
    }
}

/// One of a fixed set of words, as an enum the parser already has.
pub(in crate::directives) trait ChoiceValue: Sized {
    /// Every word, in the order the enum lists them.
    fn choices() -> Vec<&'static str>;
    /// The value `written` names.
    fn from_choice(written: &str) -> Option<Self>;
}

impl<T: ChoiceValue> OptionValueType for T {
    const VALUE: OptionValue = OptionValue::Choice(T::choices);

    fn read(line: &OptionLine, _ctx: &ParseCtx<'_>) -> Result<Self, String> {
        T::from_choice(line.value.trim()).ok_or_else(|| {
            format!(
                "expects one of {}, found '{}'",
                T::choices().join(", "),
                line.value
            )
        })
    }
}

/// A block-level picture's alignment: the horizontal three. A picture inside
/// a substitution, which also takes the vertical three, reads its own.
impl ChoiceValue for ImageAlign {
    fn choices() -> Vec<&'static str> {
        Self::HORIZONTAL.iter().copied().map(Self::as_str).collect()
    }

    fn from_choice(written: &str) -> Option<Self> {
        Self::parse(written)
    }
}

impl ChoiceValue for TableAlign {
    fn choices() -> Vec<&'static str> {
        Self::ALL.iter().map(Self::as_str).collect()
    }

    fn from_choice(written: &str) -> Option<Self> {
        written.parse().ok()
    }
}

/// Implements [`ChoiceValue`] for enums the AST already gives the three things
/// a choice needs: `ALL`, `as_str` and `parse`, which agree with each other.
macro_rules! choice_values {
    ($($ty:ty),* $(,)?) => {
        $(
            impl ChoiceValue for $ty {
                fn choices() -> Vec<&'static str> {
                    <$ty>::ALL.iter().copied().map(<$ty>::as_str).collect()
                }

                fn from_choice(written: &str) -> Option<Self> {
                    <$ty>::parse(written)
                }
            }
        )*
    };
}

choice_values!(
    rinx_ast::SemanticColor,
    rinx_ast::Chevron,
    rinx_ast::Animation,
    rinx_ast::TextAlign,
    rinx_ast::ChildDirection,
    rinx_ast::ChildAlign,
    rinx_ast::ImageLoading,
);

/// Implements [`OptionValueType`] for types whose `parse` already says why a
/// value is none, in an error that reads as the rest of a sentence.
macro_rules! parsed_values {
    ($($ty:ty => $value:ident),* $(,)?) => {
        $(
            impl OptionValueType for $ty {
                const VALUE: OptionValue = OptionValue::$value;

                fn read(line: &OptionLine, _ctx: &ParseCtx<'_>) -> Result<Self, String> {
                    <$ty>::parse(&line.value).map_err(|problem| problem.to_string())
                }
            }
        )*
    };
}

parsed_values!(
    rinx_ast::Gutter => Text,
    rinx_ast::ColumnSpec => Text,
    rinx_ast::ChartColor => Text,
    rinx_ast::LabelRotation => Integer,
    rinx_ast::PyVersionSpec => Text,
);

/// A length, as a picture's `:height:` is written.
impl OptionValueType for rinx_ast::Length {
    const VALUE: OptionValue = OptionValue::Length;

    fn read(line: &OptionLine, _ctx: &ParseCtx<'_>) -> Result<Self, String> {
        Self::new(&line.value).map_err(|problem| problem.to_string())
    }
}

/// A figure's `:figwidth:`: a length, or `image` for the picture's own.
impl OptionValueType for rinx_ast::FigureWidth {
    const VALUE: OptionValue = OptionValue::Length;

    fn read(line: &OptionLine, _ctx: &ParseCtx<'_>) -> Result<Self, String> {
        Self::new(&line.value).map_err(|problem| problem.to_string())
    }
}

/// A `:padding:`, by spacing level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::directives) struct Padding(pub(in crate::directives) Spacing);

impl OptionValueType for Padding {
    const VALUE: OptionValue = OptionValue::Text;

    fn read(line: &OptionLine, _ctx: &ParseCtx<'_>) -> Result<Self, String> {
        Spacing::parse(&line.value, SpacingKind::Padding)
            .map(Self)
            .map_err(|problem| problem.to_string())
    }
}

/// A `:margin:`, by spacing level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::directives) struct Margin(pub(in crate::directives) Spacing);

impl OptionValueType for Margin {
    const VALUE: OptionValue = OptionValue::Text;

    fn read(line: &OptionLine, _ctx: &ParseCtx<'_>) -> Result<Self, String> {
        Spacing::parse(&line.value, SpacingKind::Margin)
            .map(Self)
            .map_err(|problem| problem.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::Domain;

    /// An option line `:option: value`, on the block's first line.
    fn written(value: &str) -> OptionLine {
        OptionLine {
            name: "option".to_string(),
            value: value.to_string(),
            raw: format!(":option: {value}"),
            line_index: 0,
        }
    }

    /// What `value` reads as, as a `T`.
    fn read<T: OptionValueType>(value: &str) -> Result<T, String> {
        T::read(&written(value), &ParseCtx::with_domain(Domain::Py))
    }

    #[test]
    fn test_text_is_kept_as_written() {
        // Given / When / Then
        assert_eq!(read::<String>("a b"), Ok("a b".to_string()));
        assert_eq!(read::<String>(""), Ok(String::new()));
    }

    #[test]
    fn test_counts_and_integers_refuse_what_is_not_one() {
        // Given / When / Then
        assert_eq!(read::<usize>("3"), Ok(3));
        assert!(read::<usize>("-1").is_err());
        assert_eq!(read::<i32>("-1"), Ok(-1));
        assert!(read::<i32>("many").is_err());
        assert_eq!(
            read::<NonZeroU32>("2"),
            Ok(NonZeroU32::new(2).expect("two"))
        );
        assert!(read::<NonZeroU32>("0").is_err());
    }

    #[test]
    fn test_a_depth_limit_reads_zero_or_less_as_unlimited() {
        // Given / When / Then
        assert_eq!(
            read::<DepthLimit>("2"),
            Ok(DepthLimit(NonZeroUsize::new(2)))
        );
        assert_eq!(read::<DepthLimit>("0"), Ok(DepthLimit(None)));
        assert_eq!(read::<DepthLimit>("-1"), Ok(DepthLimit(None)));
        assert!(read::<DepthLimit>("deep").is_err());
    }

    #[test]
    fn test_non_empty_text_and_target_names_refuse_an_empty_value() {
        // Given / When / Then
        assert_eq!(read::<NonEmptyText>("x"), Ok(NonEmptyText("x".to_string())));
        assert!(read::<NonEmptyText>("").is_err());
        assert_eq!(read::<TargetName>("label"), Ok(TargetName::new("label")));
        assert!(read::<TargetName>("").is_err());
    }

    #[test]
    fn test_classes_split_on_whitespace() {
        // Given / When / Then
        assert_eq!(
            read::<Classes>("wide  framed"),
            Ok(Classes(vec!["wide".to_string(), "framed".to_string()]))
        );
    }

    #[test]
    fn test_a_percentage_reads_with_or_without_its_sign() {
        // Given / When / Then
        assert_eq!(read::<Percentage>("50"), Ok(Percentage(50)));
        assert_eq!(read::<Percentage>("50%"), Ok(Percentage(50)));
        assert!(read::<Percentage>("half").is_err());
    }

    #[test]
    fn test_a_boolean_reads_true_or_false() {
        // Given / When / Then
        assert_eq!(read::<Boolean>("false"), Ok(Boolean(false)));
        assert_eq!(read::<Boolean>("true"), Ok(Boolean(true)));
        assert_eq!(
            read::<Boolean>("maybe"),
            Err("expects true or false, found 'maybe'".to_string())
        );
    }

    #[test]
    fn test_a_choice_names_every_word_it_takes_when_refusing_one() {
        // Given / When / Then
        assert_eq!(read::<TableAlign>("center"), Ok(TableAlign::Center));
        assert_eq!(
            read::<TableAlign>("up"),
            Err("expects one of left, center, right, found 'up'".to_string())
        );
    }

    #[test]
    fn test_a_block_picture_aligns_horizontally_only() {
        // Given / When / Then
        assert_eq!(ImageAlign::choices(), ["left", "center", "right"]);
        assert!(read::<ImageAlign>("top").is_err());
    }

    #[test]
    fn test_a_choice_offers_its_words_to_completion() {
        // Given / When
        let OptionValue::Choice(choices) = <TableAlign as OptionValueType>::VALUE else {
            panic!("a choice");
        };

        // Then
        assert_eq!(choices(), ["left", "center", "right"]);
    }

    #[test]
    fn test_a_margin_reads_a_spacing_level() {
        // Given / When / Then
        assert!(read::<Margin>("2").is_ok());
        assert!(read::<Margin>("huge").is_err());
    }

    #[test]
    fn test_a_located_value_keeps_the_line_it_was_written_on() {
        // Given
        let ctx = ParseCtx::with_domain(Domain::Py);
        let line = OptionLine {
            line_index: 2,
            ..written("5")
        };

        // When
        let located = Located::<Percentage>::read(&line, &ctx).expect("reads");

        // Then
        assert_eq!(located.value, Percentage(5));
        assert_eq!(located.span(&ctx).map(|span| span.start.line), Some(3));
    }

    #[test]
    fn test_report_invalid_option_names_the_directive_and_option() {
        // Given
        let mut diagnostics = Diagnostics::default();

        // When
        report_invalid_option(
            &written("x"),
            "toctree",
            DiagnosticCode::ToctreeMaxdepthInvalid,
            "needs a whole number, found 'x'",
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

        // Then
        let found: Vec<_> = diagnostics.iter().collect();
        assert_eq!(found[0].code, DiagnosticCode::ToctreeMaxdepthInvalid);
        assert_eq!(
            found[0].message,
            "toctree: :option: needs a whole number, found 'x'"
        );
    }
}

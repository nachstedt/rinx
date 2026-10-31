//! The `:name: value` option lines any directive may open its body with, and
//! the diagnostic for the ones nobody claimed.
//!
//! Nothing here knows what any particular option *means*: [`scan_option_lines`]
//! splits a run of `:name: value` lines off a block without judging them —
//! the reader (`super::reader`) calls it on the lines its argument block ends
//! with — and [`report_unknown_options`] turns whatever the directive's own
//! parser handed back into diagnostics. Interpretation belongs to the
//! directive — [`super::table_options`] layers the five presentation options
//! the table directives share on top of this, and `super::math` reads
//! `:label:`/`:nowrap:`/`:class:` off the same scan.
//!
//! Split out of `table_options` once `.. math::` needed the scanning without
//! any of the table options, so the generic half stops being named after one
//! caller's construct.

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use rinx_ast::{Diagnostic, DiagnosticCode, Span, TokenKind, TokenModifiers};

use super::spec::{OptionSet, OptionSpec, OptionSupport, OptionValue};

/// One `:name: value` line, with the source text kept so a diagnostic can
/// quote the line exactly as the author wrote it.
pub(in crate::directives) struct OptionLine {
    pub name: String,
    pub value: String,
    pub raw: String,
    /// This line's index within the directive body, so a diagnostic about the
    /// option can point at the line the author wrote rather than at the
    /// directive as a whole.
    pub line_index: usize,
    /// Whether the value runs on below the option's own line, so its
    /// characters are no one range of it.
    pub continued: bool,
}

impl OptionLine {
    /// Exactly the characters of the option's `:name:` — up to the end of
    /// the line for one whose closing colon is missing — or `None` when it
    /// was read from no source line.
    pub(in crate::directives) fn name_span(&self, ctx: &ParseCtx<'_>) -> Option<Span> {
        let end = (self.name.chars().count() + 2).min(self.raw.chars().count());
        Some(
            ctx.position(self.line_index, 0)?
                .to(ctx.position(self.line_index, end)?),
        )
    }

    /// Exactly the characters of the value — `None` for an empty one, one
    /// continued onto the lines below, or one read from no source line.
    pub(in crate::directives) fn value_span(&self, ctx: &ParseCtx<'_>) -> Option<Span> {
        if self.continued || self.value.is_empty() {
            return None;
        }
        // `:name:`, then the blanks before the value.
        let marker = self.name.chars().count() + 2;
        let blanks = self
            .raw
            .chars()
            .skip(marker)
            .take_while(|c| c.is_whitespace())
            .count();
        let start = marker + blanks;
        let end = start + self.value.chars().count();
        Some(
            ctx.position(self.line_index, start)?
                .to(ctx.position(self.line_index, end)?),
        )
    }
}

/// Records each of `options`' `:name:` for an editor colouring it — see
/// [`rinx_ast::SyntaxTable`].
pub(in crate::directives) fn record_option_names(options: &[OptionLine], ctx: &ParseCtx<'_>) {
    for option in options {
        ctx.record_token(
            TokenKind::Option,
            TokenModifiers::NONE,
            option.name_span(ctx),
        );
    }
}

/// Splits the leading `:option:` lines off an already-unindented directive
/// body, returning them plus the index of the first line that isn't an option
/// (where the real body content starts). The option block is the run of lines
/// directly below the directive and ends at the first blank line.
///
/// A line whose closing colon is missing (`:oops`) is still returned, with the
/// whole remainder as its `name`, so that it reaches the caller's
/// unknown-option diagnostic rather than being silently swallowed as body.
///
/// An option's value may run over several lines: docutils parses a directive's
/// option block as a *field list*, and a field body continues on any following
/// line indented past the field marker. Since these lines arrive already
/// unindented, an option starts at column 0 and a continuation is simply a
/// line that is indented at all. Continuations are joined with a single space,
/// which is what a field body's text does when it is normalized — `CPython`'s
/// `pathlib.rst` wraps one `:alt:` across five lines, and without this the
/// last four would be read as directive content.
pub(in crate::directives) fn scan_option_lines(
    unindented_lines: &[String],
) -> (Vec<OptionLine>, usize) {
    let mut options: Vec<OptionLine> = Vec::new();
    let mut index = 0;
    while index < unindented_lines.len() {
        let raw_line = &unindented_lines[index];
        let line = raw_line.trim();
        // A blank line ends the option block for good: docutils reads options
        // only from the lines directly below the directive, so everything
        // after one is content — indented text (how a `.. code-block::
        // :dedent:` writes its first line) and `:role:`-looking lines (how a
        // code block showing roles writes its) alike.
        if line.is_empty() {
            while unindented_lines
                .get(index)
                .is_some_and(|blank| blank.trim().is_empty())
            {
                index += 1;
            }
            break;
        }
        // An indented line continues the option above it; with no option
        // above it, it is body content and the option block is over.
        if raw_line.starts_with(char::is_whitespace) {
            let Some(previous) = options.last_mut() else {
                break;
            };
            if !previous.value.is_empty() {
                previous.value.push(' ');
            }
            previous.value.push_str(line);
            previous.raw.push(' ');
            previous.raw.push_str(line);
            previous.continued = true;
            index += 1;
            continue;
        }
        let Some(rest) = line.strip_prefix(':') else {
            break;
        };
        let (name, value) = match rest.split_once(':') {
            Some((name, value)) => (name, value.trim()),
            None => (rest, ""),
        };
        options.push(OptionLine {
            name: name.to_string(),
            value: value.to_string(),
            raw: line.to_string(),
            line_index: index,
            continued: false,
        });
        index += 1;
    }
    (options, index)
}

/// Whether `line` opens with a field marker — `:name:` followed by whitespace
/// or the end of the line — by docutils' own `field_marker` pattern.
///
/// Stricter than [`scan_option_lines`]' leading-colon test, and needed where
/// content and options can share a block: `:pep:`634` -- text` starts with a
/// colon, but the backtick after the second one makes it a role, not a field.
pub(in crate::directives) fn is_field_marker(line: &str) -> bool {
    let Some(rest) = line.strip_prefix(':') else {
        return false;
    };
    let mut chars = rest.chars().peekable();
    if matches!(chars.peek(), None | Some(':' | ' ')) {
        return false;
    }
    let mut previous = ' ';
    while let Some(current) = chars.next() {
        match current {
            '\\' => {
                // An escaped character is part of the name, whatever it is.
                chars.next();
            }
            ':' => match chars.peek() {
                None | Some(' ' | '\t') => return previous != ' ',
                Some('`') => return false,
                Some(_) => {}
            },
            _ => {}
        }
        previous = current;
    }
    false
}

/// Reports every option line `directive` does not take, as
/// `directive.unknown-option` — one code whichever directive it is written
/// in, since the mistake is the same.
pub(in crate::directives) fn report_unknown_options(
    unrecognized: &[&OptionLine],
    directive: &str,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) {
    for line in unrecognized {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::DirectiveUnknownOption,
            format!(
                "Invalid or non-standard Sphinx {directive} option encountered: {}",
                line.raw
            ),
            ctx.line_span(line.line_index, &line.raw),
        ));
    }
}

/// Reads an option whose value is a percentage — `.. image::`'s `:scale:`
/// and a diagram's alike.
///
/// docutils' `directives.percentage` strips one trailing `%` and then demands
/// a non-negative integer, so `50` and `50%` are the same option and `-50` is
/// no option at all. Shared because two directives agreeing on one ambiguity
/// should not each own a copy of the answer.
pub(in crate::directives) fn parse_percentage(raw: &str) -> Option<u32> {
    let trimmed = raw.trim();
    let number = trimmed.strip_suffix('%').unwrap_or(trimmed).trim();
    number.parse::<u32>().ok()
}

/// A URI argument as docutils' `directives.uri` reads it: every unescaped
/// whitespace character removed — an argument wrapped onto the next line is
/// still one address — and an escaped one (`my\ logo.png`) kept as a single
/// space, with every other backslash escape resolved.
pub(in crate::directives) fn uri_argument(argument: &str) -> String {
    let mut parts = vec![String::new()];
    let mut chars = argument.chars().peekable();
    while let Some(character) = chars.next() {
        let current = parts.last_mut().expect("parts is never empty");
        if character == '\\' {
            match chars.next() {
                Some(escaped) if escaped.is_whitespace() => parts.push(String::new()),
                Some(escaped) => current.push(escaped),
                None => current.push(character),
            }
        } else if !character.is_whitespace() {
            current.push(character);
        }
    }
    parts.join(" ")
}

/// The options one directive takes, as an enum its parser matches on.
///
/// Implemented by [`directive_options!`], never by hand: the enum, its
/// spellings and its [`OptionSpec`]s come from one table, so a `match` on it
/// without a `_` arm makes an option that is listed but not read, or read but
/// not listed, fail to compile.
pub(in crate::directives) trait DirectiveOptions: Sized {
    /// The directive's own options.
    const OWN: &'static [OptionSpec<'static>];
    /// Its own options, then each group it shares — what its spec lists.
    const OPTIONS: OptionSet<'static>;
    /// Reads `line` as one of these options: its value parsed, reported when
    /// it does not read, or not one of them at all.
    fn read(
        line: &OptionLine,
        directive: &str,
        diagnostics: &mut Diagnostics,
        ctx: &ParseCtx<'_>,
    ) -> OptionRead<Self>;
}

/// What [`DirectiveOptions::read`] made of one option line.
#[derive(Debug, Clone, PartialEq)]
pub(in crate::directives) enum OptionRead<O> {
    /// One of the options, its value read.
    Read(O),
    /// One of the options, whose value did not read; reported.
    Refused,
    /// None of the options.
    NotTaken,
}

/// Declares one directive's option enum from a table of rows, then the
/// options the reference implementation takes and this build does not, then
/// after a `;` any shared groups as `Variant(GroupEnum)`.
///
/// A row is one of two kinds:
///
/// - `Variant(Type) => (["spelling", "alias"], Code, "doc")` — the value is
///   read into `Type`, an [`OptionValueType`], and the variant carries it;
///   one that does not read is reported under `Code`, or under
///   `directive.invalid-option-value` when the row names none. What
///   completion offers comes from the type.
/// - `Variant => (["spelling"], Value, "doc")` — a flag, or a value the
///   directive reads itself from the line handed over with the variant (a
///   filter, which needs the schema and its column); `Value` says what
///   completion offers.
///
/// An unsupported row, `_ => (["spelling"], Value, "doc", "advice")` with the
/// advice optional, has no variant, so no parser reads it:
/// [`classify_options`] reports it, with its advice, and completion still
/// knows the name.
///
/// A group is another enum declared by this macro with no groups of its own;
/// its options keep their own variant so a shared reader handles them once
/// for every directive using the group.
///
/// [`OptionValueType`]: super::option_value::OptionValueType
macro_rules! directive_options {
    (@advice) => {
        None
    };
    (@advice $advice:expr) => {
        Some($advice)
    };
    (@value [] [$kind:ident $(($choices:expr))?]) => {
        $crate::directives::spec::OptionValue::$kind $(($choices))?
    };
    (@value [$ty:ty] [$($code:ident)?]) => {
        <$ty as $crate::directives::option_value::OptionValueType>::VALUE
    };
    (@code) => {
        rinx_ast::DiagnosticCode::DirectiveInvalidOptionValue
    };
    (@code $code:ident) => {
        rinx_ast::DiagnosticCode::$code
    };
    (@read $variant:ident [] [$($second:tt)*] $line:ident $directive:ident $diagnostics:ident $ctx:ident) => {
        $crate::directives::options::OptionRead::Read(Self::$variant)
    };
    (@read $variant:ident [$ty:ty] [$($code:ident)?] $line:ident $directive:ident $diagnostics:ident $ctx:ident) => {
        match <$ty as $crate::directives::option_value::OptionValueType>::read($line, $ctx) {
            Ok(value) => $crate::directives::options::OptionRead::Read(Self::$variant(value)),
            Err(problem) => {
                $crate::directives::option_value::report_unread_value(
                    $line,
                    $directive,
                    directive_options!(@code $($code)?),
                    &problem,
                    $diagnostics,
                    $ctx,
                );
                $crate::directives::options::OptionRead::Refused
            }
        }
    };
    (
        $(#[doc = $doc:literal])*
        $vis:vis enum $name:ident {
            $( $(#[doc = $variant_doc:literal])*
               $variant:ident $(($ty:ty))? => ([$spelling:literal $(, $alias:literal)*], $( $second:ident $(($choices:expr))? , )? $help:literal) $(,)? )*
            $( _ => ([$u_spelling:literal $(, $u_alias:literal)*], $u_value:ident $(($u_choices:expr))?, $u_help:literal $(, $u_advice:expr)?) $(,)? )*
            $( ; $( $group:ident($group_type:ty) ),+ $(,)? )?
        }
    ) => {
        $(#[doc = $doc])*
        #[derive(Debug, Clone, PartialEq)]
        $vis enum $name {
            $( $(#[doc = $variant_doc])* $variant $(($ty))?, )*
            $( $( #[doc = concat!("One of the `", stringify!($group_type), "` options.")] $group($group_type), )+ )?
        }

        impl $crate::directives::options::DirectiveOptions for $name {
            const OWN: &'static [$crate::directives::spec::OptionSpec<'static>] = &[
                $( $crate::directives::spec::OptionSpec {
                    name: $spelling,
                    aliases: &[$($alias),*],
                    value: directive_options!(@value [$($ty)?] [$($second $(($choices))?)?]),
                    doc: $help,
                    support: $crate::directives::spec::OptionSupport::Supported,
                }, )*
                $( $crate::directives::spec::OptionSpec {
                    name: $u_spelling,
                    aliases: &[$($u_alias),*],
                    value: $crate::directives::spec::OptionValue::$u_value $(($u_choices))?,
                    doc: $u_help,
                    support: $crate::directives::spec::OptionSupport::Unsupported {
                        advice: directive_options!(@advice $($u_advice)?),
                    },
                }, )*
            ];
            const OPTIONS: $crate::directives::spec::OptionSet<'static> =
                $crate::directives::spec::OptionSet {
                    own: Self::OWN,
                    shared: &[
                        $( $( <$group_type as $crate::directives::options::DirectiveOptions>::OWN, )+ )?
                    ],
                };

            fn read(
                line: &$crate::directives::options::OptionLine,
                directive: &str,
                diagnostics: &mut $crate::diagnostics::Diagnostics,
                ctx: &$crate::context::ParseCtx<'_>,
            ) -> $crate::directives::options::OptionRead<Self> {
                let name = line.name.as_str();
                $(
                    if matches!(name, $spelling $(| $alias)*) {
                        return directive_options!(
                            @read $variant [$($ty)?] [$($second)?] line directive diagnostics ctx
                        );
                    }
                )*
                $( $(
                    match <$group_type as $crate::directives::options::DirectiveOptions>::read(
                        line, directive, diagnostics, ctx,
                    ) {
                        $crate::directives::options::OptionRead::Read(option) => {
                            return $crate::directives::options::OptionRead::Read(Self::$group(option));
                        }
                        $crate::directives::options::OptionRead::Refused => {
                            return $crate::directives::options::OptionRead::Refused;
                        }
                        $crate::directives::options::OptionRead::NotTaken => {}
                    }
                )+ )?
                // A table of only unsupported rows reads nothing at all.
                let _ = (name, directive, diagnostics, ctx);
                $crate::directives::options::OptionRead::NotTaken
            }
        }
    };
}
pub(in crate::directives) use directive_options;

/// The option lines `O` takes, each with its option — its value read — and
/// the line it was written on, in the order written.
///
/// A value that does not read is reported and left out. An option written
/// again under the same spelling is reported each further time and still
/// handed over, as is a flag written with a value. Two spellings of one
/// option (`:noindex:` and `:no-index:`) are not a repeat: docutils keys
/// options by spelling, and a page writing both for two Sphinx versions is
/// deliberate. Every other line is reported as unsupported when `O`
/// declares it so, as `directive.unknown-option` otherwise.
pub(in crate::directives) fn classify_options<'l, O: DirectiveOptions>(
    option_lines: &'l [OptionLine],
    directive: &str,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Vec<(O, &'l OptionLine)> {
    let mut known = Vec::new();
    let mut unknown = Vec::new();
    let mut written: Vec<&str> = Vec::new();
    for line in option_lines {
        let spec = declared_spec::<O>(&line.name);
        if spec.is_some_and(|spec| spec.support == OptionSupport::Supported) {
            if written.contains(&line.name.as_str()) {
                report_duplicate_option(line, directive, diagnostics, ctx);
            } else {
                written.push(&line.name);
            }
        }
        match O::read(line, directive, diagnostics, ctx) {
            OptionRead::Read(option) => {
                if spec.is_some_and(|spec| matches!(spec.value, OptionValue::Flag))
                    && !line.value.is_empty()
                {
                    report_flag_with_value(line, directive, diagnostics, ctx);
                }
                known.push((option, line));
            }
            OptionRead::Refused => {}
            OptionRead::NotTaken => match spec.map(|spec| spec.support) {
                Some(OptionSupport::Unsupported { advice }) => {
                    report_unsupported_option(line, directive, advice, diagnostics, ctx);
                }
                _ => unknown.push(line),
            },
        }
    }
    report_unknown_options(&unknown, directive, diagnostics, ctx);
    known
}

/// Reports an option written again, which docutils refuses; the line is still
/// read, so which one counts stays the directive's to decide.
fn report_duplicate_option(
    line: &OptionLine,
    directive: &str,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) {
    diagnostics.push(Diagnostic::at(
        DiagnosticCode::DirectiveDuplicateOption,
        format!("{directive}: :{}: is given more than once", line.name),
        ctx.line_span(line.line_index, &line.raw),
    ));
}

/// Reports a flag written with a value, which docutils refuses; the flag still
/// applies.
fn report_flag_with_value(
    line: &OptionLine,
    directive: &str,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) {
    diagnostics.push(Diagnostic::at(
        DiagnosticCode::DirectiveFlagWithValue,
        format!(
            "{directive}: :{}: takes no value, so '{}' was ignored",
            line.name, line.value
        ),
        ctx.line_span(line.line_index, &line.raw),
    ));
}

/// The option `line` is, read with nothing reported — for tests about a
/// table's spellings rather than its diagnostics.
#[cfg(test)]
pub(in crate::directives) fn read_quietly<O: DirectiveOptions>(line: &OptionLine) -> Option<O> {
    match O::read(
        line,
        "test",
        &mut Diagnostics::default(),
        &ParseCtx::with_domain(rinx_ast::Domain::Py),
    ) {
        OptionRead::Read(option) => Some(option),
        OptionRead::Refused | OptionRead::NotTaken => None,
    }
}

/// The option written as `name`, if `O` declares it at all.
fn declared_spec<O: DirectiveOptions>(name: &str) -> Option<&'static OptionSpec<'static>> {
    O::OPTIONS.iter().find(|spec| spec.is_spelled(name))
}

/// Reports an option the reference implementation takes and this build
/// does not, as `directive.unsupported-option` — not as unknown, since the
/// author wrote a valid name — with what to reach for instead, if anything.
pub(in crate::directives) fn report_unsupported_option(
    line: &OptionLine,
    directive: &str,
    advice: Option<&str>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) {
    let refusal = format!(
        "{directive}: option :{}: is not supported by this build and is ignored",
        line.name
    );
    let message = match advice {
        Some(advice) => format!("{refusal}; {advice}"),
        None => refusal,
    };
    diagnostics.push(Diagnostic::at(
        DiagnosticCode::DirectiveUnsupportedOption,
        message,
        ctx.line_span(line.line_index, &line.raw),
    ));
}

/// The names of every value of an enum, for an [`OptionValue::Choice`]
/// computed from the enum its parser reads the value into.
///
/// [`OptionValue::Choice`]: super::spec::OptionValue::Choice
pub(in crate::directives) fn choice_names<T: Copy>(
    values: &[T],
    name_of: fn(T) -> &'static str,
) -> Vec<&'static str> {
    values.iter().map(|value| name_of(*value)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::directives::spec::OptionValue;
    use rinx_ast::Domain;

    super::directive_options! {
        /// A group a test directive shares.
        enum SharedOption {
            /// A shared flag.
            Wide => (["wide"], Flag, "Makes it wide."),
        }
    }

    super::directive_options! {
        /// A test directive's options.
        enum SampleOption {
            /// One with an alias.
            LinkTypes => (["link_types", "link-types"], Text, "Which links."),
            /// One with a choice.
            Align => (["align"], Choice(|| vec!["left", "right"]), "Where it sits."),
            /// One read into a type, under a code of its own.
            Count(usize) => (["count"], TableDataIntegerInvalid, "How many."),
            /// One read into a type, under the shared code.
            Title(crate::directives::option_value::NonEmptyText) => (["title"], "Its title."),
            /// Another, which a written value can fail.
            Ratio(crate::directives::option_value::Percentage) => (["ratio"], "How much."),
            _ => (["class"], Classes, "Classes for the output.", "write it on a container"),
            _ => (["name"], Text, "A label for the output.");
            Shared(SharedOption),
        }
    }

    /// The option written as `name`, with no value.
    fn named(name: &str) -> Option<SampleOption> {
        read_quietly(&OptionLine {
            name: name.to_string(),
            value: String::new(),
            raw: format!(":{name}:"),
            line_index: 0,
            continued: false,
        })
    }

    #[test]
    fn test_read_takes_every_spelling_and_each_group() {
        // Given / When / Then
        assert_eq!(named("link-types"), Some(SampleOption::LinkTypes));
        assert_eq!(named("link_types"), Some(SampleOption::LinkTypes));
        assert_eq!(
            named("wide"),
            Some(SampleOption::Shared(SharedOption::Wide))
        );
        assert_eq!(named("other"), None);
        assert_eq!(named("class"), None);
    }

    #[test]
    fn test_options_list_the_own_options_first_then_each_group() {
        // Given / When
        let names: Vec<&str> = SampleOption::OPTIONS.iter().map(|spec| spec.name).collect();

        // Then
        assert_eq!(
            names,
            [
                "link_types",
                "align",
                "count",
                "title",
                "ratio",
                "class",
                "name",
                "wide"
            ]
        );
        assert_eq!(SampleOption::OWN[0].name, "link_types");
        assert_eq!(SampleOption::OWN[0].aliases, ["link-types"]);
    }

    #[test]
    fn test_a_typed_row_takes_its_value_kind_from_its_type() {
        // Given / When / Then
        assert!(matches!(SampleOption::OWN[2].value, OptionValue::Integer));
        assert!(matches!(SampleOption::OWN[3].value, OptionValue::Text));
    }

    #[test]
    fn test_classify_options_hands_over_a_typed_value_parsed() {
        // Given
        let lines: Vec<String> = [":count: 3", ":title: Hello"].map(String::from).to_vec();
        let (option_lines, _) = scan_option_lines(&lines);
        let mut diagnostics = Diagnostics::default();

        // When
        let known = classify_options::<SampleOption>(
            &option_lines,
            "sample",
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

        // Then
        assert!(diagnostics.is_empty());
        let options: Vec<SampleOption> = known.into_iter().map(|(option, _)| option).collect();
        assert_eq!(
            options,
            [
                SampleOption::Count(3),
                SampleOption::Title(crate::directives::option_value::NonEmptyText(
                    "Hello".to_string()
                )),
            ]
        );
    }

    #[test]
    fn test_classify_options_reports_a_bad_value_and_leaves_it_out() {
        // Given — one under the row's own code, one under the shared code,
        // one missing
        let lines: Vec<String> = [":count: many", ":ratio: half", ":title:", ":align: left"]
            .map(String::from)
            .to_vec();
        let (option_lines, _) = scan_option_lines(&lines);
        let mut diagnostics = Diagnostics::default();

        // When
        let known = classify_options::<SampleOption>(
            &option_lines,
            "sample",
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

        // Then
        assert_eq!(known.len(), 1);
        assert_eq!(known[0].0, SampleOption::Align);
        let codes: Vec<DiagnosticCode> = diagnostics.iter().map(|d| d.code).collect();
        assert_eq!(
            codes,
            [
                DiagnosticCode::TableDataIntegerInvalid,
                DiagnosticCode::DirectiveInvalidOptionValue,
                DiagnosticCode::DirectiveEmptyOptionValue,
            ]
        );
        assert!(
            diagnostics
                .iter()
                .next()
                .is_some_and(|d| d.message.starts_with("sample: :count: "))
        );
    }

    #[test]
    fn test_classify_options_reports_a_repeated_option_and_hands_every_one_over() {
        // Given — `:link_types:` twice, and its alias once, which is no repeat
        let lines: Vec<String> = [":link_types: a", ":link-types: b", ":link_types: c"]
            .map(String::from)
            .to_vec();
        let (option_lines, _) = scan_option_lines(&lines);
        let mut diagnostics = Diagnostics::default();

        // When
        let known = classify_options::<SampleOption>(
            &option_lines,
            "sample",
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

        // Then
        assert_eq!(known.len(), 3);
        let found: Vec<_> = diagnostics.iter().collect();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].code, DiagnosticCode::DirectiveDuplicateOption);
        assert_eq!(found[0].span.map(|span| span.start.line), Some(3));
    }

    #[test]
    fn test_classify_options_reports_a_value_written_on_a_flag_and_keeps_the_flag() {
        // Given
        let lines: Vec<String> = [":wide: yes"].map(String::from).to_vec();
        let (option_lines, _) = scan_option_lines(&lines);
        let mut diagnostics = Diagnostics::default();

        // When
        let known = classify_options::<SampleOption>(
            &option_lines,
            "sample",
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

        // Then
        assert_eq!(known[0].0, SampleOption::Shared(SharedOption::Wide));
        let found: Vec<_> = diagnostics.iter().collect();
        assert_eq!(found[0].code, DiagnosticCode::DirectiveFlagWithValue);
        assert!(found[0].message.contains("'yes'"), "{}", found[0].message);
    }

    #[test]
    fn test_read_answers_not_taken_for_a_name_of_no_row() {
        // Given
        let line = OptionLine {
            name: "bogus".to_string(),
            value: String::new(),
            raw: ":bogus:".to_string(),
            line_index: 0,
            continued: false,
        };

        // When
        let read = SampleOption::read(
            &line,
            "sample",
            &mut Diagnostics::default(),
            &ParseCtx::with_domain(Domain::Py),
        );

        // Then
        assert_eq!(read, OptionRead::NotTaken);
    }

    #[test]
    fn test_classify_options_keeps_the_known_and_reports_the_rest() {
        // Given
        let lines: Vec<String> = [":align: left", ":bogus: x"].map(String::from).to_vec();
        let (option_lines, _) = scan_option_lines(&lines);
        let mut diagnostics = Diagnostics::default();

        // When
        let known = classify_options::<SampleOption>(
            &option_lines,
            "sample",
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

        // Then
        assert_eq!(known.len(), 1);
        assert_eq!(known[0].0, SampleOption::Align);
        let codes: Vec<DiagnosticCode> = diagnostics.iter().map(|d| d.code).collect();
        assert_eq!(codes, [DiagnosticCode::DirectiveUnknownOption]);
    }

    #[test]
    fn test_classify_options_reports_an_unsupported_option_with_its_advice() {
        // Given
        let lines: Vec<String> = [":class: wide"].map(String::from).to_vec();
        let (option_lines, _) = scan_option_lines(&lines);
        let mut diagnostics = Diagnostics::default();

        // When
        let known = classify_options::<SampleOption>(
            &option_lines,
            "sample",
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

        // Then
        assert!(known.is_empty());
        let found: Vec<_> = diagnostics.iter().collect();
        assert_eq!(found[0].code, DiagnosticCode::DirectiveUnsupportedOption);
        assert!(found[0].message.ends_with("; write it on a container"));
    }

    #[test]
    fn test_declared_spec_tells_supported_unsupported_and_undeclared_apart() {
        // Given / When / Then
        assert_eq!(
            declared_spec::<SampleOption>("wide").map(|spec| spec.support),
            Some(OptionSupport::Supported)
        );
        assert_eq!(
            declared_spec::<SampleOption>("class").map(|spec| spec.support),
            Some(OptionSupport::Unsupported {
                advice: Some("write it on a container")
            })
        );
        assert_eq!(
            declared_spec::<SampleOption>("name").map(|spec| spec.support),
            Some(OptionSupport::Unsupported { advice: None })
        );
        assert_eq!(
            declared_spec::<SampleOption>("bogus").map(|spec| spec.support),
            None
        );
    }

    #[test]
    fn test_choice_names_lists_every_value_by_its_name() {
        // Given / When / Then
        assert_eq!(
            choice_names(rinx_ast::Chevron::ALL, rinx_ast::Chevron::as_str),
            ["right-down", "down-up"]
        );
    }

    #[test]
    fn test_report_unsupported_option_names_the_option() {
        // Given
        let lines: Vec<String> = vec![":class: wide".to_string()];
        let (option_lines, _) = scan_option_lines(&lines);
        let mut diagnostics = Diagnostics::default();

        // When
        report_unsupported_option(
            &option_lines[0],
            "note",
            Some("write it on a container instead"),
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

        // Then
        let found: Vec<_> = diagnostics.iter().collect();
        assert_eq!(found[0].code, DiagnosticCode::DirectiveUnsupportedOption);
        assert!(found[0].message.contains(":class:"));
        assert!(
            found[0]
                .message
                .ends_with("; write it on a container instead")
        );
    }

    #[test]
    fn test_uri_argument_drops_unescaped_whitespace() {
        // Given / When / Then — a wrapped address, and a stray space
        assert_eq!(
            super::uri_argument("https://example.com/a/\nlong/path"),
            "https://example.com/a/long/path"
        );
        assert_eq!(super::uri_argument("my logo.png"), "mylogo.png");
    }

    #[test]
    fn test_uri_argument_keeps_an_escaped_space() {
        // Given / When / Then
        assert_eq!(super::uri_argument("my\\ logo.png"), "my logo.png");
        assert_eq!(super::uri_argument("a\\_b"), "a_b");
    }

    fn lines(raw: &[&str]) -> Vec<String> {
        raw.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn test_scan_option_lines_splits_name_and_value() {
        // Given
        let body = lines(&[":header-rows: 1", ":widths: 30 70", "", "* - Cell"]);

        // When
        let (options, body_start) = scan_option_lines(&body);

        // Then
        assert_eq!(options.len(), 2);
        assert_eq!(options[0].name, "header-rows");
        assert_eq!(options[0].value, "1");
        assert_eq!(options[1].name, "widths");
        assert_eq!(options[1].value, "30 70");
        assert_eq!(body_start, 3);
    }

    /// The span `value_span` gives the first option of `body`, read with
    /// no origin to rebase onto.
    fn first_value_span(body: &[&str]) -> Option<Span> {
        let (options, _) = scan_option_lines(&lines(body));
        options[0].value_span(&ParseCtx::with_domain(Domain::Py))
    }

    #[test]
    fn test_value_span_is_the_value_alone() {
        // When — `install` past `:name:` and two blanks
        let span = first_value_span(&[":name:  install"]);

        // Then
        assert_eq!(
            span,
            Some(Span::new(
                rinx_ast::Position::new(1, 9),
                rinx_ast::Position::new(1, 16)
            ))
        );
    }

    #[test]
    fn test_value_span_is_none_for_an_empty_or_continued_value() {
        // When / Then
        assert_eq!(first_value_span(&[":name:"]), None);
        assert_eq!(first_value_span(&[":name: first", "   second"]), None);
    }

    /// The span `name_span` gives the first option of `body`.
    fn first_name_span(body: &[&str]) -> Option<Span> {
        let (options, _) = scan_option_lines(&lines(body));
        options[0].name_span(&ParseCtx::with_domain(Domain::Py))
    }

    #[test]
    fn test_name_span_is_the_name_and_its_colons() {
        // When
        let span = first_name_span(&[":name:  install"]);

        // Then
        assert_eq!(
            span,
            Some(Span::new(
                rinx_ast::Position::new(1, 1),
                rinx_ast::Position::new(1, 7)
            ))
        );
    }

    #[test]
    fn test_name_span_stops_at_the_line_end_without_a_closing_colon() {
        // When
        let span = first_name_span(&[":oops"]);

        // Then
        assert_eq!(span.map(|span| span.end.column), Some(6));
    }

    #[test]
    fn test_scan_option_lines_marks_a_value_continued_below() {
        // When
        let (options, _) = scan_option_lines(&lines(&[":alt: one", "   two", ":name: x"]));

        // Then
        assert!(options[0].continued);
        assert!(!options[1].continued);
    }

    #[test]
    fn test_scan_option_lines_accepts_a_value_with_no_space_after_the_colon() {
        // Given
        let body = lines(&[":header-rows:1"]);

        // When
        let (options, _) = scan_option_lines(&body);

        // Then
        assert_eq!(options[0].name, "header-rows");
        assert_eq!(options[0].value, "1");
    }

    #[test]
    fn test_scan_option_lines_treats_a_flag_option_as_an_empty_value() {
        // Given
        let body = lines(&[":keepspace:"]);

        // When
        let (options, _) = scan_option_lines(&body);

        // Then
        assert_eq!(options[0].name, "keepspace");
        assert_eq!(options[0].value, "");
    }

    #[test]
    fn test_scan_option_lines_keeps_an_unterminated_option_line() {
        // Given
        let body = lines(&[":oops"]);

        // When
        let (options, body_start) = scan_option_lines(&body);

        // Then
        assert_eq!(options.len(), 1);
        assert_eq!(options[0].name, "oops");
        assert_eq!(options[0].raw, ":oops");
        assert_eq!(body_start, 1);
    }

    #[test]
    fn test_scan_option_lines_stops_at_the_first_body_line() {
        // Given
        let body = lines(&[":widths: auto", "Apple, Red", ":not-an-option: x"]);

        // When
        let (options, body_start) = scan_option_lines(&body);

        // Then
        assert_eq!(options.len(), 1);
        assert_eq!(body_start, 1);
    }

    #[test]
    fn test_scan_option_lines_joins_a_wrapped_value() {
        // Given — CPython's `pathlib.rst` writes an `:alt:` this way
        let body = lines(&[
            ":alt: Inheritance diagram showing",
            "      the pathlib classes",
        ]);

        // When
        let (options, body_start) = scan_option_lines(&body);

        // Then
        assert_eq!(options.len(), 1);
        assert_eq!(
            options[0].value,
            "Inheritance diagram showing the pathlib classes"
        );
        assert_eq!(body_start, 2);
    }

    #[test]
    fn test_scan_option_lines_joins_a_value_wrapped_over_several_lines() {
        // Given
        let body = lines(&[":alt: one", "      two", "      three"]);

        // When
        let (options, _) = scan_option_lines(&body);

        // Then
        assert_eq!(options[0].value, "one two three");
    }

    #[test]
    fn test_scan_option_lines_continues_a_value_that_started_empty() {
        // Given — the whole value written on the wrapped line
        let body = lines(&[":alt:", "      the only text"]);

        // When
        let (options, _) = scan_option_lines(&body);

        // Then — no leading space from joining onto nothing
        assert_eq!(options[0].value, "the only text");
    }

    #[test]
    fn test_scan_option_lines_ends_the_option_block_at_a_blank_line() {
        // Given — a blank line ends the field body, so the indented line below
        // it is directive content (a `.. code-block:: :dedent:` writes this)
        let body = lines(&[":dedent: 2", "", "     x = 1"]);

        // When
        let (options, body_start) = scan_option_lines(&body);

        // Then
        assert_eq!(options.len(), 1);
        assert_eq!(options[0].value, "2");
        assert_eq!(body_start, 2);
    }

    #[test]
    fn test_scan_option_lines_reads_no_option_after_the_blank_line_opening_the_content() {
        // Given — docutils reads options only from the lines directly below
        // the directive, so after a blank line a `:role:`-looking line is
        // content: a code block showing roles writes exactly this
        let body = lines(&["", ":ref:`label`", ":term:`word`"]);

        // When
        let (options, body_start) = scan_option_lines(&body);

        // Then
        assert!(options.is_empty());
        assert_eq!(body_start, 1);
    }

    #[test]
    fn test_scan_option_lines_reads_no_option_after_the_blank_line_ending_the_options() {
        // Given
        let body = lines(&[":caption: Roles", "", ":ref:`label`"]);

        // When
        let (options, body_start) = scan_option_lines(&body);

        // Then
        assert_eq!(options.len(), 1);
        assert_eq!(options[0].name, "caption");
        assert_eq!(body_start, 2);
    }

    #[test]
    fn test_scan_option_lines_stops_at_an_indented_line_with_no_option_above_it() {
        // Given
        let body = lines(&["   indented body content"]);

        // When
        let (options, body_start) = scan_option_lines(&body);

        // Then
        assert!(options.is_empty());
        assert_eq!(body_start, 0);
    }

    #[test]
    fn test_scan_option_lines_quotes_the_whole_wrapped_line_in_raw() {
        // Given — `raw` is what an unknown-option diagnostic echoes back
        let body = lines(&[":bogus: one", "      two"]);

        // When
        let (options, _) = scan_option_lines(&body);

        // Then
        assert_eq!(options[0].raw, ":bogus: one two");
    }

    #[test]
    fn test_scan_option_lines_returns_nothing_for_an_empty_body() {
        // Given
        let body: Vec<String> = Vec::new();

        // When
        let (options, body_start) = scan_option_lines(&body);

        // Then
        assert!(options.is_empty());
        assert_eq!(body_start, 0);
    }

    #[test]
    fn test_report_unknown_options_quotes_the_source_line() {
        // Given
        let body = lines(&[":bogus: value"]);
        let (option_lines, _) = scan_option_lines(&body);
        let unrecognized: Vec<&OptionLine> = option_lines.iter().collect();
        let mut diagnostics = Diagnostics::default();

        // When
        report_unknown_options(
            &unrecognized,
            "list-table",
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

        // Then
        assert_eq!(diagnostics.len(), 1);
        assert!(
            diagnostics[0].message.contains(
                "Invalid or non-standard Sphinx list-table option encountered: :bogus: value"
            ),
            "{}",
            diagnostics[0].message
        );
    }

    #[test]
    fn test_report_unknown_options_uses_one_code_for_every_directive() {
        // Given
        let body = lines(&[":bogus:"]);
        let (option_lines, _) = scan_option_lines(&body);
        let unrecognized: Vec<&OptionLine> = option_lines.iter().collect();
        let mut diagnostics = Diagnostics::default();

        // When
        report_unknown_options(
            &unrecognized,
            "toctree",
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

        // Then
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, DiagnosticCode::DirectiveUnknownOption);
    }

    #[test]
    fn test_parse_percentage_reads_a_bare_integer() {
        // Given / When / Then
        assert_eq!(parse_percentage("50"), Some(50));
    }

    #[test]
    fn test_parse_percentage_strips_one_trailing_percent_sign() {
        // Given — docutils' `directives.percentage` accepts both spellings of
        // the same option
        assert_eq!(parse_percentage("50%"), parse_percentage("50"));
    }

    #[test]
    fn test_parse_percentage_ignores_surrounding_whitespace() {
        // Given / When / Then
        assert_eq!(parse_percentage("  75 % "), Some(75));
    }

    #[test]
    fn test_parse_percentage_accepts_zero() {
        // Given — zero is a real percentage, unlike a depth limit's zero
        assert_eq!(parse_percentage("0"), Some(0));
    }

    #[test]
    fn test_parse_percentage_refuses_a_negative_number() {
        // Given — docutils demands a non-negative integer
        assert_eq!(parse_percentage("-50"), None);
    }

    #[test]
    fn test_parse_percentage_refuses_a_fraction() {
        // Given / When / Then
        assert_eq!(parse_percentage("50.5"), None);
    }

    #[test]
    fn test_parse_percentage_refuses_text() {
        // Given / When / Then
        assert_eq!(parse_percentage("half"), None);
        assert_eq!(parse_percentage(""), None);
        assert_eq!(parse_percentage("%"), None);
    }

    #[test]
    fn test_is_field_marker_accepts_an_option() {
        // Given / When / Then
        assert!(is_field_marker(":collapsible:"));
        assert!(is_field_marker(":collapsible: open"));
        assert!(is_field_marker(":class: a b"));
    }

    #[test]
    fn test_is_field_marker_refuses_a_role() {
        // Given / When / Then
        assert!(!is_field_marker(
            ":pep:`634` -- Structural Pattern Matching"
        ));
        assert!(!is_field_marker(":ref:`label`"));
    }

    #[test]
    fn test_is_field_marker_refuses_text_and_malformed_markers() {
        // Given / When / Then
        assert!(!is_field_marker("Plain text"));
        assert!(!is_field_marker(":: x"));
        assert!(!is_field_marker(": x:"));
        assert!(!is_field_marker(":name :"));
        assert!(!is_field_marker(":unclosed"));
    }

    #[test]
    fn test_is_field_marker_accepts_an_escaped_colon_in_the_name() {
        // Given / When / Then
        assert!(is_field_marker(r":a\:b: value"));
    }
}

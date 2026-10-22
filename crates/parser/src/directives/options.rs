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

use std::num::NonZeroUsize;

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use rinx_ast::{Diagnostic, DiagnosticCode};

use super::spec::{OptionSpec, OptionSupport};

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

/// Reads an option whose value is a depth limit: a positive integer limits
/// the depth, and Sphinx's documented "unlimited" spelling — zero or a
/// negative number — clears it rather than being treated as a real limit.
///
/// Shared by `.. toctree::`'s `:maxdepth:` and `.. contents::`'s `:depth:`,
/// which agree on this exact ambiguity; `directive` and `option` name the
/// caller's own directive and option spelling for the diagnostic message, and
/// `code` lets each caller keep its own stable, `.. noqa:`-suppressible code.
pub(in crate::directives) fn parse_positive_depth(
    line: &OptionLine,
    directive: &str,
    option: &str,
    code: DiagnosticCode,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<NonZeroUsize> {
    match line.value.trim().parse::<isize>() {
        Ok(value) if value <= 0 => None,
        Ok(value) => NonZeroUsize::new(usize::try_from(value).unwrap_or(0)),
        Err(_) => {
            diagnostics.push(Diagnostic::at(
                code,
                format!(
                    "A {directive} :{option}: option needs a whole number: {}",
                    line.raw
                ),
                ctx.line_span(line.line_index, &line.raw),
            ));
            None
        }
    }
}

/// Reads an option whose value is a percentage — `.. image::`'s `:scale:`
/// and a diagram's alike.
///
/// docutils' `directives.percentage` strips one trailing `%` and then demands
/// a non-negative integer, so `50` and `50%` are the same option and `-50` is
/// no option at all. Shared for the reason [`parse_positive_depth`] is: two
/// directives agreeing on one ambiguity should not each own a copy of the
/// answer.
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
pub(in crate::directives) trait DirectiveOptions: Sized + Copy {
    /// The directive's own options.
    const OWN: &'static [OptionSpec];
    /// Its own options, then each group it shares.
    const GROUPS: &'static [&'static [OptionSpec]];
    /// The option written as `name`, if the directive takes it.
    fn from_name(name: &str) -> Option<Self>;
}

/// Declares one directive's option enum from a table of rows
/// `Variant => (["spelling", "alias"], Value, "doc")`, then the options the
/// reference implementation takes and this build does not, as rows
/// `_ => (["spelling"], Value, "doc", "advice")` with the advice optional,
/// followed after a `;` by any shared groups as `Variant(GroupEnum)`.
///
/// An unsupported row has no variant, so no parser reads it:
/// [`classify_options`] reports it, with its advice, and completion still
/// knows the name.
///
/// Generates the enum, its [`DirectiveOptions`] — the lookup by spelling and
/// the [`OptionSpec`]s completion and documentation read — from those rows
/// alone. A group is another enum declared by this macro with no groups of
/// its own; its options keep their own variant so a shared reader handles
/// them once for every directive using the group.
macro_rules! directive_options {
    (@advice) => {
        None
    };
    (@advice $advice:expr) => {
        Some($advice)
    };
    (
        $(#[doc = $doc:literal])*
        $vis:vis enum $name:ident {
            $( $(#[doc = $variant_doc:literal])*
               $variant:ident => ([$($spelling:literal),+], $value:ident $(($choices:expr))?, $help:literal) $(,)? )*
            $( _ => ([$($u_spelling:literal),+], $u_value:ident $(($u_choices:expr))?, $u_help:literal $(, $u_advice:expr)?) $(,)? )*
            $( ; $( $group:ident($group_type:ty) ),+ $(,)? )?
        }
    ) => {
        $(#[doc = $doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        $vis enum $name {
            $( $(#[doc = $variant_doc])* $variant, )*
            $( $( #[doc = concat!("One of the `", stringify!($group_type), "` options.")] $group($group_type), )+ )?
        }

        impl $crate::directives::options::DirectiveOptions for $name {
            const OWN: &'static [$crate::directives::spec::OptionSpec] = &[
                $( $crate::directives::spec::OptionSpec {
                    names: &[$($spelling),+],
                    value: $crate::directives::spec::OptionValue::$value $(($choices))?,
                    doc: $help,
                    support: $crate::directives::spec::OptionSupport::Supported,
                }, )*
                $( $crate::directives::spec::OptionSpec {
                    names: &[$($u_spelling),+],
                    value: $crate::directives::spec::OptionValue::$u_value $(($u_choices))?,
                    doc: $u_help,
                    support: $crate::directives::spec::OptionSupport::Unsupported {
                        advice: directive_options!(@advice $($u_advice)?),
                    },
                }, )*
            ];
            const GROUPS: &'static [&'static [$crate::directives::spec::OptionSpec]] = &[
                Self::OWN,
                $( $( <$group_type as $crate::directives::options::DirectiveOptions>::OWN, )+ )?
            ];

            fn from_name(name: &str) -> Option<Self> {
                let spellings: &[(&str, Self)] = &[ $( $( ($spelling, Self::$variant), )+ )* ];
                if let Some((_, option)) = spellings.iter().find(|(spelling, _)| *spelling == name) {
                    return Some(*option);
                }
                $( $(
                    if let Some(option) =
                        <$group_type as $crate::directives::options::DirectiveOptions>::from_name(name)
                    {
                        return Some(Self::$group(option));
                    }
                )+ )?
                None
            }
        }
    };
}
pub(in crate::directives) use directive_options;

/// The option lines `O` takes, each with its option; every other line is
/// reported as `directive.unknown-option` under `directive`'s name.
pub(in crate::directives) fn classify_options<'l, O: DirectiveOptions>(
    option_lines: &'l [OptionLine],
    directive: &str,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Vec<(O, &'l OptionLine)> {
    let mut known = Vec::new();
    let mut unknown = Vec::new();
    for line in option_lines {
        if let Some(option) = O::from_name(&line.name) {
            known.push((option, line));
        } else if let Some(OptionSupport::Unsupported { advice }) =
            declared_support::<O>(&line.name)
        {
            report_unsupported_option(line, directive, advice, diagnostics, ctx);
        } else {
            unknown.push(line);
        }
    }
    report_unknown_options(&unknown, directive, diagnostics, ctx);
    known
}

/// How the option written as `name` is supported, if `O` declares it at all.
fn declared_support<O: DirectiveOptions>(name: &str) -> Option<OptionSupport> {
    O::GROUPS
        .iter()
        .flat_map(|group| group.iter())
        .find(|spec| spec.names.contains(&name))
        .map(|spec| spec.support)
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
            _ => (["class"], Classes, "Classes for the output.", "write it on a container"),
            _ => (["name"], Text, "A label for the output.");
            Shared(SharedOption),
        }
    }

    #[test]
    fn test_from_name_reads_every_spelling_and_each_group() {
        // Given / When / Then
        assert_eq!(
            SampleOption::from_name("link-types"),
            Some(SampleOption::LinkTypes)
        );
        assert_eq!(
            SampleOption::from_name("link_types"),
            Some(SampleOption::LinkTypes)
        );
        assert_eq!(
            SampleOption::from_name("wide"),
            Some(SampleOption::Shared(SharedOption::Wide))
        );
        assert_eq!(SampleOption::from_name("other"), None);
        assert_eq!(SampleOption::from_name("class"), None);
    }

    #[test]
    fn test_groups_list_the_own_options_first_then_each_group() {
        // Given / When
        let names: Vec<&str> = SampleOption::GROUPS
            .iter()
            .flat_map(|group| group.iter())
            .map(|spec| spec.names[0])
            .collect();

        // Then
        assert_eq!(names, ["link_types", "align", "class", "name", "wide"]);
        assert_eq!(SampleOption::OWN[0].names, ["link_types", "link-types"]);
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
    fn test_declared_support_tells_supported_unsupported_and_undeclared_apart() {
        // Given / When / Then
        assert_eq!(
            declared_support::<SampleOption>("wide"),
            Some(OptionSupport::Supported)
        );
        assert_eq!(
            declared_support::<SampleOption>("class"),
            Some(OptionSupport::Unsupported {
                advice: Some("write it on a container")
            })
        );
        assert_eq!(
            declared_support::<SampleOption>("name"),
            Some(OptionSupport::Unsupported { advice: None })
        );
        assert_eq!(declared_support::<SampleOption>("bogus"), None);
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

    fn option_line(name: &str, value: &str) -> OptionLine {
        OptionLine {
            name: name.to_string(),
            value: value.to_string(),
            raw: format!(":{name}: {value}"),
            line_index: 0,
        }
    }

    #[test]
    fn test_parse_positive_depth_reads_a_positive_value() {
        // Given
        let line = option_line("depth", "2");
        let mut diagnostics = Diagnostics::default();

        // When
        let depth = parse_positive_depth(
            &line,
            "contents",
            "depth",
            DiagnosticCode::ContentsDepthInvalid,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

        // Then
        assert_eq!(depth, NonZeroUsize::new(2));
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_parse_positive_depth_treats_zero_as_unlimited() {
        // Given
        let line = option_line("depth", "0");
        let mut diagnostics = Diagnostics::default();

        // When
        let depth = parse_positive_depth(
            &line,
            "contents",
            "depth",
            DiagnosticCode::ContentsDepthInvalid,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

        // Then
        assert_eq!(depth, None);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_parse_positive_depth_treats_a_negative_value_as_unlimited() {
        // Given — Sphinx's documented spelling of "no limit".
        let line = option_line("maxdepth", "-1");
        let mut diagnostics = Diagnostics::default();

        // When
        let depth = parse_positive_depth(
            &line,
            "toctree",
            "maxdepth",
            DiagnosticCode::ToctreeMaxdepthInvalid,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

        // Then
        assert_eq!(depth, None);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_parse_positive_depth_diagnoses_a_non_numeric_value_under_the_callers_code() {
        // Given
        let line = option_line("depth", "deep");
        let mut diagnostics = Diagnostics::default();

        // When
        let depth = parse_positive_depth(
            &line,
            "contents",
            "depth",
            DiagnosticCode::ContentsDepthInvalid,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

        // Then
        assert_eq!(depth, None);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics.entries()[0].code,
            DiagnosticCode::ContentsDepthInvalid
        );
        assert!(diagnostics.entries()[0].message.contains(":depth:"));
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

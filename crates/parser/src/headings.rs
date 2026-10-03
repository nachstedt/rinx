use super::inline::{SourceMap, parse_inline_text_mapped};
use crate::context::{ParseCtx, SectionTitles};
use crate::diagnostics::Diagnostics;
use crate::width::column_width;
use rinx_ast::{Diagnostic, DiagnosticCode, Node};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AdornmentStyle {
    Underline,
    Overline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Adornment {
    pub(super) character: char,
    pub(super) style: AdornmentStyle,
}

/// Checks whether `line` is a valid section adornment: a non-empty run of a
/// *single* repeated ASCII punctuation character (e.g. `=====`, `-----`,
/// `+++++`). Mixed-punctuation lines such as a grid-table border
/// (`+------+------+`) are deliberately rejected, matching the RST spec's
/// definition of section adornments and mirroring `try_parse_transition`'s
/// single-character rule — this is what keeps a header-less grid table from
/// being mistaken for an overline/underline heading.
pub(super) fn is_section_adornment(line: &str) -> bool {
    let mut chars = line.chars();
    match chars.next() {
        Some(first) if first.is_ascii_punctuation() => chars.all(|c| c == first),
        _ => false,
    }
}

/// The shortest adornment that may still carry a title wider than itself.
///
/// docutils' own threshold: below it, a short run of punctuation under a line
/// of prose is far likelier to be something else than a heading the author
/// mis-drew, so the pair degrades to ordinary text.
const MIN_TOLERATED_ADORNMENT: usize = 4;

/// What was wrong with a heading that is a heading nonetheless.
///
/// One variant at most per heading: docutils stops at a mismatch before it
/// measures the title, so the two never apply together.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum HeadingProblem {
    /// The adornment is shorter than the title's display width. docutils
    /// accepts such a heading and warns, rather than rejecting it, as long as
    /// the adornment is at least [`MIN_TOLERATED_ADORNMENT`] characters.
    AdornmentTooShort,
    /// The overline and the underline differ in character or length. docutils
    /// drops the block with an error; this build keeps the heading the author
    /// evidently meant, with the overline's style.
    OverlineMismatch,
}

/// A heading recognised at some line, and what was wrong with it.
///
/// A named struct rather than a tuple because the last field answers a
/// different question from the first three: they say *what* was parsed, it
/// says what to report about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DetectedHeading {
    /// Lines the heading occupies: 3 for the overlined form, 2 otherwise.
    pub(super) consumed: usize,
    pub(super) adornment: Adornment,
    pub(super) text: String,
    pub(super) problem: Option<HeadingProblem>,
}

/// An overline that starts no heading, which docutils reports and this build
/// leaves as text.
///
/// Text rather than a heading because neither shape says what was meant: a
/// rule written directly above prose is as likely a transition missing its
/// blank line as a title missing its underline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MalformedOverline {
    /// A title follows the overline, but no matching underline follows the
    /// title — docutils' "Missing matching underline for section title
    /// overline", or "Incomplete section title" at the end of the input.
    MissingUnderline,
    /// Another adornment follows directly — docutils' "Invalid section title
    /// or transition marker".
    AdornmentWithoutTitle,
}

/// How an adornment measures up against the title it adorns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AdornmentFit {
    /// At least as wide as the title.
    Covers,
    /// Narrower than the title, but long enough that docutils still makes a
    /// heading of the pair and warns.
    TooShort,
    /// Narrower than the title and shorter than [`MIN_TOLERATED_ADORNMENT`]:
    /// not a heading at all.
    Refused,
}

impl AdornmentFit {
    /// The problem to report about a heading adorned this way, or `Err` when
    /// the adornment makes no heading.
    const fn problem(self) -> Result<Option<HeadingProblem>, ()> {
        match self {
            Self::Covers => Ok(None),
            Self::TooShort => Ok(Some(HeadingProblem::AdornmentTooShort)),
            Self::Refused => Err(()),
        }
    }
}

/// Decides whether `adornment` may underline a title of `text`, and whether
/// it is too short to do so cleanly.
///
/// The title is measured in **display columns**, never in bytes: an author
/// draws the underline under what they see, so a byte comparison rejects every
/// heading whose title holds a non-ASCII character — which is what made the
/// emoji-prefixed titles of the sphinx-needs demo corpus parse as paragraphs.
///
/// Note the deliberate silence of [`AdornmentFit::Refused`]: docutils
/// additionally emits an info-level `possible title underline, too short for
/// the title` for it, which this build cannot express (it has no severity
/// below warning) and should not promote to a warning — measured over the
/// `CPython` corpus the rule fires nine times at block level, every one of
/// them a literal-block `::` marker rather than a heading anybody mis-drew.
fn adornment_fits(text: &str, adornment: &str) -> AdornmentFit {
    if column_width(text) <= adornment.len() {
        AdornmentFit::Covers
    } else if adornment.len() >= MIN_TOLERATED_ADORNMENT {
        AdornmentFit::TooShort
    } else {
        AdornmentFit::Refused
    }
}

/// `line` as an adornment, when it is one that starts in column 0.
///
/// docutils matches overlines and underlines at the block's own indentation
/// only: an indented run of punctuation under a line is that line's
/// definition, not its underline. Trailing whitespace is not part of it.
fn column_zero_adornment(line: &str) -> Option<&str> {
    if line.starts_with(char::is_whitespace) {
        return None;
    }
    let adornment = line.trim_end();
    is_section_adornment(adornment).then_some(adornment)
}

/// Whether `line` is an adornment long enough to be read as an overline
/// rather than as text — docutils' `short_overline` threshold.
fn is_long_adornment(line: &str) -> bool {
    column_zero_adornment(line).is_some_and(|adornment| adornment.len() >= MIN_TOLERATED_ADORNMENT)
}

/// The section title starting at line `i`, in either form.
pub(super) fn detect_adornment(lines: &[&str], i: usize) -> Option<DetectedHeading> {
    detect_overlined_title(lines, i).or_else(|| detect_underlined_title(lines, i))
}

/// The overline + title + underline form starting at line `i`.
///
/// Only the title may be inset (docutils' `Line.indent`); both adornments
/// start in column 0. A short overline that does not match its underline is
/// not one — below [`MIN_TOLERATED_ADORNMENT`] docutils reads it as text.
fn detect_overlined_title(lines: &[&str], i: usize) -> Option<DetectedHeading> {
    let overline = column_zero_adornment(lines.get(i)?)?;
    let text = lines.get(i + 1)?.trim();
    let underline = column_zero_adornment(lines.get(i + 2)?)?;
    if text.is_empty() || is_section_adornment(text) {
        return None;
    }
    let problem = if overline == underline {
        adornment_fits(text, overline).problem().ok()?
    } else if overline.len() >= MIN_TOLERATED_ADORNMENT {
        Some(HeadingProblem::OverlineMismatch)
    } else {
        return None;
    };
    Some(DetectedHeading {
        consumed: 3,
        adornment: Adornment {
            character: overline.chars().next()?,
            style: AdornmentStyle::Overline,
        },
        text: text.to_string(),
        problem,
    })
}

/// The title + underline form starting at line `i`, both in column 0.
///
/// A long adornment is never the title here: docutils reads it as an
/// overline, which is [`find_malformed_overline`]'s business.
fn detect_underlined_title(lines: &[&str], i: usize) -> Option<DetectedHeading> {
    let line = lines.get(i)?;
    if line.starts_with(char::is_whitespace) || is_long_adornment(line) {
        return None;
    }
    let underline = column_zero_adornment(lines.get(i + 1)?)?;
    let text = line.trim();
    let problem = adornment_fits(text, underline).problem().ok()?;
    Some(DetectedHeading {
        consumed: 2,
        adornment: Adornment {
            character: underline.chars().next()?,
            style: AdornmentStyle::Underline,
        },
        text: text.to_string(),
        problem,
    })
}

/// What is wrong with the overline at `i`, when it starts no heading.
///
/// Asked only once [`detect_adornment`] has found no heading at `i`, and kept
/// apart from it because the paragraph parser asks that function too, to know
/// where a paragraph ends: these shapes stay text, so they must not end one.
/// An overline followed by a blank line is a transition's business, and one
/// shorter than [`MIN_TOLERATED_ADORNMENT`] is ordinary text, as in docutils.
pub(super) fn find_malformed_overline(lines: &[&str], i: usize) -> Option<MalformedOverline> {
    if !is_long_adornment(lines.get(i)?) {
        return None;
    }
    let next = lines.get(i + 1)?;
    if next.trim().is_empty() {
        return None;
    }
    if column_zero_adornment(next).is_some() {
        return Some(MalformedOverline::AdornmentWithoutTitle);
    }
    match lines
        .get(i + 2)
        .and_then(|line| column_zero_adornment(line))
    {
        // An underline is there, so whatever is wrong is a heading's problem.
        Some(_) => None,
        None => Some(MalformedOverline::MissingUnderline),
    }
}

/// Reports the overline at `i` that starts no heading, if it is one.
fn report_malformed_overline(
    lines: &[&str],
    i: usize,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) {
    let Some(malformed) = find_malformed_overline(lines, i) else {
        return;
    };
    let (code, message) = match malformed {
        MalformedOverline::MissingUnderline => (
            DiagnosticCode::HeadingMissingUnderline,
            "this section title overline has no matching underline below its title; \
             a transition needs a blank line after it",
        ),
        MalformedOverline::AdornmentWithoutTitle => (
            DiagnosticCode::HeadingAdornmentWithoutTitle,
            "two adornment lines with no section title between them; \
             a transition needs a blank line before and after it",
        ),
    };
    diagnostics.push(Diagnostic::at(code, message, ctx.line_span(i, lines[i])));
}

/// The message for an overline and underline that do not match.
fn overline_mismatch_message(overline: &str, underline: &str) -> String {
    let describe = |adornment: &str| {
        let character = adornment.chars().next().unwrap_or(' ');
        format!("{} × '{character}'", adornment.chars().count())
    };
    format!(
        "this section title's overline ({}) and underline ({}) must be the same character \
         and length",
        describe(overline.trim()),
        describe(underline.trim())
    )
}

/// The level of `adornment`, registering it as the next level when it is new
/// and `titles` allows a title here.
///
/// A title where none may stand still gets a level to render with, but must
/// not claim one for the document: its style would otherwise shift every
/// later top-level heading a level deeper.
fn heading_level(
    adornment: Adornment,
    adornment_order: &mut Vec<Adornment>,
    titles: SectionTitles,
) -> usize {
    if let Some(pos) = adornment_order.iter().position(|&a| a == adornment) {
        return pos + 1;
    }
    match titles {
        SectionTitles::Allowed => {
            adornment_order.push(adornment);
            adornment_order.len()
        }
        SectionTitles::Forbidden => adornment_order.len() + 1,
    }
}

/// Parses the section title at `i`, if one starts there.
///
/// An overline that starts no title is reported here even though the result
/// is `None` and its lines go on to be read as a paragraph: this is the one
/// place that looks at a block's first line as a possible title, and the
/// paragraph parser has no reason to.
pub(super) fn try_parse_heading(
    lines: &[&str],
    i: usize,
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Option<(usize, Node)> {
    let Some(DetectedHeading {
        consumed,
        adornment,
        text,
        problem,
    }) = detect_adornment(lines, i)
    else {
        report_malformed_overline(lines, i, diagnostics, ctx);
        return None;
    };
    // The heading's *text* line, which is the one below the overline in the
    // three-line form and the line itself in the two-line form.
    let text_line = match adornment.style {
        AdornmentStyle::Overline => i + 1,
        AdornmentStyle::Underline => i,
    };

    match problem {
        Some(HeadingProblem::AdornmentTooShort) => diagnostics.push(Diagnostic::at(
            DiagnosticCode::HeadingUnderlineTooShort,
            "section title is wider than the adornment underlining it",
            ctx.line_span(text_line, lines[text_line]),
        )),
        Some(HeadingProblem::OverlineMismatch) => diagnostics.push(Diagnostic::at(
            DiagnosticCode::HeadingOverlineMismatch,
            overline_mismatch_message(lines[i], lines[i + 2]),
            ctx.lines_span(i, i + 2, lines[i + 2]),
        )),
        None => {}
    }
    if ctx.section_titles() == SectionTitles::Forbidden {
        diagnostics.push(Diagnostic::at(
            DiagnosticCode::HeadingUnexpected,
            "a section title may only stand at the top level of a document or section, \
             not inside a block quote, list, table or directive body",
            ctx.line_span(text_line, lines[text_line]),
        ));
    }

    // No document has 255 adornment styles; saturating keeps that a
    // non-question rather than a cast that could wrap.
    let level = u8::try_from(heading_level(
        adornment,
        adornment_order,
        ctx.section_titles(),
    ))
    .unwrap_or(u8::MAX);

    let text = parse_inline_text_mapped(
        &text,
        ctx.default_domain,
        &SourceMap::single_line(
            &text,
            text_line,
            lines[text_line]
                .chars()
                .take_while(|c| c.is_whitespace())
                .count(),
        ),
        ctx,
    );

    Some((consumed, Node::Heading { level, text }))
}

#[cfg(test)]
mod adornment_tests;
#[cfg(test)]
mod malformed_tests;
#[cfg(test)]
mod nested_tests;
#[cfg(test)]
mod test_support;

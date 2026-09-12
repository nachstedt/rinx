//! Every spelling of a `PlantUML` diagram — `.. plantuml::`/`.. uml::`,
//! `.. entity-diagram::`/`.. needuml::` and `.. entity-arch::`/`.. needarch::`.
//!
//! Two of the three constructs are neither docutils' nor Sphinx's:
//! sphinxcontrib-plantuml's `UmlDirective` is the authority for the first pair
//! and sphinx-needs' `NeedumlDirective.option_spec` for the other four names.
//!
//! One parser serves all six because the difference between them is entirely
//! in what *fills* the template, never in how the directive is written: the
//! same option block, the same body, the same node. What the spelling does
//! decide is which options mean anything at all — `:key:` and `:extra:` are
//! read by the expander, so writing one on a plain `.. plantuml::` is
//! diagnosed rather than silently ignored.
//!
//! An unreadable option value is dropped and reported, leaving the diagram
//! intact: the same error-resilience `.. dropdown::` follows, and for a
//! sharper reason here — a diagram refused over a misspelled `:align:` is a
//! page with a missing picture.

use rusty_sphinx_ast::{
    Diagnostic, DiagnosticCode, Directive, ImageAlign, LengthOrPercentage, Span, TargetName, Uml,
    UmlSource,
};

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::indent::unindent_body_lines;

use super::options::{OptionLine, parse_percentage, report_unknown_options, scan_option_lines};

/// Parses any of the six diagram spellings into a [`Directive::Uml`].
pub(super) fn parse_uml(
    source: UmlSource,
    directive_span: Option<Span>,
    body_lines: &[&str],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Directive {
    let unindented_lines = unindent_body_lines(body_lines);
    let (option_lines, body_start) = scan_option_lines(&unindented_lines);

    let mut uml = Uml {
        span: directive_span,
        // Whatever the spelling, a diagram written inside an entity records
        // the one it sits in: the expander needs it, and this is the only
        // phase that still knows it.
        entity: ctx.enclosing_entity_id.cloned(),
        ..Uml::new(source, join_template_lines(&unindented_lines[body_start..]))
    };

    let unrecognized = read_options(&mut uml, &option_lines, diagnostics, ctx);
    report_unknown_options(
        &unrecognized,
        source.as_str(),
        DiagnosticCode::UmlUnknownOption,
        diagnostics,
        ctx,
    );
    if uml.has_unusable_scale() {
        report_unusable_scale(&option_lines, source, diagnostics, ctx);
    }

    Directive::Uml(Box::new(uml))
}

/// Joins the directive's content back into the text `PlantUML` will read.
///
/// Kept verbatim, blank lines and all: this is a template whose whitespace is
/// significant to both `PlantUML` and the Jinja expansion, and it is the text a
/// `:debug:` shows the author. Trailing blank lines go, because
/// `collect_directive_body` includes the separator before whatever follows the
/// directive and a diagram must not hash differently for it.
fn join_template_lines(lines: &[String]) -> String {
    let end = lines
        .iter()
        .rposition(|line| !line.trim().is_empty())
        .map_or(0, |last| last + 1);
    lines[..end].join("\n")
}

/// Reads every option onto `uml`, returning the lines nobody claimed.
fn read_options<'a>(
    uml: &mut Uml,
    option_lines: &'a [OptionLine],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Vec<&'a OptionLine> {
    let mut unrecognized = Vec::new();
    for line in option_lines {
        match line.name.as_str() {
            // Read by the expander, so meaningless without one.
            "key" | "extra" if !uml.source.is_templated() => {
                report_needs_template(line, uml.source, diagnostics, ctx);
            }
            "key" => {
                if line.value.is_empty() {
                    report_empty_value(line, uml.source, diagnostics, ctx);
                } else {
                    uml.key = Some(line.value.clone());
                }
            }
            "extra" => read_extra(uml, line, diagnostics, ctx),
            "config" => {
                if line.value.is_empty() {
                    report_empty_value(line, uml.source, diagnostics, ctx);
                } else {
                    uml.config = Some(line.value.clone());
                }
            }
            // A flag: what matters is only that the option was written.
            "debug" => uml.debug = true,
            // Accepted so a migrating document still parses, but reported:
            // a sandboxed build action may only write files declared before it
            // runs, and the path here is written inside the document. The
            // expanded source is available as the site's `diagram_sources`
            // output group instead.
            "save" => diagnostics.push(Diagnostic::at(
                DiagnosticCode::UmlSaveUnsupported,
                format!(
                    "{}: :save: cannot write '{}' — a build action may only write files declared \
                     before it runs; build the site's `diagram_sources` output group to get every \
                     diagram's expanded source instead",
                    uml.source.as_str(),
                    line.value
                ),
                ctx.line_span(line.line_index, &line.raw),
            )),
            "caption" => uml.caption = Some(line.value.clone()),
            "align" => match ImageAlign::parse(&line.value) {
                Some(align) => uml.align = Some(align),
                None => diagnostics.push(Diagnostic::at(
                    DiagnosticCode::UmlInvalidAlign,
                    format!(
                        "{}: :align: expects one of center, left, right, found '{}'",
                        uml.source.as_str(),
                        line.value
                    ),
                    ctx.line_span(line.line_index, &line.raw),
                )),
            },
            "scale" => match parse_percentage(&line.value) {
                Some(scale) => uml.scale = Some(scale),
                None => diagnostics.push(Diagnostic::at(
                    DiagnosticCode::UmlInvalidScale,
                    format!(
                        "{}: :scale: expects a non-negative percentage, found '{}'",
                        uml.source.as_str(),
                        line.value
                    ),
                    ctx.line_span(line.line_index, &line.raw),
                )),
            },
            "width" => match LengthOrPercentage::new(&line.value) {
                Ok(width) => uml.width = Some(width),
                Err(problem) => diagnostics.push(Diagnostic::at(
                    DiagnosticCode::UmlInvalidWidth,
                    format!("{}: :width: {problem}", uml.source.as_str()),
                    ctx.line_span(line.line_index, &line.raw),
                )),
            },
            "class" => uml.classes = line.value.split_whitespace().map(str::to_string).collect(),
            "name" => {
                if line.value.is_empty() {
                    report_empty_value(line, uml.source, diagnostics, ctx);
                } else {
                    uml.name = Some(TargetName::new(&line.value));
                }
            }
            _ => unrecognized.push(line),
        }
    }
    unrecognized
}

/// Reads an `:extra:` value: comma-separated `name: value` pairs.
///
/// A malformed pair is dropped and reported while the rest are still bound,
/// for the same reason a bad `:align:` does not cost the picture — a diagram
/// that vanishes teaches nothing about which pair was wrong.
fn read_extra(uml: &mut Uml, line: &OptionLine, diagnostics: &mut Diagnostics, ctx: &ParseCtx<'_>) {
    for pair in line.value.split(',') {
        let pair = pair.trim();
        if pair.is_empty() {
            continue;
        }
        match pair.split_once(':') {
            Some((name, value)) if !name.trim().is_empty() => {
                uml.extra
                    .insert(name.trim().to_string(), value.trim().to_string());
            }
            _ => diagnostics.push(Diagnostic::at(
                DiagnosticCode::UmlInvalidExtra,
                format!(
                    "{}: :extra: expects comma-separated 'name: value' pairs, found '{pair}'",
                    uml.source.as_str()
                ),
                ctx.line_span(line.line_index, &line.raw),
            )),
        }
    }
}

/// Reports a `:scale:` that has no `:width:` to apply to.
///
/// Pointed at the `:scale:` line itself rather than at the directive, since
/// that is the line the author would have to change — the same choice
/// `.. image::` makes for its own version of this.
fn report_unusable_scale(
    option_lines: &[OptionLine],
    source: UmlSource,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) {
    let span = option_lines
        .iter()
        .rev()
        .find(|line| line.name == "scale")
        .and_then(|line| ctx.line_span(line.line_index, &line.raw));
    diagnostics.push(Diagnostic::at(
        DiagnosticCode::UmlUnusableScale,
        format!(
            "{}: :scale: has no :width: to apply to, so it was ignored — the compiled diagram \
             is never opened while rendering, so it has no size of its own to scale",
            source.as_str()
        ),
        span,
    ));
}

/// Reports an option that only a templated diagram could ever read.
fn report_needs_template(
    line: &OptionLine,
    source: UmlSource,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) {
    diagnostics.push(Diagnostic::at(
        DiagnosticCode::UmlOptionNeedsTemplate,
        format!(
            "{}: :{}: is only read when the diagram is expanded against the entity graph, so it \
             was ignored; write `.. entity-diagram::` to use it",
            source.as_str(),
            line.name
        ),
        ctx.line_span(line.line_index, &line.raw),
    ));
}

/// Reports an option whose value is required but was left empty.
fn report_empty_value(
    line: &OptionLine,
    source: UmlSource,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) {
    diagnostics.push(Diagnostic::at(
        DiagnosticCode::UmlEmptyOptionValue,
        format!(
            "{}: :{}: needs a value, so the option was ignored",
            source.as_str(),
            line.name
        ),
        ctx.line_span(line.line_index, &line.raw),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use rusty_sphinx_ast::{Node, TargetName};

    /// Parses a whole document and returns its single diagram with the
    /// document's diagnostics — the dispatcher is what positions the parse
    /// context, so spans are only meaningful this way.
    fn parse_document(input: &str) -> (Uml, Vec<Diagnostic>) {
        let doc = parse("test.rst", input);
        let uml = doc
            .nodes
            .iter()
            .find_map(|node| match node {
                Node::Directive(Directive::Uml(uml)) => Some((**uml).clone()),
                _ => None,
            })
            .expect("document should contain a diagram directive");
        (uml, doc.diagnostics)
    }

    /// The codes reported, in order, for asserting about diagnostics without
    /// pinning their wording.
    fn codes(diagnostics: &[Diagnostic]) -> Vec<DiagnosticCode> {
        diagnostics.iter().map(|d| d.code).collect()
    }

    #[test]
    fn test_every_spelling_parses_to_one_node_recording_which_was_written() {
        // Given
        let spellings = [
            ("plantuml", UmlSource::PlantUml),
            ("uml", UmlSource::Uml),
            ("entity-diagram", UmlSource::EntityDiagram),
            ("needuml", UmlSource::NeedUml),
            ("entity-arch", UmlSource::EntityArch),
            ("needarch", UmlSource::NeedArch),
        ];

        for (name, expected) in spellings {
            // When
            let (uml, diagnostics) = parse_document(&format!(".. {name}::\n\n   A -> B\n"));

            // Then
            assert_eq!(uml.source, expected, "{name}");
            assert_eq!(uml.template, "A -> B", "{name}");
            assert_eq!(codes(&diagnostics), [], "{name}");
        }
    }

    #[test]
    fn test_the_template_keeps_its_blank_lines_but_not_its_trailing_ones() {
        // Given — a blank line inside the body is PlantUML's own, while the
        // one before the next paragraph belongs to the document
        let input = ".. plantuml::\n\n   A -> B\n\n   B -> C\n\nNext paragraph.\n";

        // When
        let (uml, diagnostics) = parse_document(input);

        // Then
        assert_eq!(uml.template, "A -> B\n\nB -> C");
        assert_eq!(codes(&diagnostics), []);
    }

    #[test]
    fn test_an_option_block_is_not_part_of_the_template() {
        // Given
        let input = ".. needuml::\n   :caption: How A talks to B\n\n   A -> B\n";

        // When
        let (uml, diagnostics) = parse_document(input);

        // Then
        assert_eq!(uml.template, "A -> B");
        assert_eq!(uml.caption.as_deref(), Some("How A talks to B"));
        assert_eq!(codes(&diagnostics), []);
    }

    #[test]
    fn test_reads_the_presentation_options() {
        // Given
        let input = ".. plantuml::\n   :align: center\n   :width: 400px\n   :scale: 50\n   \
                     :class: wide framed\n   :name: my-diagram\n\n   A -> B\n";

        // When
        let (uml, diagnostics) = parse_document(input);

        // Then
        assert_eq!(uml.align, Some(rusty_sphinx_ast::ImageAlign::Center));
        assert_eq!(
            uml.width,
            Some(LengthOrPercentage::new("400px").expect("a valid length"))
        );
        assert_eq!(uml.scale, Some(50));
        assert_eq!(uml.classes, ["wide", "framed"]);
        assert_eq!(uml.name, Some(TargetName::new("my-diagram")));
        assert_eq!(codes(&diagnostics), []);
    }

    #[test]
    fn test_a_percent_sign_on_a_scale_is_the_same_option() {
        // Given — docutils' `directives.percentage` strips one trailing `%`
        let input = ".. plantuml::\n   :width: 400px\n   :scale: 50%\n\n   A -> B\n";

        // When
        let (uml, diagnostics) = parse_document(input);

        // Then
        assert_eq!(uml.scale, Some(50));
        assert_eq!(codes(&diagnostics), []);
    }

    #[test]
    fn test_debug_is_a_flag_needing_no_value() {
        // Given
        let input = ".. needuml::\n   :debug:\n\n   A -> B\n";

        // When
        let (uml, diagnostics) = parse_document(input);

        // Then
        assert!(uml.debug);
        assert_eq!(codes(&diagnostics), []);
    }

    #[test]
    fn test_reads_the_templated_options() {
        // Given
        let input = ".. needarch::\n   :key: overview\n   :config: monochrome\n\n   A -> B\n";

        // When
        let (uml, diagnostics) = parse_document(input);

        // Then
        assert_eq!(uml.key.as_deref(), Some("overview"));
        assert_eq!(uml.config.as_deref(), Some("monochrome"));
        assert_eq!(codes(&diagnostics), []);
    }

    #[test]
    fn test_save_is_reported_rather_than_silently_ignored() {
        // Given — a sandboxed build action may only write files declared
        // before it runs, and this path is written inside the document
        let input = ".. needuml::\n   :save: out/arch.puml\n\n   A -> B\n";

        // When
        let (uml, diagnostics) = parse_document(input);

        // Then — the diagram still renders, and the author is pointed at what
        // does work
        assert_eq!(uml.template, "A -> B");
        assert_eq!(codes(&diagnostics), [DiagnosticCode::UmlSaveUnsupported]);
        assert!(
            diagnostics[0].message.contains("diagram_sources"),
            "{}",
            diagnostics[0].message
        );
    }

    #[test]
    fn test_extra_reads_comma_separated_pairs() {
        // Given
        let input = ".. needuml::\n   :extra: role: owner, team: platform\n\n   A -> B\n";

        // When
        let (uml, diagnostics) = parse_document(input);

        // Then
        assert_eq!(uml.extra.get("role").map(String::as_str), Some("owner"));
        assert_eq!(uml.extra.get("team").map(String::as_str), Some("platform"));
        assert_eq!(codes(&diagnostics), []);
    }

    #[test]
    fn test_a_malformed_extra_pair_is_reported_and_the_rest_still_bind() {
        // Given — `owner` carries no colon at all
        let input = ".. needuml::\n   :extra: role: owner, owner\n\n   A -> B\n";

        // When
        let (uml, diagnostics) = parse_document(input);

        // Then
        assert_eq!(uml.extra.get("role").map(String::as_str), Some("owner"));
        assert_eq!(codes(&diagnostics), [DiagnosticCode::UmlInvalidExtra]);
    }

    #[test]
    fn test_a_templated_option_on_a_plain_plantuml_is_reported() {
        // Given — nothing would ever read this `:key:`
        let input = ".. plantuml::\n   :key: overview\n\n   A -> B\n";

        // When
        let (uml, diagnostics) = parse_document(input);

        // Then — the diagram survives, and the ignored option is named
        assert_eq!(uml.template, "A -> B");
        assert_eq!(uml.key, None);
        assert_eq!(
            codes(&diagnostics),
            [DiagnosticCode::UmlOptionNeedsTemplate]
        );
    }

    #[test]
    fn test_an_unknown_option_is_reported_and_the_diagram_survives() {
        // Given
        let input = ".. plantuml::\n   :colour: red\n\n   A -> B\n";

        // When
        let (uml, diagnostics) = parse_document(input);

        // Then
        assert_eq!(uml.template, "A -> B");
        assert_eq!(codes(&diagnostics), [DiagnosticCode::UmlUnknownOption]);
    }

    #[test]
    fn test_an_unreadable_align_is_reported_and_the_diagram_survives() {
        // Given
        let input = ".. plantuml::\n   :align: sideways\n\n   A -> B\n";

        // When
        let (uml, diagnostics) = parse_document(input);

        // Then
        assert_eq!(uml.template, "A -> B");
        assert_eq!(uml.align, None);
        assert_eq!(codes(&diagnostics), [DiagnosticCode::UmlInvalidAlign]);
    }

    #[test]
    fn test_an_unreadable_scale_is_reported() {
        // Given
        let input = ".. plantuml::\n   :width: 400px\n   :scale: half\n\n   A -> B\n";

        // When
        let (uml, diagnostics) = parse_document(input);

        // Then
        assert_eq!(uml.scale, None);
        assert_eq!(codes(&diagnostics), [DiagnosticCode::UmlInvalidScale]);
    }

    #[test]
    fn test_an_unreadable_width_is_reported() {
        // Given
        let input = ".. plantuml::\n   :width: wide\n\n   A -> B\n";

        // When
        let (uml, diagnostics) = parse_document(input);

        // Then
        assert_eq!(uml.width, None);
        assert_eq!(codes(&diagnostics), [DiagnosticCode::UmlInvalidWidth]);
    }

    #[test]
    fn test_a_scale_with_no_width_is_reported_as_unusable() {
        // Given — the renderer never opens the compiled SVG, so there is no
        // natural size for a bare scale to apply to
        let input = ".. plantuml::\n   :scale: 50\n\n   A -> B\n";

        // When
        let (uml, diagnostics) = parse_document(input);

        // Then
        assert_eq!(uml.scale, Some(50));
        assert_eq!(codes(&diagnostics), [DiagnosticCode::UmlUnusableScale]);
    }

    #[test]
    fn test_an_option_needing_a_value_is_reported_when_written_empty() {
        // Given
        let input = ".. needuml::\n   :key:\n\n   A -> B\n";

        // When
        let (uml, diagnostics) = parse_document(input);

        // Then
        assert_eq!(uml.key, None);
        assert_eq!(codes(&diagnostics), [DiagnosticCode::UmlEmptyOptionValue]);
    }

    #[test]
    fn test_a_diagnostic_points_at_the_option_line_that_caused_it() {
        // Given — the `:align:` is on the document's second line
        let input = ".. plantuml::\n   :align: sideways\n\n   A -> B\n";

        // When
        let (_, diagnostics) = parse_document(input);

        // Then
        let span = diagnostics[0].span.expect("the option line has a position");
        assert_eq!(span.start.line, 2);
    }

    #[test]
    fn test_the_directive_carries_the_span_it_was_written_at() {
        // Given
        let input = "Intro.\n\n.. plantuml::\n\n   A -> B\n";

        // When
        let (uml, _) = parse_document(input);

        // Then
        assert_eq!(
            uml.span.expect("the directive has a position").start.line,
            3
        );
    }

    #[test]
    fn test_a_diagram_outside_an_entity_records_no_entity() {
        // Given
        let input = ".. entity-diagram::\n\n   A -> B\n";

        // When
        let (uml, _) = parse_document(input);

        // Then
        assert_eq!(uml.entity, None);
    }

    #[test]
    fn test_an_empty_diagram_parses_to_an_empty_template() {
        // Given — error resilience: an unfinished diagram must not panic
        let input = ".. plantuml::\n";

        // When
        let (uml, diagnostics) = parse_document(input);

        // Then
        assert_eq!(uml.template, "");
        assert_eq!(codes(&diagnostics), []);
    }
}

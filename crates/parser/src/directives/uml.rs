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

use crate::directives::invocation::{DirectiveInvocation, ParseState, single, unknown};
use crate::directives::option_value::{Classes, NonEmptyText, Percentage};
use crate::directives::spec::{Arguments, DirectiveSpec};
use rinx_ast::{
    Diagnostic, DiagnosticCode, Directive, ImageAlign, LengthOrPercentage, TargetName, Uml,
    UmlSource, Written,
};

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;

use super::options::{DirectiveOptions, OptionLine, classify_options, directive_options};

/// The plain `PlantUML` directives' entry in the table of directives this
/// build implements: sphinxcontrib-plantuml's two spellings.
pub(in crate::directives) const SPEC: DirectiveSpec = DirectiveSpec {
    names: &["plantuml", "uml"],
    doc: "A PlantUML diagram.",
    arguments: Arguments::OPTIONAL_TEXT,
    takes_content: true,
    content_hint: None,
    options: <PlantUmlOption as DirectiveOptions>::OPTIONS,
    accepts_further: false,
};

/// The templated diagrams' entry in the table: this build's two spellings and
/// sphinx-needs' two, expanded against the entity graph.
pub(in crate::directives) const TEMPLATED_SPEC: DirectiveSpec = DirectiveSpec {
    names: &["entity-diagram", "needuml", "entity-arch", "needarch"],
    doc: "A PlantUML diagram written as a template over the entity graph.",
    arguments: Arguments::OPTIONAL_TEXT,
    takes_content: true,
    content_hint: None,
    options: <TemplatedUmlOption as DirectiveOptions>::OPTIONS,
    accepts_further: false,
};

directive_options! {
    /// The options every diagram takes, whatever its spelling.
    enum UmlCommonOption {
        /// `:caption:`.
        Caption(String) => (["caption"], "A caption shown below the diagram."),
        /// `:align:`.
        Align(ImageAlign) => (["align"], UmlInvalidAlign, "Where the diagram sits."),
        /// `:scale:`.
        Scale(Percentage) => (["scale"], UmlInvalidScale, "The diagram's size, as a percentage."),
        /// `:width:`.
        Width(LengthOrPercentage) => (["width"], UmlInvalidWidth, "The diagram's width."),
        /// `:class:`.
        Class(Classes) => (["class"], "Classes for the diagram."),
        /// `:name:`.
        Name(Written<TargetName>) => (["name"], "A label to reference the diagram by."),
        /// `:config:`.
        Config(NonEmptyText) => (["config"], "PlantUML configuration written before the diagram."),
        /// `:debug:`.
        Debug => (["debug"], Flag, "Shows the expanded PlantUML source below the diagram."),
    }
}

directive_options! {
    /// The options `.. plantuml::` and `.. uml::` take: sphinxcontrib-plantuml's
    /// ten, and this build's own `:config:` and `:debug:`.
    enum PlantUmlOption {
        _ => (["alt"], Text, "Text describing the diagram."),
        _ => (["height"], Length, "The diagram's height."),
        _ => (["html_format"], Text, "The image format of the HTML output."),
        _ => (["latex_format"], Text, "The image format of the LaTeX output."),
        _ => (["max-width"], Length, "The diagram's largest width.");
        Common(UmlCommonOption),
    }
}

directive_options! {
    /// The options the templated diagrams take: sphinx-needs' seven, and the
    /// ones every diagram takes.
    enum TemplatedUmlOption {
        /// `:key:`.
        Key(NonEmptyText) => (["key"], "Which of the entity's diagrams this is."),
        /// `:extra:`.
        Extra => (["extra"], Text, "Values the template reads, as `name:value` pairs."),
        _ => (["save"], Path, "A file the expanded source is written to.", "a build action may only write files declared before it runs; build the site's `diagram_sources` output group to get every diagram's expanded source instead");
        Common(UmlCommonOption),
    }
}

/// Interprets the six spellings of a `PlantUML` diagram, recording which was
/// written.
pub(in crate::directives) fn interpret_uml(
    input: &DirectiveInvocation<'_, '_>,
    state: &mut ParseState<'_>,
) -> Vec<rinx_ast::Node> {
    let Ok(source) = input.name.parse::<rinx_ast::UmlSource>() else {
        return unknown(input, state);
    };
    single(parse_uml(source, input, state.diagnostics))
}

/// Parses any of the six diagram spellings into a [`Directive::Uml`].
pub(super) fn parse_uml(
    source: UmlSource,
    invocation: &DirectiveInvocation<'_, '_>,
    diagnostics: &mut Diagnostics,
) -> Directive {
    let ctx = &invocation.ctx;
    let option_lines = &invocation.options;
    let content = invocation.content();

    let mut uml = Uml {
        span: invocation.span,
        template_span: content
            .lines
            .first()
            .and_then(|first| content.ctx.line_span(0, first)),
        // Whatever the spelling, a diagram written inside an entity records
        // the one it sits in: the expander needs it, and this is the only
        // phase that still knows it.
        entity: ctx.enclosing_entity_id.cloned(),
        ..Uml::new(source, join_template_lines(&content.lines))
    };

    read_options(&mut uml, option_lines, diagnostics, ctx);
    if uml.has_unusable_scale() {
        report_unusable_scale(option_lines, source, diagnostics, ctx);
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

/// Reads a diagram's options: the templated spellings' own, or the plain
/// ones', and those every diagram takes.
fn read_options(
    uml: &mut Uml,
    option_lines: &[OptionLine],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) {
    let directive = uml.source.as_str();
    if !uml.source.is_templated() {
        for (option, _line) in
            classify_options::<PlantUmlOption>(option_lines, directive, diagnostics, ctx)
        {
            match option {
                PlantUmlOption::Common(common) => {
                    read_common_option(uml, common);
                }
            }
        }
        return;
    }
    for (option, line) in
        classify_options::<TemplatedUmlOption>(option_lines, directive, diagnostics, ctx)
    {
        match option {
            TemplatedUmlOption::Key(key) => uml.key = Some(key.0),
            TemplatedUmlOption::Extra => read_extra(uml, line, diagnostics, ctx),
            TemplatedUmlOption::Common(common) => {
                read_common_option(uml, common);
            }
        }
    }
}

/// Records one of the options every diagram takes.
fn read_common_option(uml: &mut Uml, option: UmlCommonOption) {
    match option {
        UmlCommonOption::Config(config) => uml.config = Some(config.0),
        // A flag: what matters is only that the option was written.
        UmlCommonOption::Debug => uml.debug = true,
        UmlCommonOption::Caption(caption) => uml.caption = Some(caption),
        UmlCommonOption::Align(align) => uml.align = Some(align),
        UmlCommonOption::Scale(scale) => uml.scale = Some(scale.0),
        UmlCommonOption::Width(width) => uml.width = Some(width),
        UmlCommonOption::Class(classes) => uml.classes = classes.0,
        UmlCommonOption::Name(name) => uml.name = Some(name),
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use rinx_ast::{Node, TargetName};

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
        assert_eq!(uml.align, Some(rinx_ast::ImageAlign::Center));
        assert_eq!(
            uml.width,
            Some(LengthOrPercentage::new("400px").expect("a valid length"))
        );
        assert_eq!(uml.scale, Some(50));
        assert_eq!(uml.classes, ["wide", "framed"]);
        assert_eq!(uml.name, Some(TargetName::new("my-diagram").into()));
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
        assert_eq!(
            codes(&diagnostics),
            [DiagnosticCode::DirectiveUnsupportedOption]
        );
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
            [DiagnosticCode::DirectiveUnknownOption]
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
        assert_eq!(
            codes(&diagnostics),
            [DiagnosticCode::DirectiveUnknownOption]
        );
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
        assert_eq!(
            codes(&diagnostics),
            [DiagnosticCode::DirectiveEmptyOptionValue]
        );
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
    fn test_the_template_span_is_the_first_template_line() {
        // Given — the body starts two lines below the marker
        let input = "Intro.\n\n.. plantuml::\n\n   A -> B\n   B -> C\n";

        // When
        let (uml, _) = parse_document(input);

        // Then — line 5, from the template's own indentation to its end
        let span = uml.template_span.expect("the template has a position");
        assert_eq!((span.start.line, span.start.column), (5, 4));
        assert_eq!((span.end.line, span.end.column), (5, 10));
    }

    #[test]
    fn test_the_template_span_skips_the_option_block_and_the_blank_line_after_it() {
        // Given
        let input = ".. needuml::\n   :debug:\n   :caption: Flow\n\n   {{ uml(\"x\") }}\n";

        // When
        let (uml, _) = parse_document(input);

        // Then
        let span = uml.template_span.expect("the template has a position");
        assert_eq!(span.start.line, 5);
        assert_eq!(uml.template.lines().next(), Some("{{ uml(\"x\") }}"));
    }

    #[test]
    fn test_a_diagram_with_no_template_has_no_template_span() {
        // Given
        let input = ".. plantuml::\n   :debug:\n";

        // When
        let (uml, _) = parse_document(input);

        // Then
        assert_eq!(uml.template_span, None);
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

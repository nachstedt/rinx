//! `.. dropdown::` — sphinx-design's collapsible container.
//!
//! The one directive here that is neither docutils' nor Sphinx's.
//! sphinx-design's `DropdownDirective.option_spec` is the authority for every
//! name and value below, and its `directives.choice` validators are why each
//! option is matched case-insensitively against a closed set.
//!
//! An unreadable option value is dropped and reported, leaving the dropdown
//! itself intact — the same error-resilience every other directive here
//! follows, and it matters more than usual for this one: an unknown directive
//! never parses its body, so refusing the whole dropdown over a misspelled
//! `:color:` would silently swallow every construct written inside it.

use crate::directives::invocation::{DirectiveInvocation, ParseState, single};
use crate::directives::option_value::OptionValueType;
use crate::directives::spec::OptionValue;
use crate::directives::spec::{Arguments, DirectiveSpec};
use rinx_ast::{
    Animation, Chevron, Directive, Dropdown, Named, OcticonName, SemanticColor, TargetName,
};

use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::headings::Adornment;
use crate::inline::{SourceMap, parse_inline_text_mapped};

use super::option_value::{Classes, Margin};
use super::options::{DirectiveOptions, OptionLine, classify_options, directive_options};

const DIRECTIVE: &str = "dropdown";

/// This directive's entry in the table of directives this build implements.
pub(in crate::directives) const SPEC: DirectiveSpec = DirectiveSpec {
    names: &["dropdown"],
    doc: "A collapsible container.",
    arguments: Arguments::OPTIONAL_TEXT,
    takes_content: true,
    content_hint: None,
    options: <DropdownOption as DirectiveOptions>::OPTIONS,
    accepts_further: false,
};

/// Interprets `.. dropdown::`.
pub(in crate::directives) fn interpret_dropdown(
    input: &DirectiveInvocation<'_, '_>,
    state: &mut ParseState<'_>,
) -> Vec<rinx_ast::Node> {
    single(parse_dropdown(
        input,
        state.adornment_order,
        state.diagnostics,
    ))
}

/// Parses a `.. dropdown::` into a [`Directive::Dropdown`].
pub(super) fn parse_dropdown(
    invocation: &DirectiveInvocation<'_, '_>,
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
) -> Directive {
    let ctx = &invocation.ctx;
    let mut dropdown = Dropdown {
        title: parse_title(invocation.argument(0), ctx),
        span: invocation.span,
        ..Dropdown::new()
    };
    read_options(&mut dropdown, &invocation.options, diagnostics, ctx);

    dropdown.body = invocation.content().parse(adornment_order, diagnostics);

    Directive::Dropdown(Box::new(dropdown))
}

/// Reads the directive's argument as inline markup.
///
/// Unlike every caption in this build — a code block's, a table's — a
/// dropdown's title really is parsed: sphinx-design runs it through
/// `inline_text`, so a role or a literal in the title works.
fn parse_title(argument: &str, ctx: &ParseCtx<'_>) -> Vec<rinx_ast::InlineNode> {
    let title = argument.trim();
    if title.is_empty() {
        return Vec::new();
    }
    parse_inline_text_mapped(title, ctx.default_domain, &SourceMap::none(), ctx)
}

directive_options! {
    /// The options `.. dropdown::` takes: sphinx-design's ten.
    enum DropdownOption {
        /// `:open:`.
        Open => (["open"], Flag, "Starts the dropdown open."),
        /// `:color:`.
        Color(SemanticColor) => (["color"], DropdownInvalidColor, "The title bar's colour."),
        /// `:icon:`.
        Icon(Octicon) => (["icon"], DropdownUnknownIcon, "An octicon shown before the title."),
        /// `:chevron:`.
        Chevron(Chevron) => (["chevron"], DropdownInvalidChevron, "Which way the chevron points, closed and open."),
        /// `:animate:`.
        Animate(Animation) => (["animate"], DropdownInvalidAnimate, "How the dropdown opens."),
        /// `:margin:`.
        Margin(Margin) => (["margin"], DropdownInvalidMargin, "The margin around the dropdown, by spacing level."),
        /// `:name:`.
        Name(Named<TargetName>) => (["name"], "A label to reference the dropdown by."),
        /// `:class-container:`.
        ClassContainer(Classes) => (["class-container"], "Classes for the whole dropdown."),
        /// `:class-title:`.
        ClassTitle(Classes) => (["class-title"], "Classes for its title."),
        /// `:class-body:`.
        ClassBody(Classes) => (["class-body"], "Classes for its body."),
    }
}

/// Reads every option onto `dropdown`, returning the lines nobody claimed.
fn read_options(
    dropdown: &mut Dropdown,
    option_lines: &[OptionLine],
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) {
    for (option, _) in classify_options::<DropdownOption>(option_lines, DIRECTIVE, diagnostics, ctx)
    {
        match option {
            // A flag: `directives.flag` accepts no value at all, so what
            // matters is only that the option was written.
            DropdownOption::Open => dropdown.open = true,
            DropdownOption::Color(color) => dropdown.color = Some(color),
            DropdownOption::Icon(icon) => dropdown.icon = Some(icon.0),
            DropdownOption::Chevron(chevron) => dropdown.chevron = chevron,
            DropdownOption::Animate(animation) => dropdown.animate = Some(animation),
            DropdownOption::Margin(margin) => dropdown.margin = Some(margin.0),
            DropdownOption::Name(name) => dropdown.name = Some(name),
            DropdownOption::ClassContainer(classes) => dropdown.class_container = classes.0,
            DropdownOption::ClassTitle(classes) => dropdown.class_title = classes.0,
            DropdownOption::ClassBody(classes) => dropdown.class_body = classes.0,
        }
    }
}

/// An `:icon:`: the name of an octicon the set that will draw it has.
///
/// The name is kept only when an icon exists by it: a name nothing matches
/// would otherwise reach the renderer, which cannot report it against the
/// `:icon:` line the way this can.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Octicon(OcticonName);

impl OptionValueType for Octicon {
    const VALUE: OptionValue<'static> = OptionValue::Text;

    fn read(line: &OptionLine, _ctx: &ParseCtx<'_>) -> Result<Self, String> {
        let name = OcticonName::new(&line.value).map_err(|error| error.to_string())?;
        if is_known_octicon(name.as_str()) {
            Ok(Self(name))
        } else {
            Err(format!("'{name}' is not the name of an octicon"))
        }
    }
}

/// Whether the icon set defines an icon by this name, at any size.
///
/// The set keys each drawing by name *and* height (`light-bulb-16`), and the
/// heights an icon ships in vary — mostly 16 and 24, but a few icons add a 12
/// and `copilot` a 48 and a 96, and some ship at a single height — so a name
/// is known when any of them resolves. The renderer picks between them when it draws (see its
/// `octicon` module); here only existence is being asked.
fn is_known_octicon(name: &str) -> bool {
    const ARTWORK_HEIGHTS: &[u32] = &[16, 24, 12, 48, 96];
    ARTWORK_HEIGHTS
        .iter()
        .any(|height| octicons_pack::get_icon(&format!("{name}-{height}")).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::Spacing;
    use rinx_ast::{Diagnostic, DiagnosticCode};
    use rinx_ast::{InlineNode, Node, SpacingValue};

    /// Parses a whole document and returns its single `.. dropdown::` with the
    /// document's diagnostics — the dispatcher is what positions the parse
    /// context, so spans are only meaningful this way.
    fn parse_document(input: &str) -> (Dropdown, Vec<Diagnostic>) {
        let doc = crate::parse_document("test.rst", input);
        let dropdown = doc
            .nodes
            .iter()
            .find_map(|node| match node {
                Node::Directive {
                    directive: Directive::Dropdown(dropdown),
                    ..
                } => Some((**dropdown).clone()),
                _ => None,
            })
            .expect("document should contain a dropdown directive");
        (dropdown, doc.diagnostics)
    }

    /// The codes reported, in order, for asserting about diagnostics without
    /// pinning their wording.
    fn codes(diagnostics: &[Diagnostic]) -> Vec<DiagnosticCode> {
        diagnostics.iter().map(|d| d.code).collect()
    }

    #[test]
    fn test_parses_a_bare_dropdown_with_a_title_and_a_body() {
        // Given
        let input = ".. dropdown:: Details\n\n   Some prose.\n";

        // When
        let (dropdown, diagnostics) = parse_document(input);

        // Then
        assert_eq!(
            dropdown.title,
            vec![InlineNode::Text("Details".to_string())]
        );
        assert_eq!(
            dropdown.body,
            vec![Node::paragraph(vec![InlineNode::Text(
                "Some prose.".to_string()
            )])]
        );
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_an_absent_argument_leaves_the_title_empty() {
        // Given
        let input = ".. dropdown::\n\n   Some prose.\n";

        // When
        let (dropdown, diagnostics) = parse_document(input);

        // Then
        assert!(dropdown.title.is_empty());
        assert!(!dropdown.has_title());
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_the_title_is_parsed_as_inline_markup() {
        // Given — a literal in the argument, which no other directive caption
        // in this build would parse
        let input = ".. dropdown:: See ``config.toml``\n";

        // When
        let (dropdown, _) = parse_document(input);

        // Then
        assert_eq!(
            dropdown.title,
            vec![
                InlineNode::Text("See ".to_string()),
                InlineNode::Literal("config.toml".to_string()),
            ]
        );
    }

    #[test]
    fn test_open_is_a_flag() {
        // Given
        let input = ".. dropdown:: Details\n   :open:\n";

        // When
        let (dropdown, diagnostics) = parse_document(input);

        // Then
        assert!(dropdown.open);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_reads_every_option() {
        // Given — the whole `option_spec`, in one directive
        let input = ".. dropdown:: Details\n\
             \x20  :open:\n\
             \x20  :color: success\n\
             \x20  :icon: light-bulb\n\
             \x20  :chevron: down-up\n\
             \x20  :animate: fade-in-slide-down\n\
             \x20  :margin: 0 1 2 auto\n\
             \x20  :name: my-dropdown\n\
             \x20  :class-container: one two\n\
             \x20  :class-title: three\n\
             \x20  :class-body: four\n";

        // When
        let (dropdown, diagnostics) = parse_document(input);

        // Then
        assert!(dropdown.open);
        assert_eq!(dropdown.color, Some(SemanticColor::Success));
        assert_eq!(
            dropdown.icon.as_ref().map(OcticonName::as_str),
            Some("light-bulb")
        );
        assert_eq!(dropdown.chevron, Chevron::DownUp);
        assert_eq!(dropdown.animate, Some(Animation::FadeInSlideDown));
        assert_eq!(
            dropdown.margin,
            Some(Spacing::Sides {
                top: SpacingValue::Zero,
                bottom: SpacingValue::One,
                left: SpacingValue::Two,
                right: SpacingValue::Auto,
            })
        );
        assert_eq!(
            dropdown.name.as_ref().map(|name| name.value().as_str()),
            Some("my-dropdown")
        );
        assert_eq!(dropdown.class_container, vec!["one", "two"]);
        assert_eq!(dropdown.class_title, vec!["three"]);
        assert_eq!(dropdown.class_body, vec!["four"]);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_an_option_value_is_matched_case_insensitively() {
        // Given — docutils' own `choice` validator lowercases first
        let input = ".. dropdown:: Details\n   :color: Success\n   :chevron: Down-Up\n";

        // When
        let (dropdown, diagnostics) = parse_document(input);

        // Then
        assert_eq!(dropdown.color, Some(SemanticColor::Success));
        assert_eq!(dropdown.chevron, Chevron::DownUp);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_an_unknown_colour_is_reported_and_dropped() {
        // Given
        let input = ".. dropdown:: Details\n   :color: chartreuse\n";

        // When
        let (dropdown, diagnostics) = parse_document(input);

        // Then — the dropdown survives; only the option is refused
        assert_eq!(dropdown.color, None);
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::DropdownInvalidColor]
        );
        assert!(diagnostics[0].message.contains("primary, secondary"));
    }

    #[test]
    fn test_an_unknown_chevron_is_reported_and_the_default_kept() {
        // Given
        let input = ".. dropdown:: Details\n   :chevron: left-right\n";

        // When
        let (dropdown, diagnostics) = parse_document(input);

        // Then
        assert_eq!(dropdown.chevron, Chevron::RightDown);
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::DropdownInvalidChevron]
        );
    }

    #[test]
    fn test_an_unknown_animation_is_reported_and_dropped() {
        // Given
        let input = ".. dropdown:: Details\n   :animate: slide-up\n";

        // When
        let (dropdown, diagnostics) = parse_document(input);

        // Then
        assert_eq!(dropdown.animate, None);
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::DropdownInvalidAnimate]
        );
    }

    #[test]
    fn test_a_margin_off_the_scale_is_reported_and_dropped() {
        // Given
        let input = ".. dropdown:: Details\n   :margin: 6\n";

        // When
        let (dropdown, diagnostics) = parse_document(input);

        // Then
        assert_eq!(dropdown.margin, None);
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::DropdownInvalidMargin]
        );
        assert!(diagnostics[0].message.contains("auto, 0, 1, 2, 3, 4, 5"));
    }

    #[test]
    fn test_a_margin_of_two_values_is_reported() {
        // Given — one or four, never two
        let input = ".. dropdown:: Details\n   :margin: 1 2\n";

        // When
        let (dropdown, diagnostics) = parse_document(input);

        // Then
        assert_eq!(dropdown.margin, None);
        assert!(diagnostics[0].message.contains("found 2"));
    }

    #[test]
    fn test_an_unknown_icon_is_reported_and_dropped() {
        // Given
        let input = ".. dropdown:: Details\n   :icon: not-an-octicon\n";

        // When
        let (dropdown, diagnostics) = parse_document(input);

        // Then — caught here rather than silently drawing nothing later
        assert_eq!(dropdown.icon, None);
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::DropdownUnknownIcon]
        );
        assert!(diagnostics[0].message.contains("not-an-octicon"));
    }

    #[test]
    fn test_a_two_word_icon_is_reported_as_unknown() {
        // Given
        let input = ".. dropdown:: Details\n   :icon: light bulb\n";

        // When
        let (dropdown, diagnostics) = parse_document(input);

        // Then
        assert_eq!(dropdown.icon, None);
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::DropdownUnknownIcon]
        );
    }

    #[test]
    fn test_an_option_with_no_value_is_reported() {
        // Given
        let input = ".. dropdown:: Details\n   :name:\n   :icon:\n";

        // When
        let (dropdown, diagnostics) = parse_document(input);

        // Then
        assert_eq!(dropdown.name, None);
        assert_eq!(dropdown.icon, None);
        assert_eq!(
            codes(&diagnostics),
            vec![
                DiagnosticCode::DirectiveEmptyOptionValue,
                DiagnosticCode::DirectiveEmptyOptionValue,
            ]
        );
    }

    #[test]
    fn test_an_unrecognized_option_is_reported() {
        // Given — `:padding:` belongs to cards and grids, not to a dropdown
        let input = ".. dropdown:: Details\n   :padding: 2\n";

        // When
        let (_, diagnostics) = parse_document(input);

        // Then
        assert_eq!(
            codes(&diagnostics),
            vec![DiagnosticCode::DirectiveUnknownOption]
        );
    }

    #[test]
    fn test_the_body_parses_as_ordinary_block_content() {
        // Given
        let input = ".. dropdown:: Details\n\n   * one\n   * two\n";

        // When
        let (dropdown, diagnostics) = parse_document(input);

        // Then
        assert!(matches!(dropdown.body[..], [Node::BulletList { .. }]));
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_a_directive_nested_in_the_body_really_parses() {
        // Given — the whole point of supporting this directive: an unknown
        // container swallows everything written inside it
        let input = ".. dropdown:: Details\n\n   .. note::\n\n      Inside.\n";

        // When
        let (dropdown, diagnostics) = parse_document(input);

        // Then
        assert!(matches!(
            dropdown.body[..],
            [Node::Directive {
                directive: Directive::Admonition { .. },
                ..
            }]
        ));
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_a_diagnostic_inside_the_body_points_at_the_line_it_was_written_on() {
        // Given — an unknown option on a nested directive, four lines down
        let input =
            ".. dropdown:: Details\n   :color: success\n\n   .. contents::\n      :nope: 1\n";

        // When
        let (_, diagnostics) = parse_document(input);

        // Then — the body context was rebased, so the position is the real one
        let span = diagnostics
            .iter()
            .find(|d| d.code == DiagnosticCode::DirectiveUnknownOption)
            .and_then(|d| d.span)
            .expect("the nested option is reported with a span");
        assert_eq!(span.start.line, 5);
    }

    #[test]
    fn test_the_directives_own_span_is_kept() {
        // Given
        let input = "Intro.\n\n.. dropdown:: Details\n";

        // When
        let (dropdown, _) = parse_document(input);

        // Then
        assert_eq!(
            dropdown
                .span
                .expect("a dropdown records its span")
                .start
                .line,
            3
        );
    }

    #[test]
    fn test_is_known_octicon_accepts_a_name_at_any_shipped_size() {
        // Given / When / Then — `light-bulb` ships at 16 and 24,
        // `no-entry-fill` at 12 alone
        assert!(is_known_octicon("light-bulb"));
        assert!(is_known_octicon("no-entry-fill"));
        assert!(!is_known_octicon("not-an-octicon"));
    }

    #[test]
    fn test_a_choice_lists_its_vocabulary_in_declaration_order() {
        // Given / When
        let listed = <Chevron as crate::directives::option_value::ChoiceValue>::choices();

        // Then
        assert_eq!(listed, ["right-down", "down-up"]);
    }
}

//! Rendering the two math roles: `:math:` (inline LaTeX) and `:eq:` (a
//! reference to a numbered equation).
//!
//! They sit together because they are two halves of one feature, but they are
//! dispatched differently: `:math:` needs nothing but the LaTeX, while `:eq:`
//! must resolve against the project index like any other cross-reference.

use std::fmt::Write as _;

use math_core::MathDisplay;
use rinx_ast::Span;
use rinx_index::{EquationLocation, ProjectIndex};

use crate::blocks::equation_anchor_id;
use crate::math::{MathError, MathRenderer};
use crate::{BrokenLink, BrokenLinkKind, ReferenceTarget};
use rinx_index::relative_doc_href;

/// Renders a `:math:` role.
///
/// The wrapper span carries Sphinx's class names so a borrowed theme styles
/// inline math the same way it styles a display equation.
pub(super) fn render_inline_math(
    html: &mut String,
    latex: &str,
    span: Option<Span>,
    math: &MathRenderer,
    math_errors: &mut Vec<MathError>,
) {
    html.push_str("<span class=\"math notranslate nohighlight\">");
    match math.to_html(latex, MathDisplay::Inline) {
        Ok(mathml) => html.push_str(&mathml),
        Err(failure) => {
            html.push_str(&failure.html);
            math_errors.push(MathError {
                message: failure.message,
                span,
            });
        }
    }
    html.push_str("</span>");
}

/// Renders an `:eq:` role: a link to a labeled `.. math::`, showing that
/// equation's number as the link text.
///
/// Unresolvable for two distinct reasons, both handled the same way — the
/// label names no equation at all, or names one that exists but was never
/// numbered (unlabeled equations aren't in the index, and a `:nowrap:` one is
/// deliberately left out). Either way the author asked for a number that
/// doesn't exist, so `(?)` stands in and a [`BrokenLink`] is reported.
pub(super) fn render_equation_reference(
    html: &mut String,
    label: &str,
    span: Option<Span>,
    index: &ProjectIndex,
    doc_path: &str,
    broken_links: &mut Vec<BrokenLink>,
) {
    let target_name = rinx_ast::TargetName::new(label);
    if let Some(location) = index.equations.get(&target_name) {
        write_equation_link(html, &target_name, location, doc_path);
    } else {
        let id = equation_anchor_id(&target_name);
        let anchor = html_escape::encode_double_quoted_attribute(&id);
        let _ = write!(
            html,
            "<a href=\"#{anchor}\" class=\"broken-link\"><span class=\"eqno\">(?)</span></a>"
        );
        broken_links.push(BrokenLink {
            kind: BrokenLinkKind::EquationReference
                .unless_contested(index.ambiguous_definitions.equations.get(&target_name)),
            target: label.to_string(),
            span,
        });
    }
}

/// Where the `:eq:` to `label` leads — `None` when
/// [`render_equation_reference`] would draw it broken.
pub(super) fn equation_target(index: &ProjectIndex, label: &str) -> Option<ReferenceTarget> {
    let name = rinx_ast::TargetName::new(label);
    index
        .equations
        .get(&name)
        .map(|location| equation_reference_target(&name, location))
}

/// The equation labeled `label` as a reference target, titled with its
/// number as `:eq:` shows it — shared with `:any:`.
pub(super) fn equation_reference_target(
    label: &rinx_ast::TargetName,
    location: &EquationLocation,
) -> ReferenceTarget {
    ReferenceTarget::in_document(
        format!("({})", location.number),
        &location.doc_path,
        Some(equation_anchor_id(label)),
    )
}

/// Writes a link to the equation labeled `label`, showing its number — what
/// every reference that resolved to an equation renders as.
pub(super) fn write_equation_link(
    html: &mut String,
    label: &rinx_ast::TargetName,
    location: &EquationLocation,
    doc_path: &str,
) {
    let href = format!(
        "{}#{}",
        relative_doc_href(&location.doc_path, doc_path),
        equation_anchor_id(label)
    );
    let href_attr = html_escape::encode_double_quoted_attribute(&href);
    let _ = write!(
        html,
        "<a class=\"reference internal\" href=\"{href_attr}\">\
         <span class=\"eqno\">({})</span></a>",
        location.number
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::Position;
    use rinx_index::EquationLocation;

    fn index_with(label: &str, doc_path: &str, number: usize) -> ProjectIndex {
        let mut index = ProjectIndex::default();
        index.equations.insert(
            rinx_ast::TargetName::new(label),
            EquationLocation::new(doc_path, number),
        );
        index
    }

    #[test]
    fn test_render_inline_math_wraps_mathml_in_a_math_span() {
        // Given
        let mut html = String::new();
        let math = MathRenderer::new();
        let mut errors = Vec::new();

        // When
        render_inline_math(&mut html, "a + b", None, &math, &mut errors);

        // Then
        assert!(
            html.starts_with("<span class=\"math notranslate nohighlight\">"),
            "{html}"
        );
        assert!(html.contains("<math>"), "{html}");
        assert!(html.ends_with("</span>"), "{html}");
        assert!(errors.is_empty());
    }

    #[test]
    fn test_render_inline_math_reports_invalid_latex_and_keeps_the_source() {
        // Given LaTeX the backend rejects
        let mut html = String::new();
        let math = MathRenderer::new();
        let mut errors = Vec::new();
        let span = Some(Span::new(Position::new(4, 2), Position::new(4, 7)));

        // When
        render_inline_math(&mut html, r"\frac{1}{2", span, &math, &mut errors);

        // Then the reader still sees the source, and the author gets a placed warning
        assert!(html.contains(r"\frac{1}{2"), "{html}");
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].span, span);
    }

    #[test]
    fn test_render_equation_reference_links_to_a_numbered_equation() {
        // Given an equation numbered in this same document
        let index = index_with("euler", "math.rst", 3);
        let mut html = String::new();
        let mut broken = Vec::new();

        // When
        render_equation_reference(&mut html, "euler", None, &index, "math.rst", &mut broken);

        // Then
        assert_eq!(
            html,
            "<a class=\"reference internal\" href=\"math.html#equation-euler\">\
             <span class=\"eqno\">(3)</span></a>"
        );
        assert!(broken.is_empty());
    }

    #[test]
    fn test_render_equation_reference_resolves_across_directories() {
        // Given an equation defined in another directory
        let index = index_with("euler", "math.rst", 1);
        let mut html = String::new();
        let mut broken = Vec::new();

        // When referenced from a nested page
        render_equation_reference(
            &mut html,
            "euler",
            None,
            &index,
            "team_a/index.rst",
            &mut broken,
        );

        // Then the href walks back up
        assert!(
            html.contains("href=\"../math.html#equation-euler\""),
            "{html}"
        );
    }

    #[test]
    fn test_render_equation_reference_reports_an_unknown_label() {
        // Given an empty index
        let index = ProjectIndex::default();
        let mut html = String::new();
        let mut broken = Vec::new();
        let span = Some(Span::new(Position::new(9, 1), Position::new(9, 5)));

        // When
        render_equation_reference(&mut html, "missing", span, &index, "math.rst", &mut broken);

        // Then a placeholder is shown and the reference is reported
        assert!(html.contains("class=\"broken-link\""), "{html}");
        assert!(html.contains("(?)"), "{html}");
        assert_eq!(broken.len(), 1);
        assert_eq!(broken[0].kind, BrokenLinkKind::EquationReference);
        assert_eq!(broken[0].target, "missing");
        assert_eq!(broken[0].span, span);
    }
}

//! Rendering `.. math::` — display equations.
//!
//! Two jobs live here. First, assembling one LaTeX string out of the
//! [`Directive::Math`] parts the parser split, which is where Sphinx's own
//! rules about multi-equation bodies are reproduced. Second, wrapping the
//! resulting `MathML` in the `div.math` shell that carries the equation number
//! and its anchor.
//!
//! The class names are Sphinx's (`math notranslate nohighlight`, `eqno`,
//! `headerlink`) so CSS borrowed from a Sphinx theme styles equations here
//! too — the same reason the rest of the renderer borrows Sphinx's vocabulary
//! (see `docs/decisions/001-template-system.md`).

use std::fmt::Write as _;

use math_core::MathDisplay;
use rinx_ast::{Span, TargetName};

use crate::RenderCtx;
use crate::math::MathError;

/// Builds the `id` an equation is anchored under, matching Sphinx so an
/// `:eq:` href is predictable from the label alone.
pub(crate) fn equation_anchor_id(label: &TargetName) -> String {
    format!("equation-{}", label.as_str())
}

/// Joins the parts of a math body into the single LaTeX string the backend is
/// given, following `sphinx.ext.mathjax`'s `html_visit_displaymath`.
///
/// Two rules, both about alignment. A part containing `\\` holds several
/// aligned lines, so it is wrapped in an alignment environment; and when there
/// is more than one part, the whole body is wrapped in one so the separate
/// equations line up with each other, joined by `\\`.
///
/// Sphinx uses `\begin{split}` for the first of those. `split` is not a
/// environment the `MathML` backend implements, so `aligned` — which produces the
/// same alignment and is implemented — is used for both. This is the one
/// deliberate divergence from Sphinx's LaTeX assembly.
fn assemble_latex(parts: &[String]) -> String {
    let aligned = |part: &String| {
        if part.contains("\\\\") {
            format!("\\begin{{aligned}}{part}\\end{{aligned}}")
        } else {
            part.clone()
        }
    };

    if parts.len() > 1 {
        let joined = parts.iter().map(aligned).collect::<Vec<_>>().join("\\\\");
        format!("\\begin{{aligned}}{joined}\\end{{aligned}}")
    } else {
        parts.first().map(aligned).unwrap_or_default()
    }
}

/// Renders the `<span class="eqno">` an equation's number lives in, with the
/// permalink anchor beside it.
fn render_equation_number(html: &mut String, label: &TargetName, number: usize) {
    let id = equation_anchor_id(label);
    let anchor = html_escape::encode_double_quoted_attribute(&id);
    let _ = write!(
        html,
        "<span class=\"eqno\">({number})\
         <a class=\"headerlink\" href=\"#{anchor}\" title=\"Permalink to this equation\">¶</a>\
         </span>"
    );
}

/// Renders a `.. math::` directive.
///
/// `:nowrap:` short-circuits both of the shell's extras, exactly as Sphinx
/// does: the body is handed to the backend untouched, and no number is emitted
/// even when the directive carries a label — under `:nowrap:` the author owns
/// the LaTeX environment, so numbering is theirs to write.
pub(super) fn render_math(
    html: &mut String,
    parts: &[String],
    label: Option<&TargetName>,
    nowrap: bool,
    classes: &[String],
    span: Option<Span>,
    ctx: &mut RenderCtx<'_>,
) {
    let latex = if nowrap {
        parts.join("\n")
    } else {
        assemble_latex(parts)
    };

    // An equation is numbered only if the analyzer gave it one, which it does
    // for a labeled, non-`:nowrap:` block. Reading the number back out of the
    // index rather than recomputing it here keeps one source of truth: the
    // same lookup an `:eq:` in another document performs.
    let number = label
        .filter(|_| !nowrap)
        .and_then(|name| ctx.index.equations.get(name))
        .filter(|location| location.doc_path == ctx.original_doc_path)
        .map(|location| location.number);

    let mut class_names = String::from("math notranslate nohighlight");
    for class in classes {
        let _ = write!(
            class_names,
            " {}",
            html_escape::encode_double_quoted_attribute(class)
        );
    }

    let _ = write!(html, "<div class=\"{class_names}\"");
    if let Some(name) = label.filter(|_| !nowrap) {
        let id = equation_anchor_id(name);
        let anchor = html_escape::encode_double_quoted_attribute(&id);
        let _ = write!(html, " id=\"{anchor}\"");
    }
    let _ = writeln!(html, ">");

    if let (Some(name), Some(number)) = (label, number) {
        render_equation_number(html, name, number);
    }

    match ctx.math.to_html(&latex, MathDisplay::Block) {
        Ok(mathml) => html.push_str(&mathml),
        Err(failure) => {
            html.push_str(&failure.html);
            ctx.math_errors.push(MathError {
                message: failure.message,
                span,
            });
        }
    }

    html.push_str("\n</div>\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_assemble_latex_leaves_a_single_plain_equation_alone() {
        // Given one equation with no alignment marks
        let parts = vec!["a = b".to_string()];

        // When
        let latex = assemble_latex(&parts);

        // Then nothing is wrapped around it
        assert_eq!(latex, "a = b");
    }

    #[test]
    fn test_assemble_latex_wraps_a_single_part_containing_a_row_break() {
        // Given one equation holding two aligned lines
        let parts = vec![r"a &= b \\ c &= d".to_string()];

        // When
        let latex = assemble_latex(&parts);

        // Then it gains an alignment environment
        assert_eq!(latex, r"\begin{aligned}a &= b \\ c &= d\end{aligned}");
    }

    #[test]
    fn test_assemble_latex_joins_several_parts_into_one_alignment() {
        // Given two separate equations
        let parts = vec!["a = b".to_string(), "c = d".to_string()];

        // When
        let latex = assemble_latex(&parts);

        // Then they are aligned with each other, separated by a row break
        assert_eq!(latex, r"\begin{aligned}a = b\\c = d\end{aligned}");
    }

    #[test]
    fn test_assemble_latex_wraps_each_multiline_part_and_the_whole_body() {
        // Given two equations, one of which is itself multi-line
        let parts = vec![r"a &= b \\ a &= c".to_string(), "d = e".to_string()];

        // When
        let latex = assemble_latex(&parts);

        // Then the inner part keeps its own alignment inside the outer one
        assert_eq!(
            latex,
            r"\begin{aligned}\begin{aligned}a &= b \\ a &= c\end{aligned}\\d = e\end{aligned}"
        );
    }

    #[test]
    fn test_assemble_latex_returns_nothing_for_an_empty_body() {
        // Given no equations at all
        let parts: Vec<String> = Vec::new();

        // When
        let latex = assemble_latex(&parts);

        // Then
        assert_eq!(latex, "");
    }

    #[test]
    fn test_equation_anchor_id_matches_sphinx() {
        // Given
        let label = TargetName::new("euler");

        // When / Then
        assert_eq!(equation_anchor_id(&label), "equation-euler");
    }

    #[test]
    fn test_render_equation_number_writes_the_number_and_permalink() {
        // Given
        let mut html = String::new();

        // When
        render_equation_number(&mut html, &TargetName::new("euler"), 3);

        // Then
        assert!(html.contains("<span class=\"eqno\">(3)"), "{html}");
        assert!(html.contains("href=\"#equation-euler\""), "{html}");
    }
}

#[cfg(test)]
mod render_tests {
    use rinx_ast::{Directive, Document, Node, TargetName};
    use rinx_index::{EquationLocation, ProjectIndex};

    fn math_node(label: Option<&str>, nowrap: bool, classes: Vec<String>, parts: &[&str]) -> Node {
        Node::Directive(Directive::Math {
            parts: parts.iter().map(|p| (*p).to_string()).collect(),
            label: label.map(TargetName::new),
            nowrap,
            classes,
            span: None,
        })
    }

    fn index_with(label: &str, doc_path: &str, number: usize) -> ProjectIndex {
        let mut index = ProjectIndex::default();
        index.equations.insert(
            TargetName::new(label),
            EquationLocation::new(doc_path, number),
        );
        index
    }

    fn render_with(index: &ProjectIndex, node: Node) -> crate::RenderOutput {
        let doc = Document::new("math.rst".to_string(), vec![node]);
        crate::render(&doc, index, &doc.path)
    }

    #[test]
    fn test_render_math_emits_a_block_equation_without_a_number_when_unlabeled() {
        // Given an unlabeled equation
        let output = render_with(
            &ProjectIndex::default(),
            math_node(None, false, vec![], &["a = b"]),
        );

        // Then it renders as display math with no number and no anchor
        assert!(
            output
                .html
                .contains("<div class=\"math notranslate nohighlight\">"),
            "{}",
            output.html
        );
        assert!(
            output.html.contains("<math display=\"block\">"),
            "{}",
            output.html
        );
        assert!(!output.html.contains("eqno"), "{}", output.html);
        assert!(!output.html.contains("id=\"equation-"), "{}", output.html);
        assert!(output.math_errors.is_empty());
    }

    #[test]
    fn test_render_math_numbers_and_anchors_a_labeled_equation() {
        // Given an equation the analyzer numbered
        let index = index_with("euler", "math.rst", 1);

        // When
        let output = render_with(&index, math_node(Some("euler"), false, vec![], &["a = b"]));

        // Then the anchor, number and permalink are all present
        assert!(
            output.html.contains("id=\"equation-euler\""),
            "{}",
            output.html
        );
        assert!(
            output.html.contains("<span class=\"eqno\">(1)"),
            "{}",
            output.html
        );
        assert!(
            output.html.contains("href=\"#equation-euler\""),
            "{}",
            output.html
        );
    }

    #[test]
    fn test_render_math_appends_the_class_option_to_the_wrapper() {
        // Given `:class: boxed`
        let output = render_with(
            &ProjectIndex::default(),
            math_node(None, false, vec!["boxed".to_string()], &["a = b"]),
        );

        // Then the class joins Sphinx's own on the wrapper
        assert!(
            output
                .html
                .contains("<div class=\"math notranslate nohighlight boxed\">"),
            "{}",
            output.html
        );
    }

    #[test]
    fn test_render_math_leaves_a_nowrap_body_unnumbered_even_when_labeled() {
        // Given a labeled `:nowrap:` equation — which the analyzer never
        // numbers, so the index has no entry for it
        let output = render_with(
            &ProjectIndex::default(),
            math_node(
                Some("raw"),
                true,
                vec![],
                [r"\begin{align}a &= b\end{align}"].as_ref(),
            ),
        );

        // Then neither a number nor an anchor is emitted, matching Sphinx
        assert!(!output.html.contains("eqno"), "{}", output.html);
        assert!(
            !output.html.contains("id=\"equation-raw\""),
            "{}",
            output.html
        );
        assert!(output.math_errors.is_empty(), "{:?}", output.math_errors);
    }

    #[test]
    fn test_render_math_reports_invalid_latex_and_keeps_the_source_visible() {
        // Given LaTeX the backend rejects
        let output = render_with(
            &ProjectIndex::default(),
            math_node(None, false, vec![], &[r"\frac{1}{2"]),
        );

        // Then the page still shows what the author wrote, and a warning is raised
        assert!(output.html.contains(r"\frac{1}{2"), "{}", output.html);
        assert!(output.html.contains("math-error"), "{}", output.html);
        assert_eq!(output.math_errors.len(), 1);
        assert!(
            output.math_errors[0]
                .message
                .contains("Expected closing token"),
            "{}",
            output.math_errors[0].message
        );
    }

    #[test]
    fn test_render_math_aligns_several_equations_as_one_block() {
        // Given two equations in one directive
        let output = render_with(
            &ProjectIndex::default(),
            math_node(None, false, vec![], &["a = b", "c = d"]),
        );

        // Then they render into a single aligned table, not two separate blocks
        assert_eq!(output.html.matches("<math").count(), 1, "{}", output.html);
        assert!(output.html.contains("<mtable"), "{}", output.html);
        assert!(output.math_errors.is_empty(), "{:?}", output.math_errors);
    }

    #[test]
    fn test_render_math_does_not_number_an_equation_defined_in_another_document() {
        // Given an index whose label points at a *different* document — the
        // shape a duplicated label across two files produces
        let index = index_with("euler", "other.rst", 1);

        // When this document renders its own equation under that label
        let output = render_with(&index, math_node(Some("euler"), false, vec![], &["a = b"]));

        // Then it claims no number, rather than borrowing the other file's
        assert!(!output.html.contains("eqno"), "{}", output.html);
    }
}

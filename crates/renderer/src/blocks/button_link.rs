//! `.. button-link::` rendering — sphinx-design's button-shaped external link.
//!
//! The elements this writes are what `_ButtonDirective.run_with_defaults`
//! builds, class for class, so a site already styled for sphinx-design keeps
//! its appearance:
//!
//! ```html
//! <p class="sd-text-center">                       <!-- :align: only -->
//!   <span class="sd-d-grid">                       <!-- :expand: only -->
//!     <a class="sd-sphinx-override sd-btn sd-text-wrap sd-btn-primary sd-shadow-sm"
//!        href="…" title="…">                       <!-- title from :tooltip: -->
//!       <span>…label…</span>
//!     </a>
//!   </span>
//! </p>
//! ```
//!
//! Four details are easy to get wrong and are therefore spelled out here. The
//! inner `<span>` is present **always**, label or no label: sphinx-design
//! wraps the content either way, and its stylesheet is written against that
//! shape. `:align:` lands on the *containing* `<p>` rather than on the button,
//! which is why there is a `<p>` at all — with no `:align:` it is a bare one.
//! `:outline:` produces no class at all without a `:color:` to outline, which
//! the parser reports as `button-link.unusable-outline`. And docutils' own
//! `reference external` classes are deliberately **not** emitted: every other
//! link in this build is a bare `<a href>` (see `inline/hyperlink.rs`), and two
//! link renderings that disagree would be the bug — unlike the trailing
//! `docutils` a grid keeps, no sphinx-design rule is written against them.

use std::fmt::Write as _;

use rinx_ast::{ButtonFlag, ButtonLink, ButtonTarget, inline_plain_text};

use crate::RenderCtx;
use crate::inline::render_inline;

/// Renders a `.. button-link::` directive.
pub(super) fn render_button_link(html: &mut String, button: &ButtonLink, ctx: &mut RenderCtx<'_>) {
    let _ = writeln!(html, "<p{}>", container_class_attribute(button));
    if button.has(ButtonFlag::Expand) {
        let _ = write!(html, "<span class=\"sd-d-grid\">");
    }

    let _ = write!(
        html,
        "<a class=\"{}\" href=\"{}\"{}>",
        html_escape::encode_double_quoted_attribute(&button_classes(button).join(" ")),
        html_escape::encode_double_quoted_attribute(href(&button.target)),
        title_attribute(button)
    );
    let _ = write!(html, "<span>");
    render_label(html, button, ctx);
    let _ = write!(html, "</span>");
    let _ = write!(html, "</a>");

    if button.has(ButtonFlag::Expand) {
        let _ = write!(html, "</span>");
    }
    let _ = writeln!(html, "\n</p>");
}

/// The href the button points at.
///
/// A function of its own, rather than a field read at the call site, because
/// it is the *only* thing a `.. button-ref::` would render differently — see
/// [`ButtonTarget`]. A second variant adds an arm here and changes nothing
/// else in this module.
fn href(target: &ButtonTarget) -> &str {
    match target {
        ButtonTarget::Url(url) => url,
    }
}

/// Renders the button's label, or the target when none was written.
///
/// A reference role inside the label is written as its text alone: the label
/// is already inside an `<a>`, and nesting a second one is invalid HTML. The
/// parser reports that as `button-link.nested-reference`, so this is the
/// quiet half of a diagnostic the author has already been given — which is
/// why it flattens rather than dropping the words.
fn render_label(html: &mut String, button: &ButtonLink, ctx: &mut RenderCtx<'_>) {
    if !button.has_label() {
        let _ = write!(
            html,
            "{}",
            html_escape::encode_text(button.target.display_text())
        );
        return;
    }
    for node in &button.label {
        if node.renders_as_link() {
            let _ = write!(
                html,
                "{}",
                html_escape::encode_text(&inline_plain_text(std::slice::from_ref(node)))
            );
        } else {
            render_inline(html, node, ctx);
        }
    }
}

/// The `<a>`'s classes, in sphinx-design's own order: the fixed three, the
/// colour, the two flags that add one, and the author's own.
fn button_classes(button: &ButtonLink) -> Vec<String> {
    let mut classes = vec![
        "sd-sphinx-override".to_string(),
        "sd-btn".to_string(),
        "sd-text-wrap".to_string(),
    ];
    // Only a written `:color:` adds a class — `:outline:` alone names no
    // colour to outline, so sphinx-design adds nothing at all.
    if let Some(color) = button.color {
        if button.has(ButtonFlag::Outline) {
            classes.push(format!("sd-btn-outline-{color}"));
        } else {
            classes.push(format!("sd-btn-{color}"));
        }
    }
    if button.has(ButtonFlag::ClickParent) {
        classes.push("sd-stretched-link".to_string());
    }
    if button.has(ButtonFlag::Shadow) {
        classes.push("sd-shadow-sm".to_string());
    }
    classes.extend(button.class.iter().cloned());
    classes
}

/// The containing `<p>`'s `class` attribute, which carries `:align:` alone —
/// empty, and so absent, when no alignment was written.
fn container_class_attribute(button: &ButtonLink) -> String {
    button.align.map_or_else(String::new, |align| {
        format!(
            " class=\"{}\"",
            html_escape::encode_double_quoted_attribute(&align.css_class())
        )
    })
}

/// The `title` attribute a `:tooltip:` produces, which is how sphinx-design's
/// `reftitle` reaches the page.
fn title_attribute(button: &ButtonLink) -> String {
    button.tooltip.as_ref().map_or_else(String::new, |tooltip| {
        format!(
            " title=\"{}\"",
            html_escape::encode_double_quoted_attribute(tooltip)
        )
    })
}

#[cfg(test)]
mod tests;

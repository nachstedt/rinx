//! `:option:` cross-reference rendering.

use std::fmt::Write as _;

use super::RefText;
use rusty_sphinx_ast::{ObjectType, StdObjectType};

use crate::resolution::{OptionResolution, OptionResolver};
use crate::{BrokenLink, BrokenLinkKind};

/// Renders a `:option:` cross-reference.
///
/// Delegates the search to [`crate::resolution::option`] (its module doc
/// comment documents the ambient-program/global-fallback/embedded-program
/// search order) and only decides here what each outcome looks like on the
/// page — the same split [`super::domain_object_reference::render_inline_domain_object_reference`]
/// makes for `:func:`/`:py:func:`/etc.
pub(super) fn render_inline_option_reference(
    html: &mut String,
    reference: RefText<'_>,
    resolver: &OptionResolver<'_>,
    ambient_program: Option<&str>,
    doc_path: &str,
    broken_links: &mut Vec<BrokenLink>,
) {
    let RefText {
        display,
        target,
        span,
    } = reference;
    let display_escaped = html_escape::encode_text(display);
    let literal =
        format!("<code class=\"xref std cmdoption docutils literal\">{display_escaped}</code>");

    match resolver.resolve(ambient_program, target) {
        OptionResolution::Resolved {
            qualified_name,
            doc_path: target_doc_path,
        } => {
            let anchor = rusty_sphinx_ast::build_domain_object_key(
                ObjectType::Std(StdObjectType::Cmdoption),
                &qualified_name,
            );
            let current_dir = std::path::Path::new(doc_path)
                .parent()
                .unwrap_or_else(|| std::path::Path::new(""));
            let target_html_path = std::path::Path::new(target_doc_path).with_extension("html");
            let relative_path =
                pathdiff::diff_paths(&target_html_path, current_dir).unwrap_or(target_html_path);
            let href = format!("{}#{}", relative_path.display(), anchor.as_str());
            let href_attr = html_escape::encode_double_quoted_attribute(&href);
            let _ = write!(
                html,
                "<a class=\"reference internal\" href=\"{href_attr}\">{literal}</a>"
            );
        }
        OptionResolution::NotFound => {
            let _ = write!(html, "<a href=\"#\" class=\"broken-link\">{literal}</a>");
            broken_links.push(BrokenLink {
                kind: BrokenLinkKind::OptionReference,
                target: target.to_string(),
                span,
            });
        }
    }
}

//! Rendering `.. entity-update::` / `.. needextend::` itself.
//!
//! On by default (see [`crate::config::SiteConfig::show_entity_updates`]),
//! the one deliberate divergence from sphinx-needs' own `needextend`, which
//! renders nothing at all: the whole point of this directive's traceable,
//! non-destructive design is the audit trail it leaves, and an audit trail
//! nobody can see on the page is not much of one. The mutation's *effect* —
//! what the entity it targets ends up showing — is a separate matter, handled
//! entirely by `rinx_analyzer::apply_entity_updates` before this ever
//! renders; this module only draws the directive's own record of what it
//! asked for and why.

use std::fmt::Write as _;

use rinx_ast::{EntityUpdate, FieldMutation, FieldMutationMode};

use crate::RenderCtx;

/// Whether `.. entity-update::`/`.. needextend::` renders its own visible box.
///
/// A two-variant enum rather than a `bool` on [`RenderCtx`] — see that
/// field's own doc comment for why — but the config file it comes from stays
/// a plain `bool`, the natural TOML shape; [`Self::from_config`] is the one
/// place the two meet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EntityUpdateVisibility {
    Show,
    Hide,
}

impl EntityUpdateVisibility {
    /// Reads the site config's plain boolean into this type.
    #[must_use]
    pub(crate) fn from_config(show: bool) -> Self {
        if show { Self::Show } else { Self::Hide }
    }

    /// Whether the directive should render its box at all.
    #[must_use]
    pub(crate) fn is_visible(self) -> bool {
        matches!(self, Self::Show)
    }
}

/// Renders an `.. entity-update::` / `.. needextend::`.
pub(super) fn render_entity_update(
    html: &mut String,
    update: &EntityUpdate,
    ctx: &mut RenderCtx<'_>,
) {
    let _ = write!(html, "<div class=\"entity-update\">");
    let _ = write!(
        html,
        "<p class=\"entity-update-target\"><code>{}</code> &rarr; <code>{}</code></p>",
        html_escape::encode_text(update.source.as_str()),
        html_escape::encode_text(&update.target.raw),
    );

    if !update.fields.is_empty() {
        html.push_str("<ul class=\"entity-update-fields\">");
        for field in &update.fields {
            render_field_mutation(html, field);
        }
        html.push_str("</ul>");
    }

    if !update.body.is_empty() {
        html.push_str("<div class=\"entity-update-justification\">");
        super::render_nodes(html, &update.body, ctx);
        html.push_str("</div>");
    }

    html.push_str("</div>");
}

/// One `<li>` describing a field mutation — its name, its operation and its
/// value, in a form close to the option line it was written on.
fn render_field_mutation(html: &mut String, field: &FieldMutation) {
    let (symbol, value) = match &field.mode {
        FieldMutationMode::Set(value) => ("", Some(value.as_str())),
        FieldMutationMode::Append(value) => ("+", Some(value.as_str())),
        FieldMutationMode::Remove(value) => ("-", Some(value.as_str())),
        FieldMutationMode::Clear => ("-", None),
    };
    let _ = write!(
        html,
        "<li><code>{symbol}{}</code>",
        html_escape::encode_text(&field.field)
    );
    if let Some(value) = value {
        let _ = write!(html, ": {}", html_escape::encode_text(value));
    } else {
        html.push_str(" (cleared)");
    }
    html.push_str("</li>");
}

#[cfg(test)]
mod tests;

//! What a `.. |name| directive::` substitution definition carries.
//!
//! Only three of docutils' five substitution-only directives are modelled:
//! `replace`, `unicode` and `image`. `date` is deliberately left out —
//! resolving it would mean baking the wall-clock time into a `.ast` file
//! during `parse`, which is a Bazel action cached on its inputs alone, so the
//! same source would need to produce different bytes on every build; that is
//! exactly the hermeticity `docs/decisions/007-image-assets.md` refuses a
//! `:loading: embed` of an external URL for. `raw` is left out because the
//! standalone `.. raw::` directive it would share its content model with is
//! itself unimplemented (see `docs/dev/spec_gaps.md`). Both fall back to
//! [`crate::Directive::Unknown`] like any other unrecognized directive name.

use serde::{Deserialize, Serialize};

use crate::image::ImageOptions;
use crate::inline_node::InlineNode;
use crate::span::Span;

/// A `.. |name| directive:: ...` substitution definition.
///
/// Produces no output of its own where it is written — like
/// [`crate::Directive::Highlight`], it exists purely to be resolved
/// elsewhere: every [`InlineNode::SubstitutionReference`] naming it is
/// spliced with [`Self::kind`]'s resolved content by a whole-document pass
/// that runs once parsing finishes (`rinx_parser`'s
/// `resolve_substitutions`), since a reference may be written before its
/// definition in source order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubstitutionDefinition {
    /// The text between the two vertical bars, exactly as written (case
    /// preserved) — matching a reference is case-sensitive first, falling
    /// back to a case-insensitive comparison, so the exact spelling matters.
    pub name: String,
    pub kind: SubstitutionKind,
    /// Where the definition was written, for the "duplicate definition" and
    /// "circular reference" diagnostics, which name a definition rather than
    /// a place text was written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
}

/// The substitution-only directive a [`SubstitutionDefinition`] embeds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubstitutionKind {
    /// `.. |name| replace:: text` — simple macro substitution. The content is
    /// inline-parsed exactly like a paragraph's, since docutils explicitly
    /// documents `replace` as "a workaround for the still missing support of
    /// nested inline markup" — it is the one substitution-only directive
    /// whose content may itself contain further substitution references,
    /// which is why it alone participates in the resolver's cycle detection.
    Replace(Vec<InlineNode>),
    /// `.. |name| unicode:: codes` — numeric/hex Unicode codepoints and XML
    /// character entities, already decoded into the literal text they name.
    Unicode { text: String, trim: TrimSides },
    /// `.. |name| image:: uri` — an inline image. Reuses [`ImageOptions`]
    /// verbatim; the substitution-only narrowings (`:name:` refused, the
    /// three vertical `:align:` values additionally accepted) are enforced by
    /// the parser before one of these is ever constructed, not by this type.
    Image(Box<ImageOptions>),
}

/// Whitespace to strip from the text immediately surrounding a `unicode`
/// substitution reference at its point of use — docutils' `:ltrim:`/
/// `:rtrim:`/`:trim:` options, unique to `unicode` among the substitution
/// directives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TrimSides {
    /// `:ltrim:`/`:trim:` — strip trailing whitespace off the text
    /// immediately before the reference.
    pub ltrim: bool,
    /// `:rtrim:`/`:trim:` — strip leading whitespace off the text
    /// immediately after the reference.
    pub rtrim: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_replace_definition_serialization_roundtrip() {
        // Given
        let definition = SubstitutionDefinition {
            name: "reST".to_string(),
            kind: SubstitutionKind::Replace(vec![InlineNode::Text("reStructuredText".to_string())]),
            span: None,
        };

        // When
        let json = serde_json::to_string(&definition).expect("should serialize");
        let restored: SubstitutionDefinition =
            serde_json::from_str(&json).expect("should deserialize");

        // Then
        assert_eq!(restored, definition);
    }

    #[test]
    fn test_unicode_definition_serialization_roundtrip() {
        // Given
        let definition = SubstitutionDefinition {
            name: "copy".to_string(),
            kind: SubstitutionKind::Unicode {
                text: "\u{a9}".to_string(),
                trim: TrimSides {
                    ltrim: true,
                    rtrim: false,
                },
            },
            span: None,
        };

        // When
        let json = serde_json::to_string(&definition).expect("should serialize");
        let restored: SubstitutionDefinition =
            serde_json::from_str(&json).expect("should deserialize");

        // Then
        assert_eq!(restored, definition);
    }

    #[test]
    fn test_image_definition_serialization_roundtrip() {
        // Given
        use crate::image::ImageUri;
        let definition = SubstitutionDefinition {
            name: "biohazard".to_string(),
            kind: SubstitutionKind::Image(Box::new(ImageOptions::new(ImageUri::new(
                "biohazard.png",
            )))),
            span: None,
        };

        // When
        let json = serde_json::to_string(&definition).expect("should serialize");
        let restored: SubstitutionDefinition =
            serde_json::from_str(&json).expect("should deserialize");

        // Then
        assert_eq!(restored, definition);
    }

    #[test]
    fn test_trim_sides_default_is_no_trimming() {
        // Given / When
        let trim = TrimSides::default();

        // Then
        assert!(!trim.ltrim);
        assert!(!trim.rtrim);
    }
}

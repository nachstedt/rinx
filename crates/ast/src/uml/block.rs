//! The diagram node itself: the template it draws from, and the options that
//! decide how the picture is placed on the page.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::entity::EntityId;
use crate::image::{ImageAlign, LengthOrPercentage};
use crate::span::Span;
use crate::target_name::TargetName;
use crate::uml::source::UmlSource;

/// A `PlantUML` diagram — `.. plantuml::`, `.. entity-diagram::`/`.. needuml::`
/// and `.. entity-arch::`/`.. needarch::` alike.
///
/// The node carries the diagram's **template**, not its picture and not even
/// its final `PlantUML` text. A templated diagram asks the entity graph
/// questions, and the entities it asks about live in documents this one has
/// never heard of — the same reason
/// [`EntityTable`](crate::EntityTable) carries a question rather than rows.
/// Expansion therefore happens once the project index exists, and the
/// resulting text is what gets hashed into the `_images/<hash>.svg` filename.
///
/// A plain `.. plantuml::` is the degenerate case: a template with nothing to
/// expand is its own expansion, so it flows through the identical path and
/// keeps the hash it has always had. That is what lets one pipeline serve
/// both, and it is why [`UmlSource::is_templated`] exists as a question about
/// the *spelling* rather than a second node type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Uml {
    /// Which of the six spellings was written.
    pub source: UmlSource,
    /// The directive's content: `PlantUML` source, possibly with Jinja markup
    /// in it. Kept exactly as the author wrote it, because this is the text a
    /// `:debug:` shows and the text a template error is reported against.
    pub template: String,
    /// `:key:` — the name this diagram is stored under on the entity it was
    /// written in, so another diagram can import it. Meaningless outside an
    /// entity, where it is diagnosed rather than silently ignored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// `:extra:` — extra names bound in the template's context, as written.
    ///
    /// A `BTreeMap` rather than a `Vec` of pairs because the expansion must be
    /// byte-for-byte deterministic: it feeds a content hash that decides
    /// whether `PlantUML` runs at all, so an iteration order that varies would
    /// defeat the build's caching.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, String>,
    /// `:config:` — the name of a `PlantUML` preamble to prepend. A *name*
    /// resolved against the site config, never a path (ADR-001).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<String>,
    /// `:debug:` — also show the expanded `PlantUML` source, as a code block
    /// after the picture.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub debug: bool,
    /// `:save:` — a project-relative path to write the expanded source to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub save: Option<String>,
    /// `:caption:` — shown under the picture.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
    /// `:align:` — horizontal placement, reusing the image directives' own
    /// vocabulary so a diagram and a picture align by the same rules.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align: Option<ImageAlign>,
    /// `:scale:` — a percentage, stored as written for the reason
    /// [`ImageOptions`](crate::ImageOptions) stores one: the renderer must be
    /// able to tell "scaled" from "sized".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<u32>,
    /// `:width:` — a length or a percentage of the available width.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<LengthOrPercentage>,
    /// `:class:` — space-separated class names, already split.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub classes: Vec<String>,
    /// `:name:` — reuses [`TargetName`] like the image and table directives
    /// do, so it registers in `ProjectIndex::targets` with no conversion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<TargetName>,
    /// The entity this diagram was written inside, when it was.
    ///
    /// Recorded while parsing, where the enclosing entity is already in hand,
    /// rather than rediscovered by every later phase walking outwards from the
    /// node. An `.. entity-arch::` binds it as `need`; the other spellings
    /// carry it too, since a diagram written inside an entity may reasonably
    /// draw the entity it sits in whatever it is spelled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity: Option<EntityId>,
    /// Where the directive was written, so a phase reporting a template error
    /// has a position to report it against. The node this one replaces carried
    /// none, which is why a broken diagram could not be located at all.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
}

impl Uml {
    /// A diagram with the given spelling and template, and no options.
    #[must_use]
    pub fn new(source: UmlSource, template: String) -> Self {
        Self {
            source,
            template,
            key: None,
            extra: BTreeMap::new(),
            config: None,
            debug: false,
            save: None,
            caption: None,
            align: None,
            scale: None,
            width: None,
            classes: Vec::new(),
            name: None,
            entity: None,
            span: None,
        }
    }

    /// The width to render with, `:scale:` applied.
    ///
    /// `None` when no `:width:` was written: exactly
    /// [`ImageOptions::rendered_width`](crate::ImageOptions::rendered_width)'s
    /// rule, because a diagram is placed on the page by the same arithmetic a
    /// picture is.
    #[must_use]
    pub fn rendered_width(&self) -> Option<LengthOrPercentage> {
        let width = self.width.as_ref()?;
        Some(match self.scale {
            Some(scale) => width.scaled(scale),
            None => width.clone(),
        })
    }

    /// Whether a `:scale:` was written that nothing can be applied to.
    ///
    /// The renderer never opens the compiled SVG, so there is no natural size
    /// to scale — the same reason an image's scale is unusable without a
    /// `:width:`, and reported the same way, by the parser.
    #[must_use]
    pub const fn has_unusable_scale(&self) -> bool {
        self.scale.is_some() && self.width.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_carries_the_spelling_and_template_with_no_options() {
        // Given / When
        let uml = Uml::new(UmlSource::PlantUml, "A -> B".to_string());

        // Then
        assert_eq!(uml.source, UmlSource::PlantUml);
        assert_eq!(uml.template, "A -> B");
        assert_eq!(uml.key, None);
        assert!(uml.extra.is_empty());
        assert_eq!(uml.config, None);
        assert!(!uml.debug);
        assert_eq!(uml.save, None);
        assert_eq!(uml.caption, None);
        assert_eq!(uml.align, None);
        assert_eq!(uml.scale, None);
        assert_eq!(uml.width, None);
        assert!(uml.classes.is_empty());
        assert_eq!(uml.name, None);
        assert_eq!(uml.entity, None);
        assert_eq!(uml.span, None);
    }

    #[test]
    fn test_rendered_width_applies_the_scale() {
        // Given
        let uml = Uml {
            width: Some(LengthOrPercentage::new("400px").expect("a valid length")),
            scale: Some(50),
            ..Uml::new(UmlSource::PlantUml, "A -> B".to_string())
        };

        // When
        let width = uml.rendered_width();

        // Then
        assert_eq!(width, Some(LengthOrPercentage::new("200px").unwrap()));
    }

    #[test]
    fn test_rendered_width_is_none_without_a_width() {
        // Given — a scale alone has nothing to apply to
        let uml = Uml {
            scale: Some(50),
            ..Uml::new(UmlSource::PlantUml, "A -> B".to_string())
        };

        // When / Then
        assert_eq!(uml.rendered_width(), None);
        assert!(uml.has_unusable_scale());
    }

    #[test]
    fn test_a_scale_with_a_width_is_usable() {
        // Given
        let uml = Uml {
            width: Some(LengthOrPercentage::new("400px").expect("a valid length")),
            scale: Some(50),
            ..Uml::new(UmlSource::PlantUml, "A -> B".to_string())
        };

        // When / Then
        assert!(!uml.has_unusable_scale());
    }

    #[test]
    fn test_round_trips_through_json() {
        // Given — every option set, so no field is silently dropped
        let uml = Uml {
            source: UmlSource::NeedArch,
            template: "{{ flow(need.id) }}".to_string(),
            key: Some("diagram".to_string()),
            extra: BTreeMap::from([("role".to_string(), "owner".to_string())]),
            config: Some("monochrome".to_string()),
            debug: true,
            save: Some("out/arch.puml".to_string()),
            caption: Some("Architecture".to_string()),
            align: Some(ImageAlign::Center),
            scale: Some(80),
            width: None,
            classes: vec!["wide".to_string()],
            name: Some(TargetName::new("my-diagram")),
            entity: Some(EntityId::new("REQ_001").expect("a valid id")),
            span: None,
        };

        // When
        let json = serde_json::to_string(&uml).expect("serializing cannot fail");
        let restored: Uml = serde_json::from_str(&json).expect("a written diagram reloads");

        // Then
        assert_eq!(restored, uml);
    }

    #[test]
    fn test_an_unset_option_is_left_out_of_the_json_entirely() {
        // Given — a plain `.. plantuml::`, which is by far the common case and
        // whose .ast entry should not grow because templated diagrams exist
        let uml = Uml::new(UmlSource::PlantUml, "A -> B".to_string());

        // When
        let json = serde_json::to_string(&uml).expect("serializing cannot fail");

        // Then
        assert!(!json.contains("key"), "{json}");
        assert!(!json.contains("debug"), "{json}");
        assert!(!json.contains("caption"), "{json}");
    }
}

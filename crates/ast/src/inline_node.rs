use serde::{Deserialize, Serialize};

use crate::image::ImageOptions;
use crate::inventory_selector::InventorySelector;
use crate::object_type::ObjectType;
use crate::span::Span;
use crate::target_search_order::TargetSearchOrder;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InlineNode {
    Text(String),
    /// An inline cross-reference produced by the `:ref:` role, linking to a
    /// labeled location elsewhere in the site.
    ///
    /// `display` is the explicit title of the angle-bracket form, and `None`
    /// when the author wrote the bare label. The two are kept apart rather
    /// than defaulting `display` to `target` while parsing, because the text a
    /// bare `:ref:` shows is the *section title* the label points at — which
    /// only the project index knows, so only the renderer can supply it.
    Reference {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        display: Option<String>,
        target: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        span: Option<Span>,
        /// Which inventories the target may come from — see
        /// [`InventorySelector`]. Left out of a `.ast` file when ordinary.
        #[serde(default, skip_serializing_if = "InventorySelector::is_any")]
        inventory: InventorySelector,
    },
    Hyperlink {
        text: String,
        target: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        span: Option<Span>,
    },
    AnonymousReference {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        span: Option<Span>,
    },
    AnonymousHyperlink {
        text: String,
        target: String,
    },
    Emphasis(String),
    Strong(String),
    Literal(String),
    Program(String),
    /// An inline cross-reference produced by the term role, linking to a glossary entry.
    ///
    /// The `display` field is the visible link text and `term` is the glossary key.
    /// They differ when the role is written with an explicit display-text override,
    /// i.e. the angle-bracket form where the text before the angle bracket is shown
    /// and the text inside the angle brackets is looked up in the glossary index.
    TermReference {
        display: String,
        term: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        span: Option<Span>,
        /// Which inventories the target may come from — see
        /// [`InventorySelector`]. Left out of a `.ast` file when ordinary.
        #[serde(default, skip_serializing_if = "InventorySelector::is_any")]
        inventory: InventorySelector,
    },
    /// An inline cross-reference produced by a domain role (e.g. `:func:`,
    /// `:py:func:`, `:c:func:`), linking to a `Directive::DomainObject`.
    ///
    /// `object_type` is always concrete by the time this node exists — the
    /// parser resolves a bare (unprefixed) role via the file's default
    /// domain immediately, mirroring how `Directive::DomainObject` is
    /// resolved.
    ///
    /// `name` and `display` differ when the role target uses a `~` prefix
    /// (e.g. `~pkg.mod.func`): `name` is the full name used to resolve the
    /// cross-reference, `display` is the shortened text shown to the reader
    /// (just the last dotted component). A `!` prefix instead sets `link`
    /// to `false`, suppressing the hyperlink entirely (the target is never
    /// looked up, so a missing target produces no broken-link warning).
    ///
    /// A leading `.` prefix is likewise consumed by the parser: it never
    /// survives into `name` or `display` (it is markup, not part of any
    /// object's name), and is recorded as `search_order` instead.
    ///
    /// A *trailing* `()` — written so the reference reads as a call at the
    /// point of use (`` :c:func:`Py_TYPE()` ``) — is markup too, and splits
    /// the other way round from `~`: it is stripped from `name`, because no
    /// declaration ever registers a name with parens in it, but kept in
    /// `display`, because that is what the reader is meant to see. Real
    /// Sphinx arrives at the same split by stripping the parens at resolution
    /// time and never showing them to the title; doing it while parsing keeps
    /// `name` a plain name at every later phase.
    DomainObjectReference {
        object_type: ObjectType,
        name: String,
        display: String,
        link: bool,
        /// Which resolution order the target asked for. `#[serde(default)]`
        /// keeps `.ast` files written before this field existed loadable —
        /// they predate leading-dot support, so the default (no dot) is the
        /// faithful reading.
        #[serde(default)]
        search_order: TargetSearchOrder,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        span: Option<Span>,
        /// Which inventories the target may come from — see
        /// [`InventorySelector`]. Left out of a `.ast` file when ordinary.
        #[serde(default, skip_serializing_if = "InventorySelector::is_any")]
        inventory: InventorySelector,
    },
    /// An inline cross-reference produced by the `:option:` role, linking to
    /// a `.. option::`/`.. cmdoption::` definition.
    ///
    /// Unlike [`Self::DomainObjectReference`], this carries no `object_type`
    /// (always `std:cmdoption`) and no `search_order` — `:option:`'s
    /// resolution is a distinct ambient-program/global-fallback/embedded-
    /// program search (see `rinx_renderer::resolution::option`), not
    /// the dot-prefixed most/least-qualified search [`TargetSearchOrder`]
    /// models. `target` is carried through close to verbatim: only the
    /// explicit-title split (this role's `` `display <target>` `` syntax)
    /// happens at parse time, because the rest of the algorithm depends on
    /// the *merged* project index, not on anything knowable from one file's
    /// parse.
    OptionReference {
        display: String,
        target: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        span: Option<Span>,
        /// Which inventories the target may come from — see
        /// [`InventorySelector`]. Left out of a `.ast` file when ordinary.
        #[serde(default, skip_serializing_if = "InventorySelector::is_any")]
        inventory: InventorySelector,
    },
    /// A reference to a project-declared entity, from a role the schema
    /// names — ``:req:`REQ_001` ``, ``:need:`REQ_001` ``, or the built-in
    /// ``:entity:`REQ_001` ``.
    ///
    /// The `role` is kept rather than resolved to a set of acceptable types,
    /// because the types it accepts are a *schema* fact and this node has to
    /// survive into a `.ast` file that outlives the process which parsed it.
    /// Resolution, and the type check the role exists for, happen where the
    /// merged project index is available.
    EntityReference {
        role: String,
        target: String,
        display: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        span: Option<Span>,
    },
    /// Inline math produced by the `:math:` role, holding LaTeX verbatim.
    ///
    /// A verbatim context like [`Self::Literal`]: the backslashes are the
    /// content, so this is one of the two variants whose escape markers turn
    /// back into backslashes rather than being dropped (see
    /// `rinx_parser`'s `unescape_node`).
    ///
    /// Carries a span despite not being a cross-reference — unlike every other
    /// self-contained variant, its content can be rejected at render time, and
    /// the resulting diagnostic needs a position.
    Math {
        latex: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        span: Option<Span>,
    },
    /// An inline cross-reference produced by the `:eq:` role, linking to a
    /// labeled `.. math::` and displaying that equation's number.
    ///
    /// Carries no `display` field, unlike every other cross-reference role:
    /// `:eq:` has no explicit-title form, because the visible text is the
    /// equation number, which is not known until the project index exists.
    EquationReference {
        label: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        span: Option<Span>,
    },
    /// A `|name|` substitution reference, before the whole-document
    /// resolution pass (`rinx_parser`'s `resolve_substitutions`)
    /// replaces it with its definition's resolved content.
    ///
    /// An intermediate node rather than something a later phase ever sees:
    /// resolution is intra-document and runs at the end of `parse()`, so a
    /// well-formed `Document` never carries one of these by the time it is
    /// returned. An unresolvable name still degrades to this being replaced
    /// with a literal `Text("|name|")` (plus a diagnostic) rather than
    /// staying — see [`crate::DiagnosticCode::SubstitutionUndefined`].
    SubstitutionReference {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        span: Option<Span>,
    },
    /// What a `.. |name| image::` substitution reference resolves to: an
    /// image inline in running text rather than a block of its own.
    ///
    /// Boxed for the same reason [`crate::Directive::Image`] is — an
    /// [`ImageOptions`] is large, and an enum costs its largest variant on
    /// every node.
    InlineImage(Box<ImageOptions>),
}

impl InlineNode {
    /// Where this node's markup was written, for the variants that can
    /// produce a diagnostic; `None` for every other variant.
    ///
    /// Only the cross-reference roles and [`Self::Math`] carry a position,
    /// because only they can fail at render time — the roles by not resolving,
    /// `Math` by holding LaTeX the math backend rejects. Giving every variant
    /// one would double the size of a parsed document to record something
    /// nothing reads.
    #[must_use]
    pub const fn span(&self) -> Option<Span> {
        match self {
            Self::Reference { span, .. }
            | Self::Hyperlink { span, .. }
            | Self::AnonymousReference { span, .. }
            | Self::TermReference { span, .. }
            | Self::DomainObjectReference { span, .. }
            | Self::OptionReference { span, .. }
            | Self::EntityReference { span, .. }
            | Self::Math { span, .. }
            | Self::EquationReference { span, .. }
            | Self::SubstitutionReference { span, .. } => *span,
            _ => None,
        }
    }

    /// Records where this node's markup was written, for the variants that
    /// carry a position; every other variant is returned unchanged.
    ///
    /// Exists so the inline scan can attach positions in exactly one place —
    /// it is the only code that knows a match's offsets — instead of every
    /// role parser having to thread them through its own construction.
    #[must_use]
    pub fn with_span(mut self, at: Option<Span>) -> Self {
        match &mut self {
            Self::Reference { span, .. }
            | Self::Hyperlink { span, .. }
            | Self::AnonymousReference { span, .. }
            | Self::TermReference { span, .. }
            | Self::DomainObjectReference { span, .. }
            | Self::OptionReference { span, .. }
            | Self::EntityReference { span, .. }
            | Self::Math { span, .. }
            | Self::EquationReference { span, .. }
            | Self::SubstitutionReference { span, .. } => *span = at,
            _ => {}
        }
        self
    }

    /// Whether this node renders as a hyperlink — an `<a>` element.
    ///
    /// Exists for the one construct that cannot contain one: a
    /// [`crate::ButtonLink`]'s label is itself inside an `<a>`, and nesting
    /// two would be invalid HTML. The parser reports such a label and the
    /// renderer flattens it, and both ask this rather than keeping a list of
    /// variants each — two lists that a new linking role would silently leave
    /// disagreeing.
    ///
    /// A [`Self::DomainObjectReference`] written with a `!` prefix has
    /// `link: false` and is deliberately *not* a link: its target is never
    /// looked up, so it nests nothing.
    #[must_use]
    pub const fn renders_as_link(&self) -> bool {
        match self {
            Self::Reference { .. }
            | Self::Hyperlink { .. }
            | Self::AnonymousReference { .. }
            | Self::AnonymousHyperlink { .. }
            | Self::TermReference { .. }
            | Self::OptionReference { .. }
            | Self::EntityReference { .. }
            | Self::EquationReference { .. } => true,
            Self::DomainObjectReference { link, .. } => *link,
            _ => false,
        }
    }
}

/// Flattens a sequence of inline nodes down to the plain text a reader would
/// see, discarding all markup/links. Used where a data field requires a
/// plain `String` — e.g. a nav-sidebar label or an HTML `<title>` — rather
/// than for rendering visible HTML body content (which uses `InlineNode`
/// directly so links/emphasis are preserved).
#[must_use]
pub fn inline_plain_text(nodes: &[InlineNode]) -> String {
    nodes
        .iter()
        .map(|node| match node {
            InlineNode::Text(text)
            | InlineNode::Emphasis(text)
            | InlineNode::Strong(text)
            | InlineNode::Literal(text)
            | InlineNode::Program(text)
            | InlineNode::AnonymousReference { text, .. } => text.as_str(),
            InlineNode::Hyperlink { text, .. } | InlineNode::AnonymousHyperlink { text, .. } => {
                text.as_str()
            }
            // Without the index, a bare label's section title is unknown, so
            // the label itself is the best plain text there is.
            InlineNode::Reference {
                display, target, ..
            } => display.as_deref().unwrap_or(target),
            InlineNode::TermReference { display, .. }
            | InlineNode::DomainObjectReference { display, .. }
            | InlineNode::OptionReference { display, .. }
            | InlineNode::EntityReference { display, .. } => display.as_str(),
            // The LaTeX source is the only plain text an equation has: its
            // rendered form is markup, and its `:eq:` number isn't known
            // without the project index this function deliberately doesn't take.
            InlineNode::Math { latex, .. } => latex.as_str(),
            InlineNode::EquationReference { label, .. } => label.as_str(),
            // Never reaches a caller of this function in a well-formed
            // document — resolved away by the end of parsing — but degrades
            // to the written name rather than vanishing if one somehow does.
            InlineNode::SubstitutionReference { name, .. } => name.as_str(),
            // An image has no text of its own beyond its `:alt:`, which
            // docutils falls back to the URI for; this function takes no
            // resolver and cannot know the URI's resolved form, so an unset
            // `:alt:` contributes nothing rather than a wrong guess.
            InlineNode::InlineImage(options) => options.alt.as_deref().unwrap_or(""),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::object_type::PyObjectType;

    #[test]
    fn test_renders_as_link_is_true_for_every_reference_role() {
        // Given — one node per variant the renderer sends to an `<a>`
        let links = vec![
            InlineNode::Reference {
                display: Some("d".to_string()),
                target: "t".to_string(),
                span: None,
                inventory: crate::InventorySelector::Any,
            },
            InlineNode::Hyperlink {
                text: "t".to_string(),
                target: "u".to_string(),
                span: None,
            },
            InlineNode::AnonymousReference {
                text: "t".to_string(),
                span: None,
            },
            InlineNode::AnonymousHyperlink {
                text: "t".to_string(),
                target: "u".to_string(),
            },
            InlineNode::TermReference {
                display: "d".to_string(),
                term: "t".to_string(),
                span: None,
                inventory: crate::InventorySelector::Any,
            },
            InlineNode::OptionReference {
                display: "d".to_string(),
                target: "t".to_string(),
                span: None,
                inventory: crate::InventorySelector::Any,
            },
            InlineNode::EntityReference {
                role: "req".to_string(),
                target: "REQ_1".to_string(),
                display: "REQ_1".to_string(),
                span: None,
            },
        ];

        // When / Then
        for node in links {
            assert!(
                node.renders_as_link(),
                "{node:?} should be reported as a link"
            );
        }
    }

    #[test]
    fn test_renders_as_link_is_false_for_markup_that_is_not_a_link() {
        // Given
        let plain = vec![
            InlineNode::Text("t".to_string()),
            InlineNode::Emphasis("t".to_string()),
            InlineNode::Strong("t".to_string()),
            InlineNode::Literal("t".to_string()),
            InlineNode::Program("t".to_string()),
        ];

        // When / Then
        for node in plain {
            assert!(!node.renders_as_link(), "{node:?} should not be a link");
        }
    }

    #[test]
    fn test_a_suppressed_domain_object_reference_is_not_a_link() {
        // Given — the `!` prefix form, whose target is never looked up
        let suppressed = InlineNode::DomainObjectReference {
            object_type: ObjectType::Py(PyObjectType::Function),
            name: "pkg.f".to_string(),
            display: "pkg.f".to_string(),
            link: false,
            search_order: TargetSearchOrder::default(),
            span: None,
            inventory: crate::InventorySelector::Any,
        };
        let linked = InlineNode::DomainObjectReference {
            object_type: ObjectType::Py(PyObjectType::Function),
            name: "pkg.f".to_string(),
            display: "pkg.f".to_string(),
            link: true,
            search_order: TargetSearchOrder::default(),
            span: None,
            inventory: crate::InventorySelector::Any,
        };

        // When / Then
        assert!(!suppressed.renders_as_link());
        assert!(linked.renders_as_link());
    }

    #[test]
    fn test_inline_node_program_serialization_roundtrip() {
        // Given
        let node = InlineNode::Program("curl".to_string());

        // When
        let json = serde_json::to_string(&node).expect("Failed to serialize");
        let deserialized: InlineNode = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(node, deserialized);
    }

    #[test]
    fn test_option_reference_serialization_roundtrip() {
        // Given
        let node = InlineNode::OptionReference {
            display: "-O <dis --show-offsets>".to_string(),
            target: "dis --show-offsets".to_string(),
            span: None,
            inventory: crate::InventorySelector::Any,
        };

        // When
        let json = serde_json::to_string(&node).expect("Failed to serialize");
        let deserialized: InlineNode = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(node, deserialized);
    }

    #[test]
    fn test_reference_serialization_roundtrip() {
        // Given
        let node = InlineNode::Reference {
            display: Some("GenericAlias".to_string()),
            target: "types-genericalias".to_string(),
            span: None,
            inventory: crate::InventorySelector::Any,
        };

        // When
        let json = serde_json::to_string(&node).expect("Failed to serialize");
        let deserialized: InlineNode = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(node, deserialized);
    }

    #[test]
    fn test_reference_without_explicit_title_roundtrips_without_display() {
        // Given — a bare `:ref:`, whose title only the index can supply
        let node = InlineNode::Reference {
            display: None,
            target: "home-index".to_string(),
            span: None,
            inventory: crate::InventorySelector::Any,
        };

        // When
        let json = serde_json::to_string(&node).expect("Failed to serialize");
        let deserialized: InlineNode = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert!(!json.contains("display"));
        assert_eq!(node, deserialized);
    }

    #[test]
    fn test_term_reference_serialization_roundtrip() {
        // Given
        let node = InlineNode::TermReference {
            display: "the environment".to_string(),
            term: "environment".to_string(),
            span: None,
            inventory: crate::InventorySelector::Any,
        };

        // When
        let json = serde_json::to_string(&node).expect("Failed to serialize");
        let deserialized: InlineNode = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(node, deserialized);
    }

    #[test]
    fn test_term_reference_display_equals_term_when_no_alias() {
        // Given
        let term_text = "environment";

        // When
        let node = InlineNode::TermReference {
            display: term_text.to_string(),
            term: term_text.to_string(),
            span: None,
            inventory: crate::InventorySelector::Any,
        };

        // Then
        if let InlineNode::TermReference { display, term, .. } = node {
            assert_eq!(display, term);
        } else {
            panic!("Expected TermReference");
        }
    }

    #[test]
    fn test_domain_object_reference_serialization_roundtrip() {
        // Given
        let node = InlineNode::DomainObjectReference {
            object_type: ObjectType::Py(PyObjectType::Function),
            name: "foo".to_string(),
            display: "foo".to_string(),
            link: true,
            search_order: TargetSearchOrder::MostQualifiedFirst,
            span: None,
            inventory: crate::InventorySelector::Any,
        };

        // When
        let json = serde_json::to_string(&node).expect("Failed to serialize");
        let deserialized: InlineNode = serde_json::from_str(&json).expect("Failed to deserialize");

        // Then
        assert_eq!(node, deserialized);
    }

    #[test]
    fn test_domain_object_reference_deserializes_without_search_order_field() {
        // Given — an `.ast` file written before leading-dot support existed.
        let json = r#"{"DomainObjectReference":{"object_type":"py:function","name":"foo","display":"foo","link":true}}"#;

        // When
        let deserialized: InlineNode = serde_json::from_str(json).expect("Failed to deserialize");

        // Then — it reads as an unprefixed target, not a parse failure.
        assert_eq!(
            deserialized,
            InlineNode::DomainObjectReference {
                object_type: ObjectType::Py(PyObjectType::Function),
                name: "foo".to_string(),
                display: "foo".to_string(),
                link: true,
                search_order: TargetSearchOrder::LeastQualifiedFirst,
                span: None,
                inventory: crate::InventorySelector::Any,
            }
        );
    }

    #[test]
    fn test_inline_plain_text_concatenates_plain_text_nodes() {
        // Given
        let nodes = vec![
            InlineNode::Text("Hello ".to_string()),
            InlineNode::Strong("world".to_string()),
        ];

        // When
        let text = inline_plain_text(&nodes);

        // Then
        assert_eq!(text, "Hello world");
    }

    #[test]
    fn test_inline_plain_text_uses_display_for_reference() {
        // Given — an explicit-title `:ref:`
        let nodes = vec![InlineNode::Reference {
            display: Some("GenericAlias".to_string()),
            target: "types-genericalias".to_string(),
            span: None,
            inventory: crate::InventorySelector::Any,
        }];

        // When
        let text = inline_plain_text(&nodes);

        // Then
        assert_eq!(text, "GenericAlias");
    }

    #[test]
    fn test_inline_plain_text_falls_back_to_the_label_for_a_bare_reference() {
        // Given — a bare `:ref:`; the section title is not known without an index
        let nodes = vec![InlineNode::Reference {
            display: None,
            target: "types-genericalias".to_string(),
            span: None,
            inventory: crate::InventorySelector::Any,
        }];

        // When
        let text = inline_plain_text(&nodes);

        // Then
        assert_eq!(text, "types-genericalias");
    }

    #[test]
    fn test_inline_plain_text_uses_shortened_display_for_domain_object_reference() {
        // Given — a `~`-shortened domain-object reference
        let nodes = vec![InlineNode::DomainObjectReference {
            object_type: ObjectType::Py(PyObjectType::Module),
            name: "pkg.submodule".to_string(),
            display: "submodule".to_string(),
            link: true,
            search_order: TargetSearchOrder::LeastQualifiedFirst,
            span: None,
            inventory: crate::InventorySelector::Any,
        }];

        // When
        let text = inline_plain_text(&nodes);

        // Then
        assert_eq!(text, "submodule");
    }

    #[test]
    fn test_inline_plain_text_uses_display_for_term_reference() {
        // Given
        let nodes = vec![InlineNode::TermReference {
            display: "the env".to_string(),
            term: "environment".to_string(),
            span: None,
            inventory: crate::InventorySelector::Any,
        }];

        // When
        let text = inline_plain_text(&nodes);

        // Then
        assert_eq!(text, "the env");
    }

    #[test]
    fn test_inline_plain_text_returns_empty_string_for_empty_input() {
        assert_eq!(inline_plain_text(&[]), "");
    }
}

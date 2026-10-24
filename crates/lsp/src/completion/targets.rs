//! Completing a role's target: what the role may name, and which of those
//! the page would link from where the role is written.
//!
//! [`candidates`] lists every target of the kind the role names, from the
//! project index and the other sites' inventories, each with the spellings it
//! might be written in; [`select`] keeps those the resolver leads somewhere,
//! writes each as short as it still leads there, ranks the nearest first and
//! cuts a long list.

mod candidates;
mod select;

use lsp_types::{CompletionItem, Range};
use rinx_ast::InlineNode;
use rinx_entity::EntitySchema;
use rinx_index::ProjectIndex;
use rinx_renderer::ReferenceResolver;

use candidates::{Asking, candidates};

/// What a probe writes where the target being typed is, so the role reads as
/// a finished one: any name will do, since the probe asks what the role is,
/// never where it leads.
pub const PLACEHOLDER: &str = "x";

/// Whether `reference` is resolved in the scope it is written in — a domain
/// role, `:option:`, `:any:` — so completing it needs that scope.
#[must_use]
pub fn is_scoped(reference: &InlineNode) -> bool {
    matches!(
        reference,
        InlineNode::DomainObjectReference { .. }
            | InlineNode::OptionReference { .. }
            | InlineNode::AnyReference { .. }
    )
}

/// The text a probe of the line `line_index` of `text` parses: the lines
/// above it, then `closed` in its place — or, `whole`, every line with
/// `closed` in its place, for a templated document a cut could break.
#[must_use]
pub fn probe_text(text: &str, line_index: usize, closed: &str, whole: bool) -> String {
    let mut probe = String::with_capacity(text.len());
    for (at, line) in text.lines().enumerate() {
        if at == line_index {
            probe.push_str(closed);
        } else if at < line_index || whole {
            probe.push_str(line);
        } else {
            break;
        }
        probe.push('\n');
    }
    probe
}

/// What a target completion is asked about.
pub struct TargetQuestion<'a> {
    /// A node standing for the role being typed: the parser's reading of it,
    /// in the document `resolver` answers for when the role is resolved in
    /// its scope.
    pub reference: &'a InlineNode,
    pub resolver: &'a ReferenceResolver<'a>,
    pub index: &'a ProjectIndex,
    pub schema: &'a EntitySchema,
    /// The document being completed in.
    pub doc_path: &'a str,
    /// What the author has written of the target, after its `~` and `.`.
    pub typed: &'a str,
    /// What a completion replaces.
    pub range: Range,
}

/// The items completing the target `question` asks about, and whether the
/// answer is incomplete, so the editor asks again as the author types.
#[must_use]
pub fn target_items(question: &TargetQuestion<'_>) -> (Vec<CompletionItem>, bool) {
    let asking = Asking {
        doc_path: question.doc_path,
        absolute: question.typed.starts_with('/'),
    };
    let found = candidates(question.reference, question.index, question.schema, &asking);
    select::select(
        found,
        question.reference,
        question.resolver,
        question.typed,
        question.range,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_probe_text_closes_the_line_and_drops_what_follows() {
        // When / Then
        assert_eq!(
            probe_text("a\n:ref:`in\nc\n", 1, ":ref:`x`", false),
            "a\n:ref:`x`\n"
        );
    }

    #[test]
    fn test_probe_text_keeps_every_line_when_whole() {
        // When / Then
        assert_eq!(
            probe_text("a\n:ref:`in\nc", 1, ":ref:`x`", true),
            "a\n:ref:`x`\nc\n"
        );
    }

    #[test]
    fn test_is_scoped_holds_for_the_roles_resolved_in_a_scope() {
        // Given
        let any = InlineNode::AnyReference {
            display: None,
            target: "x".to_string(),
            link: true,
            span: None,
            inventory: rinx_ast::InventorySelector::Any,
        };
        let equation = InlineNode::EquationReference {
            label: "x".to_string(),
            span: None,
        };

        // When / Then
        assert!(is_scoped(&any));
        assert!(!is_scoped(&equation));
    }
}

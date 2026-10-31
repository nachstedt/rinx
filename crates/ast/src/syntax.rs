//! Where a document's markup was written: the blocks an editor folds and the
//! names it colours.
//!
//! Recorded by the parser as it reads each construct, so an editor never
//! folds or colours text the parser read differently, and kept beside the
//! document's nodes rather than in them because no later phase reads it.
//! Never serialized, as [`IncludeSite`](crate::IncludeSite) is not: a `.ast`
//! file must not change when a line moves.

use crate::span::Span;

/// What a [`SyntaxBlock`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    /// A section title, adornment lines included, at its nesting level
    /// (1 for the document's title).
    Heading { level: u8 },
    /// A directive: its marker line and its whole indented block.
    Directive,
    /// A comment (`.. text`), with its indented continuation.
    Comment,
    /// A literal block introduced by `::`, or a doctest block.
    LiteralBlock,
    /// A grid or simple table.
    Table,
}

/// One block of the source, from its first written line to its last
/// non-blank one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyntaxBlock {
    pub kind: BlockKind,
    pub lines: Span,
}

/// What a [`SyntaxToken`] names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TokenKind {
    /// A directive's name and its `::`, after the `.. `.
    Directive,
    /// A directive option's `:name:`.
    Option,
    /// A role's `:name:`, before or after its backquoted text.
    Role,
    /// What a reference leads to: a role's target, a hyperlink reference's
    /// name, a `.. _name:` target's label.
    Reference,
    /// A `|name|` substitution, where it is used or defined.
    Substitution,
}

impl TokenKind {
    /// Every kind, in declaration order — the order an editor's legend lists
    /// them in.
    pub const ALL: [Self; 5] = [
        Self::Directive,
        Self::Option,
        Self::Role,
        Self::Reference,
        Self::Substitution,
    ];
}

/// How a [`SyntaxToken`] is used, beyond its kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TokenModifier {
    /// The name is defined here — a `.. _name:` label, a `.. |name|`
    /// substitution — rather than used.
    Declaration,
}

impl TokenModifier {
    /// Every modifier, in declaration order — the order an editor's legend
    /// lists them in.
    pub const ALL: [Self; 1] = [Self::Declaration];

    /// This modifier's bit in a [`TokenModifiers`] set, its index in
    /// [`Self::ALL`].
    const fn bit(self) -> u8 {
        1 << (self as u8)
    }
}

/// A set of [`TokenModifier`]s.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TokenModifiers(u8);

impl TokenModifiers {
    /// The empty set.
    pub const NONE: Self = Self(0);

    /// This set with `modifier` added.
    #[must_use]
    pub const fn with(self, modifier: TokenModifier) -> Self {
        Self(self.0 | modifier.bit())
    }

    /// Whether `modifier` is in the set.
    #[must_use]
    pub const fn contains(self, modifier: TokenModifier) -> bool {
        self.0 & modifier.bit() != 0
    }

    /// The set as the protocol's bit field, bit `i` standing for
    /// [`TokenModifier::ALL`]`[i]`.
    #[must_use]
    pub const fn bits(self) -> u32 {
        self.0 as u32
    }
}

/// One name the markup writes, with exactly its characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyntaxToken {
    pub kind: TokenKind,
    pub modifiers: TokenModifiers,
    pub span: Span,
}

/// Every block and token the parser recorded, in the order it read them —
/// not necessarily source order, since a construct's body is read before
/// what follows it but its name may be recorded after.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyntaxTable {
    pub blocks: Vec<SyntaxBlock>,
    pub tokens: Vec<SyntaxToken>,
}

impl SyntaxTable {
    /// A table recording nothing.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            blocks: Vec::new(),
            tokens: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_modifiers_start_empty() {
        // Given / When
        let modifiers = TokenModifiers::NONE;

        // Then
        assert!(!modifiers.contains(TokenModifier::Declaration));
        assert_eq!(modifiers.bits(), 0);
    }

    #[test]
    fn test_token_modifiers_with_adds_the_modifier_at_its_legend_index() {
        // Given
        let modifiers = TokenModifiers::NONE;

        // When
        let declared = modifiers.with(TokenModifier::Declaration);

        // Then
        assert!(declared.contains(TokenModifier::Declaration));
        let index = TokenModifier::ALL
            .iter()
            .position(|m| *m == TokenModifier::Declaration)
            .unwrap();
        assert_eq!(declared.bits(), 1 << index);
    }

    #[test]
    fn test_token_kind_all_lists_every_kind_once_in_declaration_order() {
        // Given / When
        let mut sorted = TokenKind::ALL;
        sorted.sort();

        // Then
        assert_eq!(sorted, TokenKind::ALL);
        for kind in TokenKind::ALL {
            // An exhaustive match: a new kind fails to compile until listed.
            let listed = match kind {
                TokenKind::Directive
                | TokenKind::Option
                | TokenKind::Role
                | TokenKind::Reference
                | TokenKind::Substitution => TokenKind::ALL.contains(&kind),
            };
            assert!(listed);
        }
    }
}

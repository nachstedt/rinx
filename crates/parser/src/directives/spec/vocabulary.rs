//! The vocabulary the directive table is written in: what one directive's
//! [`DirectiveSpec`] says.
//!
//! A leaf: it names no directive, so every module can build its spec from it
//! without depending on the registry that collects them — which directives
//! exist is the registry's to say.
//!
//! How a spec says a block splits — its [`Arguments`], whether it takes
//! options, and whether it takes content — is docutils' own `Directive` contract
//! (`required_arguments`, `optional_arguments`, `final_argument_whitespace`,
//! `option_spec`, `has_content`), transcribed from each directive's class in
//! the project that defines it. It says nothing about how a directive is
//! written down, so a reader of any syntax splits a block by the same shape.
//!
//! The [`OptionSpec`]s a spec lists are each directive's option enum's rows
//! (see `directives/options.rs`'s `directive_options!`): the parser matches on
//! that enum, so an option cannot be read without being listed here, nor
//! listed without being read.

/// One directive this build implements.
///
/// A built-in directive's spec is a `DirectiveSpec<'static>` constant beside
/// its parser; an entity type's borrows the schema declaring it (see
/// `directives/entity_spec.rs`).
#[derive(Debug, Clone, Copy)]
pub struct DirectiveSpec<'a> {
    /// Every spelling the directive is written as: this build's own name
    /// first, then the ones borrowed for compatibility (`entity-table`, then
    /// sphinx-needs' `needtable`).
    pub names: &'a [&'a str],
    /// What the directive is for, in one sentence.
    pub doc: &'a str,
    /// The arguments it takes.
    pub arguments: Arguments,
    /// Whether it takes content: docutils' `has_content`.
    pub takes_content: bool,
    /// What to write instead, when content is refused — said beside the
    /// refusal, since a directive taking none is often one letter away from
    /// one that takes some.
    pub content_hint: Option<&'a str>,
    /// The options it takes.
    pub options: OptionSet<'a>,
    /// Whether it also takes options beyond those, which its interpreter
    /// reads itself: `.. entity-update::`'s field mutations, the schema's
    /// fields, and `.. needservice::`'s, the external service's.
    pub accepts_further: bool,
}

impl<'a> DirectiveSpec<'a> {
    /// Every option it takes, its own first.
    pub fn option_specs(&self) -> impl Iterator<Item = &'a OptionSpec<'a>> + use<'a> {
        self.options.iter()
    }

    /// Whether it takes any options: docutils' `option_spec`, which decides
    /// whether the lines directly below the marker are read for them.
    #[must_use]
    pub const fn takes_options(&self) -> bool {
        !self.options.is_empty() || self.accepts_further
    }
}

/// The options a directive takes, in groups: its own, then each group it
/// shares with other directives (the table presentation options, …).
#[derive(Debug, Clone, Copy)]
pub struct OptionSet<'a> {
    /// Its own options.
    pub own: &'a [OptionSpec<'a>],
    /// The groups it shares.
    pub shared: &'a [&'a [OptionSpec<'a>]],
}

impl<'a> OptionSet<'a> {
    /// No option at all.
    pub const NONE: Self = Self {
        own: &[],
        shared: &[],
    };

    /// Every option of the set, its own first.
    pub fn iter(&self) -> impl Iterator<Item = &'a OptionSpec<'a>> + use<'a> {
        let shared = self.shared;
        self.own
            .iter()
            .chain(shared.iter().flat_map(|group| group.iter()))
    }

    /// Whether the set holds no option.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        if !self.own.is_empty() {
            return false;
        }
        let mut index = 0;
        while index < self.shared.len() {
            if !self.shared[index].is_empty() {
                return false;
            }
            index += 1;
        }
        true
    }
}

/// One option a directive takes.
#[derive(Debug, Clone, Copy)]
pub struct OptionSpec<'a> {
    /// Its canonical spelling (`link_types`).
    pub name: &'a str,
    /// The other spellings it is written as (`link-types`).
    pub aliases: &'a [&'a str],
    /// What its value is.
    pub value: OptionValue<'a>,
    /// What it does, in one sentence.
    pub doc: &'a str,
    /// Whether this build implements it.
    pub support: OptionSupport<'a>,
}

impl<'a> OptionSpec<'a> {
    /// Every spelling it is written as, the canonical one first.
    pub fn names(&self) -> impl Iterator<Item = &'a str> + use<'a> {
        std::iter::once(self.name).chain(self.aliases.iter().copied())
    }

    /// Whether it is written as `spelling`.
    #[must_use]
    pub fn is_spelled(&self, spelling: &str) -> bool {
        self.name == spelling || self.aliases.contains(&spelling)
    }
}

/// Whether this build implements an option the reference implementation
/// takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionSupport<'a> {
    /// It is read.
    Supported,
    /// It is reported and ignored, with what to write instead, if anything.
    Unsupported {
        /// What to reach for instead.
        advice: Option<&'a str>,
    },
}

/// What an option's value is, as an author writes it.
#[derive(Debug, Clone, Copy)]
pub enum OptionValue<'a> {
    /// No value: the option's presence is the setting.
    Flag,
    /// Free text.
    Text,
    /// A comma-separated list of free-text items: an entity's
    /// `list<string>` attribute.
    TextList,
    /// A whole number.
    Integer,
    /// A length, with or without a unit: `200px`, `50%`.
    Length,
    /// One of a fixed set of words, computed from the enum the parser reads
    /// the value into, so the list cannot drift from what it accepts.
    Choice(fn() -> Vec<&'static str>),
    /// One of the words a schema declares: an entity's `enum` attribute.
    Declared(&'a [String]),
    /// A comma-separated list of the words a schema declares: an entity's
    /// `list<enum>` attribute.
    DeclaredList(&'a [String]),
    /// Space-separated class names.
    Classes,
    /// A path to a file of the project.
    Path,
    /// An address.
    Uri,
    /// A filter expression over the entity graph.
    Filter,
    /// A comma-separated list of entity ids: an entity's relation, naming
    /// the types its targets may have — `None` for every type.
    EntityIds(Option<&'a [String]>),
}

impl OptionValue<'_> {
    /// Whether the value is a comma-separated list of items.
    #[must_use]
    pub const fn is_list(&self) -> bool {
        matches!(
            self,
            Self::TextList | Self::DeclaredList(_) | Self::EntityIds(_)
        )
    }
}

/// The arguments a directive takes, as docutils declares them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Arguments {
    /// How many it requires.
    pub required: usize,
    /// How many more it accepts.
    pub optional: usize,
    /// Whether its last argument keeps the rest of the text, whitespace and
    /// line breaks included, rather than surplus words being refused.
    pub final_whitespace: bool,
}

impl Arguments {
    /// No argument at all.
    pub const NONE: Self = Self {
        required: 0,
        optional: 0,
        final_whitespace: false,
    };
    /// One required word: `.. highlight:: python`.
    pub const ONE_WORD: Self = Self {
        required: 1,
        optional: 0,
        final_whitespace: false,
    };
    /// One required argument keeping the rest of the text:
    /// `.. image:: logo.png`, `.. admonition:: A title`.
    pub const ONE_TEXT: Self = Self {
        required: 1,
        optional: 0,
        final_whitespace: true,
    };
    /// One optional word: `.. code-block:: python`.
    pub const OPTIONAL_WORD: Self = Self {
        required: 0,
        optional: 1,
        final_whitespace: false,
    };
    /// One optional argument keeping the rest of the text: a title.
    pub const OPTIONAL_TEXT: Self = Self {
        required: 0,
        optional: 1,
        final_whitespace: true,
    };

    /// Whether it takes any argument, required or optional.
    #[must_use]
    pub const fn takes_any(self) -> bool {
        self.required + self.optional > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_list_holds_for_the_three_list_values() {
        // Given / When / Then
        assert!(OptionValue::TextList.is_list());
        assert!(OptionValue::DeclaredList(&[]).is_list());
        assert!(OptionValue::EntityIds(None).is_list());
        assert!(!OptionValue::Text.is_list());
        assert!(!OptionValue::Declared(&[]).is_list());
    }

    #[test]
    fn test_takes_any_is_false_only_for_none() {
        // Given / When / Then
        assert!(!Arguments::NONE.takes_any());
        assert!(Arguments::ONE_WORD.takes_any());
        assert!(Arguments::OPTIONAL_TEXT.takes_any());
    }
}

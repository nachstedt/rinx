//! The vocabulary the directive table is written in: what one directive's
//! [`DirectiveSpec`] says.
//!
//! A leaf: it names no directive, so every module can build its spec from it
//! without depending on the registry that collects them — which directives
//! exist is the registry's to say.
//!
//! The [`DirectiveShape`] a spec declares — its [`Arguments`], whether it
//! takes options, and whether it takes content — is docutils' own `Directive` contract
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
#[derive(Debug)]
pub struct DirectiveSpec {
    /// Every spelling the directive is written as: this build's own name
    /// first, then the ones borrowed for compatibility (`entity-table`, then
    /// sphinx-needs' `needtable`).
    pub names: &'static [&'static str],
    /// What the directive is for, in one sentence.
    pub doc: &'static str,
    /// How a block of it splits into arguments, options and content.
    pub shape: DirectiveShape,
    /// The options it takes, in groups: its own first, then each group it
    /// shares with other directives (the table presentation options, …).
    pub options: &'static [&'static [OptionSpec]],
    /// Whether it also takes options beyond those, which its interpreter
    /// reads itself: `.. entity-update::`'s field mutations, the schema's
    /// fields, and `.. needservice::`'s, the external service's.
    pub accepts_further: bool,
}

impl DirectiveSpec {
    /// Every option it takes, its own first.
    pub fn option_specs(&self) -> impl Iterator<Item = &'static OptionSpec> {
        self.options.iter().flat_map(|group| group.iter())
    }
}

/// One option a directive takes.
#[derive(Debug, Clone, Copy)]
pub struct OptionSpec {
    /// Every spelling it is written as, the canonical one first
    /// (`link_types`, then `link-types`).
    pub names: &'static [&'static str],
    /// What its value is.
    pub value: OptionValue,
    /// What it does, in one sentence.
    pub doc: &'static str,
    /// Whether this build implements it.
    pub support: OptionSupport,
}

/// Whether this build implements an option the reference implementation
/// takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionSupport {
    /// It is read.
    Supported,
    /// It is reported and ignored, with what to write instead, if anything.
    Unsupported {
        /// What to reach for instead.
        advice: Option<&'static str>,
    },
}

/// What an option's value is, as an author writes it.
#[derive(Debug, Clone, Copy)]
pub enum OptionValue {
    /// No value: the option's presence is the setting.
    Flag,
    /// Free text.
    Text,
    /// A whole number.
    Integer,
    /// A length, with or without a unit: `200px`, `50%`.
    Length,
    /// One of a fixed set of words, computed from the enum the parser reads
    /// the value into, so the list cannot drift from what it accepts.
    Choice(fn() -> Vec<&'static str>),
    /// Space-separated class names.
    Classes,
    /// A path to a file of the project.
    Path,
    /// An address.
    Uri,
    /// A filter expression over the entity graph.
    Filter,
}

/// How a directive's block splits into arguments, options and content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirectiveShape {
    /// The arguments it takes.
    pub arguments: Arguments,
    /// Whether it takes any options: docutils' `option_spec`, which decides
    /// whether the lines directly below the marker are read for them.
    pub takes_options: bool,
    /// Whether it takes content: docutils' `has_content`.
    pub takes_content: bool,
    /// What to write instead, when content is refused — said beside the
    /// refusal, since a directive taking none is often one letter away from
    /// one that takes some.
    pub content_hint: Option<&'static str>,
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
    fn test_takes_any_is_false_only_for_none() {
        // Given / When / Then
        assert!(!Arguments::NONE.takes_any());
        assert!(Arguments::ONE_WORD.takes_any());
        assert!(Arguments::OPTIONAL_TEXT.takes_any());
    }
}

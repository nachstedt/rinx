//! The one body parse every domain object type except `py:module` shares:
//! read the option block, then parse the rest as the object's content.
//!
//! Sphinx declares the object-description flags (`:no-index:` and its
//! siblings) once, on `ObjectDescription`, so they are one group,
//! [`DescriptionOption`], that every type's option enum includes; what a type
//! adds of its own (`py:method`'s `:async:`, `py:data`'s `:type:`) arrives
//! through its [`ObjectOptions`]. Reading the whole block
//! with the shared [`scan_option_lines`] — rather than letting each type stop
//! at the first line it does not know — is what keeps an option a type does
//! not take from leaking into its content as text, and having one place
//! slice the options off is what keeps every position below them right.

use crate::diagnostics::Diagnostics;
use crate::directives::invocation::DirectiveInvocation;
use crate::directives::options::{
    DirectiveOptions, OptionLine, classify_options, directive_options,
};
use crate::headings::Adornment;
use rinx_ast::{DescriptionFlag, DescriptionFlags, Node};

directive_options! {
    /// Sphinx's object-description flags, which every object type takes.
    pub(super) enum DescriptionOption {
        /// `:no-index:`.
        Unindexed => (["no-index", "noindex"], Flag, "Makes the object neither a reference target nor an index entry."),
        /// `:no-index-entry:`.
        OutOfIndex => (["no-index-entry", "noindexentry"], Flag, "Leaves the object out of the general index."),
        /// `:no-contents-entry:`.
        OutOfContents => (["no-contents-entry", "nocontentsentry"], Flag, "Leaves the object out of the table of contents."),
        _ => (["no-typesetting"], Flag, "Indexes the object without drawing it."),
    }
}

impl DescriptionOption {
    /// The flag this option sets.
    pub(super) const fn flag(self) -> DescriptionFlag {
        match self {
            Self::Unindexed => DescriptionFlag::NoIndex,
            Self::OutOfIndex => DescriptionFlag::NoIndexEntry,
            Self::OutOfContents => DescriptionFlag::NoContentsEntry,
        }
    }
}

/// The options one object type takes, the object-description flags among
/// them.
pub(super) trait ObjectOptions: Default {
    /// The type's options, as an enum including [`DescriptionOption`].
    type Option: DirectiveOptions;

    /// Records one option, a description flag into `flags`.
    fn read(&mut self, option: Self::Option, line: &OptionLine, flags: &mut DescriptionFlags);
}

directive_options! {
    /// The options of a type Sphinx gives only the flags: `std:cmdoption`.
    pub(super) enum PlainObjectOption {
        ;
        Description(DescriptionOption),
    }
}

/// A type that takes no option of its own — `std:cmdoption`.
impl ObjectOptions for () {
    type Option = PlainObjectOption;

    fn read(
        &mut self,
        option: PlainObjectOption,
        _line: &OptionLine,
        flags: &mut DescriptionFlags,
    ) {
        match option {
            PlainObjectOption::Description(option) => flags.set(option.flag()),
        }
    }
}

/// What a domain object's body parsed into.
pub(super) struct ObjectBody<O> {
    pub(super) options: O,
    pub(super) flags: DescriptionFlags,
    pub(super) content: Vec<Node>,
}

/// Parses a domain object's body: its options, reporting every option
/// `directive` does not take, and then its content — under a context keeping
/// the section-title permission of wherever the object stands, as Sphinx
/// parses an object description's content with `allow_section_headings=True`.
pub(super) fn parse_object_body<O: ObjectOptions>(
    directive: &str,
    invocation: &DirectiveInvocation<'_, '_>,
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
) -> ObjectBody<O> {
    let mut options = O::default();
    let mut flags = DescriptionFlags::default();
    for (option, line) in
        classify_options::<O::Option>(&invocation.options, directive, diagnostics, &invocation.ctx)
    {
        options.read(option, line, &mut flags);
    }

    let content = invocation
        .content_with_enclosing_titles()
        .parse(adornment_order, diagnostics);
    ObjectBody {
        options,
        flags,
        content,
    }
}

#[cfg(test)]
pub(super) mod test_support {
    use super::ObjectOptions;
    use crate::directives::options::{DirectiveOptions, scan_option_lines};
    use rinx_ast::DescriptionFlags;

    /// What `body` — an object's body lines, already unindented — reads as:
    /// the type's options, the flags, and the names of the options neither
    /// claimed.
    pub(in crate::directives::domains) fn read_options<O: ObjectOptions>(
        body: &[&str],
    ) -> (O, DescriptionFlags, Vec<String>) {
        let lines: Vec<String> = body.iter().map(ToString::to_string).collect();
        let (option_lines, _) = scan_option_lines(&lines);
        let mut options = O::default();
        let mut flags = DescriptionFlags::default();
        let mut unrecognized = Vec::new();
        for line in &option_lines {
            match O::Option::from_name(&line.name) {
                Some(option) => options.read(option, line, &mut flags),
                None => unrecognized.push(line.name.clone()),
            }
        }
        (options, flags, unrecognized)
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::read_options;
    use super::*;
    use crate::parse;
    use rinx_ast::{DescriptionFlag, DiagnosticCode, Directive, DomainObjectBody};

    #[test]
    fn test_read_object_options_reads_every_flag_and_leaves_the_rest() {
        // Given — a type with no options of its own.
        let body = [
            ":no-index:",
            ":noindexentry:",
            ":no-contents-entry:",
            ":final:",
        ];

        // When
        let ((), flags, unrecognized) = read_options::<()>(&body);

        // Then
        assert_eq!(
            flags,
            DescriptionFlags::of([
                DescriptionFlag::NoIndex,
                DescriptionFlag::NoIndexEntry,
                DescriptionFlag::NoContentsEntry,
            ])
        );
        assert_eq!(unrecognized, ["final"]);
    }

    #[test]
    fn test_read_object_options_reads_nothing_after_the_blank_line() {
        // Given — a docstring opening with a field list, below the blank line
        // that says there is no option block.
        let body = ["", ":param x: The value."];

        // When
        let ((), flags, unrecognized) = read_options::<()>(&body);

        // Then
        assert_eq!(flags, DescriptionFlags::default());
        assert!(unrecognized.is_empty());
    }

    #[test]
    fn test_parse_object_body_keeps_positions_below_the_options() {
        // Given — two option lines between the marker and the content.
        let input = ".. c:struct:: s\n   :no-index:\n   :no-index-entry:\n\n   .. bar::\n";

        // When
        let doc = parse("test.rst", input);

        // Then — the unknown directive is on line 5, where it was written.
        let unknown = doc
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == DiagnosticCode::DirectiveUnknown)
            .expect("the unknown directive is reported");
        assert_eq!(unknown.span.map(|span| span.start.line), Some(5));
    }

    #[test]
    fn test_parse_object_body_reports_an_option_the_type_does_not_take() {
        // Given
        let input = ".. c:function:: void f(void)\n   :final:\n\n   Body.\n";

        // When
        let doc = parse("test.rst", input);

        // Then — reported on its own line, and kept out of the content.
        let unknown = doc
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == DiagnosticCode::DirectiveUnknownOption)
            .expect("the unknown option is reported");
        assert_eq!(unknown.span.map(|span| span.start.line), Some(2));
        assert!(
            unknown.message.contains("c:function"),
            "{}",
            unknown.message
        );
        let Some(Node::Directive(Directive::DomainObject(DomainObjectBody::CFunction {
            body,
            ..
        }))) = doc.nodes.first()
        else {
            panic!("Expected CFunction, got {:?}", doc.nodes);
        };
        assert_eq!(body.len(), 1);
    }
}

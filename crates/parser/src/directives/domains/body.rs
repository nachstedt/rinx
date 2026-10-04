//! The one body parse every domain object type except `py:module` shares:
//! read the option block, then parse the rest as the object's content.
//!
//! Sphinx declares the object-description flags (`:no-index:` and its
//! siblings) once, on `ObjectDescription`, so they are read here for every
//! type; what a type adds of its own (`py:method`'s `:async:`, `py:data`'s
//! `:type:`) arrives through its [`ObjectOptions`]. Reading the whole block
//! with the shared [`scan_option_lines`] — rather than letting each type stop
//! at the first line it does not know — is what keeps an option a type does
//! not take from leaking into its content as text, and having one place
//! slice the options off is what keeps every position below them right.

use crate::blocks::parse_blocks;
use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::directives::options::{OptionLine, report_unknown_options, scan_option_lines};
use crate::headings::Adornment;
use crate::indent::unindent_body_lines;
use rinx_ast::{DescriptionFlag, DescriptionFlags, DiagnosticCode, Node};

/// The options one object type takes beyond the object-description flags.
pub(super) trait ObjectOptions: Default {
    /// Records `line` if it is one of this type's own options, and says
    /// whether it was.
    fn read(&mut self, line: &OptionLine) -> bool;
}

/// A type that takes no option of its own — `c:function`, `c:struct`,
/// `std:cmdoption` and every other type Sphinx gives only the flags.
impl ObjectOptions for () {
    fn read(&mut self, _line: &OptionLine) -> bool {
        false
    }
}

/// What a domain object's body parsed into.
pub(super) struct ObjectBody<O> {
    pub(super) options: O,
    pub(super) flags: DescriptionFlags,
    pub(super) content: Vec<Node>,
}

/// Parses a domain object's body: its option block, reporting every option
/// `directive` does not take, and then its content under a context rebased
/// past the options.
pub(super) fn parse_object_body<O: ObjectOptions>(
    directive: &str,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> ObjectBody<O> {
    let unindented_lines = unindent_body_lines(body_lines);
    let (option_lines, body_start) = scan_option_lines(&unindented_lines);
    let (options, flags, unrecognized) = read_object_options::<O>(&option_lines);
    report_unknown_options(
        &unrecognized,
        directive,
        DiagnosticCode::DirectiveUnknownOption,
        diagnostics,
        ctx,
    );

    // The content starts below the option block, so every position inside it
    // is short by that many lines unless the context is rebased first.
    let content_ctx = ctx.nested(body_start, 0);
    let content_lines: Vec<&str> = unindented_lines[body_start..]
        .iter()
        .map(String::as_str)
        .collect();
    let content = parse_blocks(&content_lines, adornment_order, diagnostics, &content_ctx);
    ObjectBody {
        options,
        flags,
        content,
    }
}

/// Interprets a domain object's option lines: the object-description flags,
/// the type's own options, and the lines that are neither.
fn read_object_options<O: ObjectOptions>(
    option_lines: &[OptionLine],
) -> (O, DescriptionFlags, Vec<&OptionLine>) {
    let mut options = O::default();
    let mut flags = DescriptionFlags::default();
    let mut unrecognized = Vec::new();
    for line in option_lines {
        if let Some(flag) = DescriptionFlag::from_option_name(&line.name) {
            flags.set(flag);
        } else if !options.read(line) {
            unrecognized.push(line);
        }
    }
    (options, flags, unrecognized)
}

#[cfg(test)]
pub(super) mod test_support {
    use super::{ObjectOptions, read_object_options, scan_option_lines};
    use rinx_ast::DescriptionFlags;

    /// What `body` — an object's body lines, already unindented — reads as:
    /// the type's options, the flags, and the names of the options neither
    /// claimed.
    pub(in crate::directives::domains) fn read_options<O: ObjectOptions>(
        body: &[&str],
    ) -> (O, DescriptionFlags, Vec<String>) {
        let lines: Vec<String> = body.iter().map(ToString::to_string).collect();
        let (option_lines, _) = scan_option_lines(&lines);
        let (options, flags, unrecognized) = read_object_options::<O>(&option_lines);
        let names = unrecognized.iter().map(|line| line.name.clone()).collect();
        (options, flags, names)
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::read_options;
    use super::*;
    use crate::parse;
    use rinx_ast::{DescriptionFlag, Directive, DomainObjectBody};

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

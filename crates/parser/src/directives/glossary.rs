//! Glossary directive parsing for RST documents.

use crate::blocks::parse_blocks;
use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::directives::invocation::{DirectiveInvocation, ParseState, single};
use crate::directives::spec::{Arguments, DirectiveSpec};
use crate::headings::Adornment;
use crate::indent::{indent_width, unindent_body_lines};
use rinx_ast::{Directive, GlossaryTerm, Node};

use super::options::{DirectiveOptions, classify_options, directive_options};

/// Dedents one accumulated definition body and parses it, rebasing `ctx` by
/// both the lines skipped to reach it and its own indentation.
///
/// The lines in `definition_lines` still carry their original leading
/// whitespace — the loop above only checks *that* a line is indented, not by
/// how much — so parsing them as-is would hand `parse_blocks` a body that
/// still looks indented relative to its own baseline, which a block quote
/// would now (correctly) wrap around the whole definition. Stripping that
/// indentation here, the same way every other multi-line body in this crate
/// dedents before recursing, keeps a definition's ordinary indentation from
/// reading as a quote.
fn parse_definition_body(
    definition_lines: &[String],
    body_start: usize,
    definition_start: usize,
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Vec<Node> {
    let def_refs: Vec<&str> = definition_lines.iter().map(String::as_str).collect();
    let definition_indent = def_refs
        .iter()
        .find(|line| !line.trim().is_empty())
        .map_or(0, |line| indent_width(line));
    let dedented = unindent_body_lines(&def_refs);
    let dedented_refs: Vec<&str> = dedented.iter().map(String::as_str).collect();
    let def_ctx = ctx.nested(body_start + definition_start, definition_indent);
    parse_blocks(&dedented_refs, adornment_order, diagnostics, &def_ctx)
}

directive_options! {
    /// The options `.. glossary::` takes: Sphinx's one.
    enum GlossaryOption {
        /// `:sorted:`.
        Sorted => (["sorted"], Flag, "Sorts the entries alphabetically."),
    }
}

/// This directive's entry in the table of directives this build implements.
pub(in crate::directives) const SPEC: DirectiveSpec = DirectiveSpec {
    names: &["glossary"],
    doc: "Terms and their definitions, for `:term:` to link to.",
    arguments: Arguments::NONE,
    takes_content: true,
    content_hint: None,
    options: <GlossaryOption as DirectiveOptions>::OPTIONS,
    accepts_further: false,
};

/// Interprets `.. glossary::`.
pub(in crate::directives) fn interpret_glossary(
    input: &DirectiveInvocation<'_, '_>,
    state: &mut ParseState<'_>,
) -> Vec<rinx_ast::Node> {
    single(parse_glossary(
        input,
        state.adornment_order,
        state.diagnostics,
    ))
}

/// Parses a `.. glossary::` directive body into a `Directive::Glossary` node.
pub(super) fn parse_glossary(
    invocation: &DirectiveInvocation<'_, '_>,
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
) -> Directive {
    let mut sorted = false;
    for (option, _) in classify_options::<GlossaryOption>(
        &invocation.options,
        "glossary",
        diagnostics,
        &invocation.ctx,
    ) {
        match option {
            GlossaryOption::Sorted => sorted = true,
        }
    }

    let content = invocation.content();
    let ctx = &content.ctx;
    let unindented = content.lines;
    let body_start = 0;

    // Parse definition-list entries from the remaining lines.
    // A line with NO leading whitespace (after base-indent stripping) is a term.
    // A line WITH leading whitespace is part of the definition.
    let mut entries: Vec<rinx_ast::GlossaryEntry> = Vec::new();
    let mut current_terms: Vec<GlossaryTerm> = Vec::new();
    let mut definition_lines: Vec<String> = Vec::new();
    let mut in_definition = false;

    let body_slice = &unindented[body_start..];

    // Where the definition being accumulated started, as an index into
    // `body_slice`, so its nested parse can be positioned in the document.
    let mut definition_start = 0;

    for (offset, line) in body_slice.iter().enumerate() {
        let is_blank = line.trim().is_empty();
        let is_indented = line.starts_with(' ') || line.starts_with('\t');

        if is_blank {
            if in_definition {
                // Blank line may end the current definition or be part of it.
                // We flush on the next term; accumulate for now.
                definition_lines.push(String::new());
            }
            // Between entries: do nothing
            continue;
        }

        if is_indented {
            // Part of the current definition body.
            if !in_definition {
                definition_start = offset;
            }
            in_definition = true;
            definition_lines.push(line.clone());
        } else {
            // Non-indented: this is a term.
            if in_definition {
                // Flush the completed entry.
                let mut dummy_adorn = adornment_order.clone();
                let def_nodes = parse_definition_body(
                    &definition_lines,
                    body_start,
                    definition_start,
                    &mut dummy_adorn,
                    diagnostics,
                    ctx,
                );
                entries.push(rinx_ast::GlossaryEntry {
                    terms: std::mem::take(&mut current_terms),
                    definition: def_nodes,
                });
                definition_lines.clear();
                in_definition = false;
            }
            current_terms.push(GlossaryTerm {
                text: line.trim().to_string(),
                span: ctx.line_span(body_start + offset, line.trim_end()),
            });
        }
    }

    // Flush any remaining entry.
    if !current_terms.is_empty() {
        let def_nodes = parse_definition_body(
            &definition_lines,
            body_start,
            definition_start,
            adornment_order,
            diagnostics,
            ctx,
        );
        entries.push(rinx_ast::GlossaryEntry {
            terms: current_terms,
            definition: def_nodes,
        });
    }

    // Sort alphabetically by the first term (case-insensitive) if :sorted: was set.
    if sorted {
        entries.sort_by(|a, b| {
            let a_key = a
                .terms
                .first()
                .map(|term| term.text.to_lowercase())
                .unwrap_or_default();
            let b_key = b
                .terms
                .first()
                .map(|term| term.text.to_lowercase())
                .unwrap_or_default();
            a_key.cmp(&b_key)
        });
    }

    Directive::Glossary { entries, sorted }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::Domain;

    #[test]
    fn test_parse_glossary_parses_a_single_term_and_definition() {
        // Given a glossary body with one term and an indented definition
        let body_lines = vec!["   term", "      Definition text."];
        let mut adornment_order = Vec::new();
        let mut diagnostics = Diagnostics::default();

        // When parsing the glossary directive body
        let ctx = ParseCtx::with_domain(Domain::Py);
        let invocation = crate::directives::reader::test_invocation(
            "glossary",
            &SPEC,
            "",
            &body_lines,
            &mut diagnostics,
            &ctx,
        );
        let directive = parse_glossary(&invocation, &mut adornment_order, &mut diagnostics);

        // Then a single entry with the parsed term is produced
        if let Directive::Glossary { entries, sorted } = directive {
            assert_eq!(entries.len(), 1);
            let terms: Vec<&str> = entries[0]
                .terms
                .iter()
                .map(|term| term.text.as_str())
                .collect();
            assert_eq!(terms, ["term"]);
            assert!(!sorted);
        } else {
            panic!("Expected Glossary directive");
        }
    }

    #[test]
    fn test_parse_records_each_terms_line_in_the_document() {
        // Given — two terms sharing a definition, and a second entry
        let input = "Intro\n\n.. glossary::\n\n   API\n   interface\n      A contract.\n\n   ABI\n      A binary one.\n";

        // When
        let doc = crate::parse_document("test.rst", input);

        // Then — each term at its own line, its column past the body's indent
        let Some(Node::Directive(Directive::Glossary { entries, .. })) = doc.nodes.get(1) else {
            panic!("expected a glossary, got {:?}", doc.nodes);
        };
        let spans: Vec<(&str, Option<rinx_ast::Span>)> = entries
            .iter()
            .flat_map(|entry| &entry.terms)
            .map(|term| (term.text.as_str(), term.span))
            .collect();
        let at = |line, end| {
            Some(rinx_ast::Span::new(
                rinx_ast::Position::new(line, 4),
                rinx_ast::Position::new(line, end),
            ))
        };
        assert_eq!(
            spans,
            vec![
                ("API", at(5, 7)),
                ("interface", at(6, 13)),
                ("ABI", at(9, 7))
            ]
        );
    }

    #[test]
    fn test_parse_glossary_parses_sorted_option() {
        // Given a glossary body starting with the :sorted: option
        let body_lines = vec!["   :sorted:", "", "   term", "      Definition text."];
        let mut adornment_order = Vec::new();
        let mut diagnostics = Diagnostics::default();

        // When parsing the glossary directive body
        let ctx = ParseCtx::with_domain(Domain::Py);
        let invocation = crate::directives::reader::test_invocation(
            "glossary",
            &SPEC,
            "",
            &body_lines,
            &mut diagnostics,
            &ctx,
        );
        let directive = parse_glossary(&invocation, &mut adornment_order, &mut diagnostics);

        // Then the sorted flag is set
        if let Directive::Glossary { sorted, .. } = directive {
            assert!(sorted);
        } else {
            panic!("Expected Glossary directive");
        }
    }

    #[test]
    fn test_parse_glossary_returns_empty_entries_when_body_has_no_non_blank_line() {
        // Given a glossary body containing only blank lines
        let body_lines = vec!["", "   "];
        let mut adornment_order = Vec::new();
        let mut diagnostics = Diagnostics::default();

        // When parsing the glossary directive body
        let ctx = ParseCtx::with_domain(Domain::Py);
        let invocation = crate::directives::reader::test_invocation(
            "glossary",
            &SPEC,
            "",
            &body_lines,
            &mut diagnostics,
            &ctx,
        );
        let directive = parse_glossary(&invocation, &mut adornment_order, &mut diagnostics);

        // Then no entries are produced
        if let Directive::Glossary { entries, sorted } = directive {
            assert!(entries.is_empty());
            assert!(!sorted);
        } else {
            panic!("Expected Glossary directive");
        }
    }

    #[test]
    fn test_parse_glossary_does_not_panic_on_multi_byte_char_in_a_short_line() {
        // Given a body whose first line has a 3-space indent and a second,
        // less-indented line containing a multi-byte character at the byte
        // offset the old byte-index slicing would have panicked on
        let body_lines = vec!["   First line normal indent.", "  éfoo"];
        let mut adornment_order = Vec::new();
        let mut diagnostics = Diagnostics::default();

        // When parsing the glossary directive body
        let ctx = ParseCtx::with_domain(Domain::Py);
        let invocation = crate::directives::reader::test_invocation(
            "glossary",
            &SPEC,
            "",
            &body_lines,
            &mut diagnostics,
            &ctx,
        );
        let directive = parse_glossary(&invocation, &mut adornment_order, &mut diagnostics);

        // Then it does not panic
        assert!(matches!(directive, Directive::Glossary { .. }));
    }
}

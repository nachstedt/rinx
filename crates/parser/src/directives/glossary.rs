//! Glossary directive parsing for RST documents.

use crate::blocks::parse_blocks;
use crate::context::ParseCtx;
use crate::diagnostics::Diagnostics;
use crate::headings::Adornment;
use crate::indent::unindent_body_lines;
use rusty_sphinx_ast::Directive;

/// Parses a `.. glossary::` directive body into a `Directive::Glossary` node.
pub(super) fn parse_glossary(
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Diagnostics,
    ctx: &ParseCtx<'_>,
) -> Directive {
    // Strip the base indentation from all lines.
    let unindented = unindent_body_lines(body_lines);
    if unindented.is_empty() {
        return Directive::Glossary {
            entries: vec![],
            sorted: false,
        };
    }

    // Parse the :sorted: option from the leading option lines.
    let mut sorted = false;
    let mut body_start = 0;
    for (idx, line) in unindented.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            body_start = idx + 1;
            continue;
        }
        if trimmed == ":sorted:" {
            sorted = true;
            body_start = idx + 1;
            continue;
        }
        // First non-option, non-blank line: definition body starts here.
        body_start = idx;
        break;
    }

    // Parse definition-list entries from the remaining lines.
    // A line with NO leading whitespace (after base-indent stripping) is a term.
    // A line WITH leading whitespace is part of the definition.
    let mut entries: Vec<rusty_sphinx_ast::GlossaryEntry> = Vec::new();
    let mut current_terms: Vec<String> = Vec::new();
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
                let def_strs: Vec<&str> = definition_lines.iter().map(String::as_str).collect();
                let mut dummy_adorn = adornment_order.clone();
                let def_ctx = ctx.nested(body_start + definition_start, 0);
                let def_nodes = parse_blocks(&def_strs, &mut dummy_adorn, diagnostics, &def_ctx);
                entries.push(rusty_sphinx_ast::GlossaryEntry {
                    terms: std::mem::take(&mut current_terms),
                    definition: def_nodes,
                });
                definition_lines.clear();
                in_definition = false;
            }
            current_terms.push(line.trim().to_string());
        }
    }

    // Flush any remaining entry.
    if !current_terms.is_empty() {
        let def_strs: Vec<&str> = definition_lines.iter().map(String::as_str).collect();
        let def_ctx = ctx.nested(body_start + definition_start, 0);
        let def_nodes = parse_blocks(&def_strs, adornment_order, diagnostics, &def_ctx);
        entries.push(rusty_sphinx_ast::GlossaryEntry {
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
                .map(|s| s.to_lowercase())
                .unwrap_or_default();
            let b_key = b
                .terms
                .first()
                .map(|s| s.to_lowercase())
                .unwrap_or_default();
            a_key.cmp(&b_key)
        });
    }

    Directive::Glossary { entries, sorted }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusty_sphinx_ast::Domain;

    #[test]
    fn test_parse_glossary_parses_a_single_term_and_definition() {
        // Given a glossary body with one term and an indented definition
        let body_lines = vec!["   term", "      Definition text."];
        let mut adornment_order = Vec::new();
        let mut diagnostics = Diagnostics::default();

        // When parsing the glossary directive body
        let directive = parse_glossary(
            &body_lines,
            &mut adornment_order,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

        // Then a single entry with the parsed term is produced
        if let Directive::Glossary { entries, sorted } = directive {
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].terms, vec!["term".to_string()]);
            assert!(!sorted);
        } else {
            panic!("Expected Glossary directive");
        }
    }

    #[test]
    fn test_parse_glossary_parses_sorted_option() {
        // Given a glossary body starting with the :sorted: option
        let body_lines = vec!["   :sorted:", "", "   term", "      Definition text."];
        let mut adornment_order = Vec::new();
        let mut diagnostics = Diagnostics::default();

        // When parsing the glossary directive body
        let directive = parse_glossary(
            &body_lines,
            &mut adornment_order,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

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
        let directive = parse_glossary(
            &body_lines,
            &mut adornment_order,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

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
        let directive = parse_glossary(
            &body_lines,
            &mut adornment_order,
            &mut diagnostics,
            &ParseCtx::with_domain(Domain::Py),
        );

        // Then it does not panic
        assert!(matches!(directive, Directive::Glossary { .. }));
    }
}

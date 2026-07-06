//! Glossary directive parsing for RST documents.

use super::blocks::parse_blocks;
use super::headings::Adornment;
use crate::ast::{Directive, Domain};

/// Parses a `.. glossary::` directive body into a `Directive::Glossary` node.
pub(super) fn parse_glossary(
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
    default_domain: Domain,
) -> Directive {
    // Determine the base indentation of the body (first non-blank line).
    let Some(first_non_blank) = body_lines.iter().find(|l| !l.trim().is_empty()) else {
        return Directive::Glossary {
            entries: vec![],
            sorted: false,
        };
    };
    let base_indent = first_non_blank
        .chars()
        .take_while(|c| c.is_whitespace())
        .count();

    // Strip the base indentation from all lines.
    let unindented: Vec<String> = body_lines
        .iter()
        .map(|l| {
            if l.len() >= base_indent {
                l[base_indent..].to_string()
            } else {
                l.trim().to_string()
            }
        })
        .collect();

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
    let mut entries: Vec<crate::ast::GlossaryEntry> = Vec::new();
    let mut current_terms: Vec<String> = Vec::new();
    let mut definition_lines: Vec<String> = Vec::new();
    let mut in_definition = false;

    let body_slice = &unindented[body_start..];

    for line in body_slice {
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
            in_definition = true;
            definition_lines.push(line.clone());
        } else {
            // Non-indented: this is a term.
            if in_definition {
                // Flush the completed entry.
                let def_strs: Vec<&str> = definition_lines.iter().map(String::as_str).collect();
                let mut dummy_adorn = adornment_order.clone();
                let def_nodes =
                    parse_blocks(&def_strs, &mut dummy_adorn, diagnostics, default_domain);
                entries.push(crate::ast::GlossaryEntry {
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
        let def_nodes = parse_blocks(&def_strs, adornment_order, diagnostics, default_domain);
        entries.push(crate::ast::GlossaryEntry {
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

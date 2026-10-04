//! Reporting the definitions several documents claim.
//!
//! [`ProjectIndex::merge`] leaves such a definition out of the index rather
//! than let one claimant win, and records every claimant in
//! [`rinx_index::AmbiguousDefinitions`]. This phase turns that record into
//! warnings: one on *every* claiming document, each naming the others, so the
//! report reads the same whichever order the documents were read in — and an
//! author looking at any of the pages learns where the clash is.

use std::collections::{BTreeMap, BTreeSet};

use rinx_ast::{Diagnostic, DiagnosticCode, TargetName};
use rinx_index::ProjectIndex;

use super::DocumentDiagnostics;

/// One warning per document claiming each contested definition in `index`,
/// grouped by document.
pub(super) fn collect_duplicate_definition_diagnostics(
    index: &ProjectIndex,
) -> Vec<DocumentDiagnostics> {
    let contested = &index.ambiguous_definitions;
    let mut by_document: BTreeMap<String, Vec<Diagnostic>> = BTreeMap::new();
    let mut report =
        |claimants: &BTreeSet<String>, code: DiagnosticCode, message: &dyn Fn(&str) -> String| {
            for document in claimants {
                let others = claimants.iter().filter(|other| *other != document);
                by_document
                    .entry(document.clone())
                    .or_default()
                    .push(Diagnostic::at(code, message(&quote_all(others)), None));
            }
        };

    // An entity is a `:ref:` target too, so a contested id contests its
    // target as well; it is reported once, as the entity it is.
    let entity_targets: BTreeSet<TargetName> = contested
        .entities
        .keys()
        .map(|id| TargetName::new(id.as_str()))
        .collect();
    for (name, claimants) in &contested.targets {
        if entity_targets.contains(name) {
            continue;
        }
        report(claimants, DiagnosticCode::TargetDuplicateName, &|others| {
            format!(
                "Target '{}' is also defined in {others}, so it links to neither; rename one of them.",
                name.as_str()
            )
        });
    }
    for (term, claimants) in &contested.glossary_terms {
        report(
            claimants,
            DiagnosticCode::GlossaryDuplicateTerm,
            &|others| {
                format!(
                    "Glossary term '{}' is also defined in {others}, so :term: reaches neither; keep one definition.",
                    term.as_str()
                )
            },
        );
    }
    for (name, by_type) in &contested.domain_objects {
        for (object_type, claimants) in by_type {
            report(
                claimants,
                DiagnosticCode::ObjectDuplicateDescription,
                &|others| {
                    format!(
                        "{}:{} '{}' is also described in {others}, so neither description is a target; mark all but one with :no-index:.",
                        object_type.domain().as_str(),
                        object_type.as_str(),
                        index.domain_object_spelling(name)
                    )
                },
            );
        }
    }
    for (label, claimants) in &contested.equations {
        report(claimants, DiagnosticCode::MathDuplicateLabel, &|others| {
            format!(
                "Equation label '{}' is also used in {others}, so neither equation can be referenced.",
                label.as_str()
            )
        });
    }
    for (id, claimants) in &contested.entities {
        report(claimants, DiagnosticCode::EntityDuplicateId, &|others| {
            format!(
                "Entity id '{}' is also declared in {others}, so neither entity is indexed.",
                id.as_str()
            )
        });
    }

    by_document
        .into_iter()
        .map(|(source_path, diagnostics)| DocumentDiagnostics {
            source_path,
            diagnostics,
        })
        .collect()
}

/// `documents`, each quoted, joined into a phrase: `'a.rst'`,
/// `'a.rst' and 'b.rst'`, `'a.rst', 'b.rst' and 'c.rst'`.
fn quote_all<'a>(documents: impl Iterator<Item = &'a String>) -> String {
    let quoted: Vec<String> = documents.map(|document| format!("'{document}'")).collect();
    match quoted.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
        Some((last, _)) => last.clone(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::{EntityId, ObjectType, PyObjectType};

    fn claimants(documents: &[&str]) -> BTreeSet<String> {
        documents.iter().map(ToString::to_string).collect()
    }

    /// The `(document, code, message)` of every diagnostic, in order.
    fn flat(reported: &[DocumentDiagnostics]) -> Vec<(String, String, String)> {
        reported
            .iter()
            .flat_map(|group| {
                group.diagnostics.iter().map(|diagnostic| {
                    (
                        group.source_path.clone(),
                        diagnostic.code.as_str().to_string(),
                        diagnostic.message.clone(),
                    )
                })
            })
            .collect()
    }

    #[test]
    fn test_collect_reports_a_contested_label_on_every_claimant() {
        // Given
        let mut index = ProjectIndex::default();
        index
            .ambiguous_definitions
            .targets
            .insert(TargetName::new("setup"), claimants(&["a.rst", "b.rst"]));

        // When
        let reported = collect_duplicate_definition_diagnostics(&index);

        // Then — each page names the other, so neither is singled out.
        assert_eq!(
            flat(&reported),
            [
                (
                    "a.rst".to_string(),
                    "target.duplicate-name".to_string(),
                    "Target 'setup' is also defined in 'b.rst', so it links to neither; rename one of them.".to_string()
                ),
                (
                    "b.rst".to_string(),
                    "target.duplicate-name".to_string(),
                    "Target 'setup' is also defined in 'a.rst', so it links to neither; rename one of them.".to_string()
                ),
            ]
        );
    }

    #[test]
    fn test_collect_advises_no_index_for_a_contested_domain_object() {
        // Given
        let mut index = ProjectIndex::default();
        index
            .ambiguous_definitions
            .domain_objects
            .entry(TargetName::new("bytearray"))
            .or_default()
            .insert(
                ObjectType::Py(PyObjectType::Class),
                claimants(&["a.rst", "b.rst"]),
            );
        index
            .domain_object_spellings
            .insert(TargetName::new("bytearray"), "bytearray".to_string());

        // When
        let reported = flat(&collect_duplicate_definition_diagnostics(&index));

        // Then
        assert_eq!(reported.len(), 2);
        assert_eq!(reported[0].1, "object.duplicate-description");
        assert_eq!(
            reported[0].2,
            "py:class 'bytearray' is also described in 'b.rst', so neither description is a target; mark all but one with :no-index:."
        );
    }

    #[test]
    fn test_collect_reports_a_contested_entity_once_rather_than_also_as_a_target() {
        // Given — an entity id contests its `:ref:` target too.
        let mut index = ProjectIndex::default();
        let id = EntityId::new("REQ_1").expect("valid id");
        index
            .ambiguous_definitions
            .entities
            .insert(id, claimants(&["a.rst", "b.rst"]));
        index
            .ambiguous_definitions
            .targets
            .insert(TargetName::new("REQ_1"), claimants(&["a.rst", "b.rst"]));

        // When
        let reported = flat(&collect_duplicate_definition_diagnostics(&index));

        // Then
        let codes: BTreeSet<&str> = reported.iter().map(|(_, code, _)| code.as_str()).collect();
        assert_eq!(codes, BTreeSet::from(["entity.duplicate-id"]));
        assert_eq!(reported.len(), 2);
    }

    #[test]
    fn test_collect_reports_glossary_terms_and_equations_under_their_own_codes() {
        // Given
        let mut index = ProjectIndex::default();
        index
            .ambiguous_definitions
            .glossary_terms
            .insert(TargetName::new("bytecode"), claimants(&["a.rst", "b.rst"]));
        index
            .ambiguous_definitions
            .equations
            .insert(TargetName::new("euler"), claimants(&["a.rst", "b.rst"]));

        // When
        let reported = flat(&collect_duplicate_definition_diagnostics(&index));

        // Then
        let codes: BTreeSet<&str> = reported.iter().map(|(_, code, _)| code.as_str()).collect();
        assert_eq!(
            codes,
            BTreeSet::from(["glossary.duplicate-term", "math.duplicate-label"])
        );
    }

    #[test]
    fn test_quote_all_joins_one_two_and_three_documents() {
        // Given
        let one = claimants(&["a.rst"]);
        let two = claimants(&["a.rst", "b.rst"]);
        let three = claimants(&["a.rst", "b.rst", "c.rst"]);

        // When / Then
        assert_eq!(quote_all(one.iter()), "'a.rst'");
        assert_eq!(quote_all(two.iter()), "'a.rst' and 'b.rst'");
        assert_eq!(quote_all(three.iter()), "'a.rst', 'b.rst' and 'c.rst'");
    }
}

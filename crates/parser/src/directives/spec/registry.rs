//! The registry of every directive this build implements: one list pairing
//! each module's `SPEC` with its `interpret_*`.
//!
//! The dependencies run one way: each module builds its `SPEC` from
//! [`super::vocabulary`] alone and knows nothing of the registry, which is the
//! only place directives are collected. A spec stays pure data, so any reader
//! of a directive's syntax can share it; the pairing with behaviour lives
//! here, in the entry.

use crate::directives::domains::object_type::{
    object_directive_names, object_directive_specs, object_spec, resolve_domain_object_type,
};
use crate::directives::invocation::{DirectiveInvocation, ParseState};
use crate::directives::scope::{
    resolve_scope_directive, scope_directive_names, scope_directive_specs,
};
use crate::directives::{
    admonitions, button_link, code_block, contents, data_table, default_role, doctest, dropdown,
    entity_bar, entity_flow, entity_pie, entity_sequence, entity_table, entity_update, glossary,
    grid, if_builder, image, include, index_directive, math, needimport, needservice, role,
    sectnum, table, toctree, uml,
};
use rinx_ast::{Domain, Node};

use super::vocabulary::DirectiveSpec;

/// One directive this build implements: what it is, and how its invocation
/// is interpreted.
pub(crate) struct RegisteredDirective {
    /// Its names and purpose.
    pub(crate) spec: &'static DirectiveSpec<'static>,
    /// Turns its invocation into nodes.
    pub(crate) interpret: Interpret,
}

/// The signature every directive's `interpret_*` shares.
pub(crate) type Interpret = fn(&DirectiveInvocation<'_, '_>, &mut ParseState<'_>) -> Vec<Node>;

/// The [`RegisteredDirective`] pairing `spec` with `interpret`.
const fn entry(spec: &'static DirectiveSpec<'static>, interpret: Interpret) -> RegisteredDirective {
    RegisteredDirective { spec, interpret }
}

/// Every directive this build implements.
///
/// A module whose `SPEC` or `interpret_*` is missing here is left unused, which
/// the compiler reports as dead code.
const DIRECTIVES: &[RegisteredDirective] = &[
    // `.. note::` and the other eight titled by their kind.
    entry(
        &admonitions::ADMONITION_SPEC,
        admonitions::interpret_admonition,
    ),
    // `.. admonition::`, titled by its argument.
    entry(
        &admonitions::GENERIC_ADMONITION_SPEC,
        admonitions::interpret_generic_admonition,
    ),
    // `.. seealso::`.
    entry(&admonitions::SEEALSO_SPEC, admonitions::interpret_seealso),
    // `.. versionadded::` and its two siblings.
    entry(
        &admonitions::VERSION_CHANGE_SPEC,
        admonitions::interpret_version_change,
    ),
    // `.. include::`.
    entry(&include::SPEC, include::interpret_include),
    // `.. needimport::`.
    entry(&needimport::SPEC, needimport::interpret_needimport),
    // `.. role::`.
    entry(&role::SPEC, role::interpret_role),
    // `.. default-role::`.
    entry(&default_role::SPEC, default_role::interpret_default_role),
    // sphinx-simplepdf's `.. if-builder::`.
    entry(&if_builder::SPEC, if_builder::interpret_if_builder),
    // `.. toctree::`.
    entry(&toctree::SPEC, toctree::interpret_toctree),
    // `.. contents::`.
    entry(&contents::SPEC, contents::interpret_contents),
    // `.. sectnum::`.
    entry(&sectnum::SPEC, sectnum::interpret_sectnum),
    // `.. code-block::`.
    entry(
        &code_block::CODE_BLOCK_SPEC,
        code_block::interpret_code_block,
    ),
    // docutils' `.. code::`.
    entry(&code_block::CODE_SPEC, code_block::interpret_code_block),
    // `.. literalinclude::`.
    entry(
        &code_block::LITERAL_INCLUDE_SPEC,
        code_block::interpret_literal_include,
    ),
    // `.. highlight::`.
    entry(&code_block::HIGHLIGHT_SPEC, code_block::interpret_highlight),
    // `.. glossary::`.
    entry(&glossary::SPEC, glossary::interpret_glossary),
    // `.. list-table::`.
    entry(
        &data_table::LIST_TABLE_SPEC,
        data_table::interpret_list_table,
    ),
    // `.. csv-table::`.
    entry(&data_table::CSV_TABLE_SPEC, data_table::interpret_csv_table),
    // `.. image::`.
    entry(&image::IMAGE_SPEC, image::interpret_image),
    // `.. figure::`.
    entry(&image::FIGURE_SPEC, image::interpret_figure),
    // `.. math::`.
    entry(&math::SPEC, math::interpret_math),
    // `.. table::`.
    entry(&table::SPEC, table::interpret_table),
    // `.. index::`.
    entry(
        &index_directive::INDEX_SPEC,
        index_directive::interpret_index,
    ),
    // `.. doctest::` and the other four of `sphinx.ext.doctest`.
    entry(&doctest::DOCTEST_SPEC, doctest::interpret_doctest),
    entry(&doctest::TESTCODE_SPEC, doctest::interpret_doctest),
    entry(&doctest::TEST_SETUP_SPEC, doctest::interpret_doctest),
    // sphinx-design's `.. dropdown::`.
    entry(&dropdown::SPEC, dropdown::interpret_dropdown),
    // sphinx-design's `.. grid::`.
    entry(&grid::GRID_SPEC, grid::interpret_grid),
    // sphinx-design's `.. grid-item::`.
    entry(&grid::GRID_ITEM_SPEC, grid::interpret_grid_item),
    // sphinx-design's `.. button-link::`.
    entry(&button_link::SPEC, button_link::interpret_button_link),
    // `.. entity-table::`, sphinx-needs' `.. needtable::`.
    entry(&entity_table::SPEC, entity_table::interpret_entity_table),
    // `.. entity-flow::`, sphinx-needs' `.. needflow::`.
    entry(&entity_flow::SPEC, entity_flow::interpret_entity_flow),
    // `.. entity-sequence::`, sphinx-needs' `.. needsequence::`.
    entry(
        &entity_sequence::SPEC,
        entity_sequence::interpret_entity_sequence,
    ),
    // `.. entity-pie::`, sphinx-needs' `.. needpie::`.
    entry(&entity_pie::SPEC, entity_pie::interpret_entity_pie),
    // `.. entity-bar::`, sphinx-needs' `.. needbar::`.
    entry(&entity_bar::SPEC, entity_bar::interpret_entity_bar),
    // `.. entity-update::`, sphinx-needs' `.. needextend::`.
    entry(&entity_update::SPEC, entity_update::interpret_entity_update),
    // sphinx-needs' `.. needservice::`, refused by name.
    entry(&needservice::SPEC, needservice::interpret_needservice),
    // The six spellings of a `PlantUML` diagram.
    entry(&uml::SPEC, uml::interpret_uml),
    // This build's two templated diagram spellings and sphinx-needs' two.
    entry(&uml::TEMPLATED_SPEC, uml::interpret_uml),
];

/// Every directive this build implements whose names are fixed, in the
/// registry's order.
pub fn directive_specs() -> impl Iterator<Item = &'static DirectiveSpec<'static>> {
    DIRECTIVES.iter().map(|directive| directive.spec)
}

/// Every directive whose name is resolved against the default domain — the
/// domain objects, then the scope directives — with the domain its names are
/// qualified in: `None` for a `std` one, written bare whatever the default
/// domain (`.. option::`), else written bare in that domain and qualified in
/// any other (`.. function::`, `.. py:function::`).
pub fn domain_directive_specs()
-> impl Iterator<Item = (Option<Domain>, &'static DirectiveSpec<'static>)> {
    object_directive_specs().chain(scope_directive_specs())
}

/// The directive written as `name`, or `None` when no entry of the table
/// spells it — a domain object, a scope directive, an entity's or an unknown
/// name.
pub(crate) fn registered_directive(name: &str) -> Option<&'static RegisteredDirective> {
    DIRECTIVES
        .iter()
        .find(|directive| directive.spec.names.contains(&name))
}

/// The spec of the directive written as `name` where `default_domain` is the
/// default domain, resolved as the dispatcher resolves it: a domain object or
/// scope directive first, then a directive of the table — `None` for an
/// entity type's name, which the schema's [`EntitySpec`] answers, and for an
/// unknown one.
///
/// [`EntitySpec`]: crate::directives::entity_spec::EntitySpec
#[must_use]
pub fn directive_spec(
    name: &str,
    default_domain: Domain,
) -> Option<&'static DirectiveSpec<'static>> {
    if let Some(object_type) = resolve_domain_object_type(name, default_domain) {
        return Some(object_spec(object_type));
    }
    if let Some(scope) = resolve_scope_directive(name, default_domain) {
        return Some(scope.spec());
    }
    registered_directive(name).map(|directive| directive.spec)
}

/// Reports whether `name` is a directive this build already understands.
///
/// Used by the entity schema loader to refuse a section that would shadow one.
/// Matches the bare name only: a domain-qualified spelling like `py:class`
/// cannot collide with a section name, which never carries a colon. Domain
/// object and scope directive names are included: they are resolved against a
/// default domain rather than by the table, but they are just as much names a
/// section must not take.
#[must_use]
pub fn is_builtin_directive_name(name: &str) -> bool {
    registered_directive(name).is_some()
        || object_directive_names().any(|object| object == name)
        || scope_directive_names().any(|scope| scope == name)
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::super::vocabulary::OptionSpec;
    use super::*;
    use rinx_ast::{
        AdmonitionKind, CodeBlockSource, EntityBarSource, EntityFlowSource, EntityPieSource,
        EntitySequenceSource, EntityTableSource, EntityUpdateSource, UmlSource, VersionChangeKind,
    };

    /// Asserts `spec` spells exactly `spellings`, each of which `E` parses —
    /// the spelling enum a directive's parser reads its name back through.
    fn assert_spelled_as<E: FromStr>(spec: &DirectiveSpec, spellings: &[&str]) {
        assert_eq!(spec.names, spellings, "{spec:?} is not spelled as expected");
        for name in spellings {
            assert!(
                name.parse::<E>().is_ok(),
                "`{name}` does not parse as the spelling its parser reads"
            );
        }
    }

    /// Every spec of a directive this build implements, fixed names and
    /// domain-resolved alike.
    fn every_spec() -> impl Iterator<Item = &'static DirectiveSpec<'static>> {
        directive_specs().chain(domain_directive_specs().map(|(_, spec)| spec))
    }

    #[test]
    fn test_no_name_is_claimed_twice() {
        // Given
        let mut seen: Vec<&str> = Vec::new();

        // When / Then
        for spec in directive_specs() {
            for name in spec.names {
                assert!(!seen.contains(name), "`{name}` is claimed twice");
                seen.push(name);
            }
        }
    }

    #[test]
    fn test_every_spec_names_itself_and_says_what_it_is_for() {
        // Given / When / Then
        for spec in every_spec() {
            assert!(!spec.names.is_empty(), "{spec:?} has no name");
            assert!(!spec.doc.is_empty(), "{spec:?} has no doc");
        }
    }

    #[test]
    fn test_only_the_schema_and_service_directives_accept_options_they_do_not_list() {
        // Given / When
        let open: Vec<&str> = every_spec()
            .filter(|spec| spec.accepts_further)
            .map(|spec| spec.names[0])
            .collect();

        // Then
        assert_eq!(open, ["entity-update", "needservice"]);
    }

    #[test]
    fn test_every_option_spelling_names_itself_and_says_what_it_does() {
        // Given / When / Then
        for spec in every_spec() {
            for option in spec.option_specs() {
                assert!(!option.name.is_empty(), "{option:?} has no name");
                assert!(!option.doc.is_empty(), "{option:?} has no doc");
            }
        }
    }

    #[test]
    fn test_no_directive_declares_an_option_name_twice() {
        // Given / When / Then
        for spec in every_spec() {
            let mut seen: Vec<&str> = Vec::new();
            for name in spec.option_specs().flat_map(OptionSpec::names) {
                assert!(
                    !seen.contains(&name),
                    "`{}` declares `:{name}:` twice",
                    spec.names[0]
                );
                seen.push(name);
            }
        }
    }

    #[test]
    fn test_the_admonitions_together_spell_every_admonition_kind() {
        // Given — one per `AdmonitionKind`, via its own `as_str`
        let every_kind = [
            AdmonitionKind::Attention,
            AdmonitionKind::Caution,
            AdmonitionKind::Danger,
            AdmonitionKind::Error,
            AdmonitionKind::Hint,
            AdmonitionKind::Important,
            AdmonitionKind::Note,
            AdmonitionKind::Tip,
            AdmonitionKind::Warning,
        ]
        .map(|kind| kind.as_str());

        // When / Then
        assert_spelled_as::<AdmonitionKind>(&admonitions::ADMONITION_SPEC, &every_kind);
        assert_spelled_as::<AdmonitionKind>(
            &admonitions::GENERIC_ADMONITION_SPEC,
            &[AdmonitionKind::Admonition.as_str()],
        );
    }

    #[test]
    fn test_the_version_change_spells_every_version_change_kind() {
        // Given / When / Then
        assert_spelled_as::<VersionChangeKind>(
            &admonitions::VERSION_CHANGE_SPEC,
            &[
                VersionChangeKind::Added.as_str(),
                VersionChangeKind::Changed.as_str(),
                VersionChangeKind::Deprecated.as_str(),
            ],
        );
    }

    #[test]
    fn test_the_doctest_family_spells_every_doctest_directive() {
        // Given
        let specs = [
            &doctest::DOCTEST_SPEC,
            &doctest::TESTCODE_SPEC,
            &doctest::TEST_SETUP_SPEC,
        ];
        let names: Vec<&str> = specs
            .iter()
            .flat_map(|spec| spec.names.iter().copied())
            .collect();

        // When / Then
        assert_eq!(
            names,
            [
                "doctest",
                "testoutput",
                "testcode",
                "testsetup",
                "testcleanup"
            ]
        );
        for name in names {
            let document =
                crate::parse_document("index.rst", &format!(".. {name}::\n\n   >>> 1\n"));
            assert!(
                matches!(
                    document.nodes[..],
                    [rinx_ast::Node::Directive {
                        directive: rinx_ast::Directive::DocTest(_),
                        ..
                    }]
                ),
                "`{name}` is no doctest directive"
            );
        }
    }

    #[test]
    fn test_the_code_blocks_spell_both_of_their_sources() {
        // Given / When / Then
        assert_eq!(
            code_block::CODE_BLOCK_SPEC.names,
            [CodeBlockSource::CodeBlock.directive_name()]
        );
        assert_eq!(
            code_block::CODE_SPEC.names,
            [CodeBlockSource::Code.directive_name()]
        );
    }

    #[test]
    fn test_the_entity_views_spell_both_of_their_sources() {
        // Given / When / Then — this build's spelling first, sphinx-needs' second
        assert_spelled_as::<EntityTableSource>(
            &entity_table::SPEC,
            &[
                EntityTableSource::EntityTable.as_str(),
                EntityTableSource::NeedTable.as_str(),
            ],
        );
        assert_spelled_as::<EntityFlowSource>(
            &entity_flow::SPEC,
            &[
                EntityFlowSource::EntityFlow.as_str(),
                EntityFlowSource::NeedFlow.as_str(),
            ],
        );
        assert_spelled_as::<EntitySequenceSource>(
            &entity_sequence::SPEC,
            &[
                EntitySequenceSource::EntitySequence.as_str(),
                EntitySequenceSource::NeedSequence.as_str(),
            ],
        );
        assert_spelled_as::<EntityPieSource>(
            &entity_pie::SPEC,
            &[
                EntityPieSource::EntityPie.as_str(),
                EntityPieSource::NeedPie.as_str(),
            ],
        );
        assert_spelled_as::<EntityBarSource>(
            &entity_bar::SPEC,
            &[
                EntityBarSource::EntityBar.as_str(),
                EntityBarSource::NeedBar.as_str(),
            ],
        );
        assert_spelled_as::<EntityUpdateSource>(
            &entity_update::SPEC,
            &[
                EntityUpdateSource::EntityUpdate.as_str(),
                EntityUpdateSource::NeedExtend.as_str(),
            ],
        );
    }

    #[test]
    fn test_the_diagrams_spell_every_uml_source_by_whether_it_is_templated() {
        // Given
        let spelled = |templated: bool| -> Vec<&str> {
            UmlSource::ALL
                .iter()
                .filter(|source| source.is_templated() == templated)
                .map(|source| source.as_str())
                .collect()
        };

        // When / Then
        assert_spelled_as::<UmlSource>(&uml::SPEC, &spelled(false));
        assert_spelled_as::<UmlSource>(&uml::TEMPLATED_SPEC, &spelled(true));
    }

    #[test]
    fn test_a_spelled_name_finds_its_directive() {
        // Given / When
        let directive = registered_directive("needtable");

        // Then
        assert_eq!(
            directive.map(|directive| directive.spec.names),
            Some(entity_table::SPEC.names)
        );
    }

    #[test]
    fn test_a_domain_object_name_finds_no_directive() {
        // Given / When / Then — resolved against the default domain instead
        assert!(registered_directive("function").is_none());
        assert!(registered_directive("py:function").is_none());
        assert!(registered_directive("currentmodule").is_none());
    }

    #[test]
    fn test_a_name_no_directive_uses_is_not_reserved() {
        // Given / When / Then
        assert!(!is_builtin_directive_name("verification-criteria"));
        assert!(!is_builtin_directive_name("req"));
        assert!(!is_builtin_directive_name("safety-comment"));
    }

    #[test]
    fn test_a_representative_directive_name_from_each_group_is_reserved() {
        // Given / When / Then
        assert!(is_builtin_directive_name("note"));
        assert!(is_builtin_directive_name("toctree"));
        assert!(is_builtin_directive_name("csv-table"));
        assert!(is_builtin_directive_name("code-block"));
        assert!(is_builtin_directive_name("image"));
        assert!(is_builtin_directive_name("include"));
        assert!(is_builtin_directive_name("dropdown"));
    }

    #[test]
    fn test_every_domain_object_and_scope_directive_name_is_reserved() {
        // Given / When / Then
        for name in [
            "function",
            "decoratormethod",
            "class",
            "macro",
            "var",
            "option",
            "cmdoption",
            "currentmodule",
            "program",
            "namespace",
            "namespace-push",
            "namespace-pop",
        ] {
            assert!(is_builtin_directive_name(name), "`{name}` is not reserved");
        }
    }

    #[test]
    fn test_domain_directive_specs_list_the_objects_then_the_scope_directives() {
        // Given / When
        let specs: Vec<(Option<Domain>, &str)> = domain_directive_specs()
            .map(|(domain, spec)| (domain, spec.names[0]))
            .collect();

        // Then
        assert_eq!(specs.first(), Some(&(None, "option")));
        assert!(specs.contains(&(Some(Domain::Py), "function")));
        assert!(specs.contains(&(Some(Domain::C), "function")));
        assert_eq!(specs.last(), Some(&(Some(Domain::C), "namespace-pop")));
    }

    #[test]
    fn test_a_domain_object_takes_options_and_a_scope_directive_none() {
        // Given
        let spec_of = |wanted: &str| {
            domain_directive_specs()
                .find(|(_, spec)| spec.names[0] == wanted)
                .map(|(_, spec)| spec)
                .expect("a domain directive")
        };

        // When / Then
        assert!(
            spec_of("method")
                .option_specs()
                .any(|option| option.name == "async")
        );
        assert!(spec_of("currentmodule").option_specs().next().is_none());
        assert!(!spec_of("currentmodule").takes_options());
    }

    #[test]
    fn test_directive_spec_resolves_a_bare_object_name_against_the_default_domain() {
        // Given / When
        let in_py = directive_spec("function", Domain::Py).map(|spec| spec.doc);
        let in_c = directive_spec("function", Domain::C).map(|spec| spec.doc);

        // Then
        assert_eq!(in_py, Some("Describes a Python function."));
        assert_eq!(in_c, Some("Describes a C function."));
    }

    #[test]
    fn test_directive_spec_resolves_qualified_scope_and_table_names() {
        // Given / When / Then
        assert_eq!(
            directive_spec("c:member", Domain::Py).map(|spec| spec.names),
            Some(&["member", "var"][..])
        );
        assert_eq!(
            directive_spec("currentmodule", Domain::Py).map(|spec| spec.names),
            Some(&["currentmodule"][..])
        );
        assert_eq!(
            directive_spec("needtable", Domain::C).map(|spec| spec.names),
            Some(entity_table::SPEC.names)
        );
    }

    #[test]
    fn test_directive_spec_answers_nothing_for_an_unknown_or_entity_name() {
        // Given / When / Then
        assert!(directive_spec("req", Domain::Py).is_none());
        assert!(directive_spec("classmethod", Domain::C).is_none());
        assert!(directive_spec("cpp:function", Domain::Py).is_none());
    }

    #[test]
    fn test_a_domain_qualified_name_is_not_reserved() {
        // Given / When / Then — a section name never carries a colon
        assert!(!is_builtin_directive_name("py:class"));
    }

    #[test]
    fn test_a_directive_refused_by_name_is_still_reserved() {
        // Given / When / Then — a section named `needservice` would otherwise
        // turn a refused construct into a silently accepted one
        assert!(is_builtin_directive_name("needservice"));
    }
}

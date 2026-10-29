//! Tests for what a rename rewrites and refuses.

use super::*;

fn label(name: &str) -> Renamed {
    Renamed::of(Some(&DefinitionKey::Label(TargetName::new(name)))).expect("a label")
}

fn entity(id: &str) -> Renamed {
    Renamed::of(Some(&DefinitionKey::Entity(EntityId::new(id).unwrap()))).expect("an entity")
}

fn at(line: u32, column: u32) -> Position {
    Position::new(line, column)
}

/// What `use_name_place` finds written in `text` for a use spanning the
/// whole of its first line.
fn name_in(form: &UseForm, text: &str) -> Option<String> {
    let width = u32::try_from(text.lines().next()?.chars().count()).ok()?;
    use_name_place(form, text, at(1, 1), at(1, width + 1)).map(|place| place.written)
}

#[test]
fn test_of_admits_labels_equations_and_entities_only() {
    // Given
    let name = TargetName::new("x");

    // When / Then
    assert!(Renamed::of(Some(&DefinitionKey::Label(name.clone()))).is_ok());
    assert!(Renamed::of(Some(&DefinitionKey::Equation(name.clone()))).is_ok());
    assert_eq!(Renamed::of(None), Err(RenameRefusal::Document));
    assert_eq!(
        Renamed::of(Some(&DefinitionKey::Term(name))),
        Err(RenameRefusal::Term)
    );
}

#[test]
fn test_is_named_by_normalizes_a_label_and_reads_an_entity_as_its_label_too() {
    // When / Then
    assert!(label("getting started").is_named_by("Getting  Started"));
    assert!(!label("install").is_named_by("setup"));
    assert!(entity("REQ_1").is_named_by("REQ_1"));
    assert!(entity("REQ_1").is_named_by("req_1"));
    assert!(!entity("REQ_1").is_named_by("REQ_2"));
}

#[test]
fn test_check_new_name_trims_and_keys_the_new_name() {
    // When
    let checked = label("install").check_new_name("  Setup Guide ");

    // Then
    assert_eq!(
        checked,
        Ok((
            "Setup Guide".to_string(),
            Renamed::Label(TargetName::new("setup guide"))
        ))
    );
}

#[test]
fn test_check_new_name_refuses_what_cannot_stand_where_the_name_does() {
    // When / Then
    for refused in ["", "a`b", "a <b>", "py:mod", "two\nlines"] {
        assert!(
            matches!(
                label("install").check_new_name(refused),
                Err(RenameRefusal::InvalidName { .. })
            ),
            "{refused:?}"
        );
    }
    assert!(matches!(
        entity("REQ_1").check_new_name("not an id"),
        Err(RenameRefusal::InvalidName { .. })
    ));
}

#[test]
fn test_use_name_place_finds_a_role_s_target() {
    // When / Then — bare, titled, marked, and across a line break
    assert_eq!(
        name_in(&UseForm::Role, ":ref:`install`"),
        Some("install".to_string())
    );
    assert_eq!(
        name_in(&UseForm::Role, ":ref:`Mise en place < install >`"),
        Some("install".to_string())
    );
    assert_eq!(
        name_in(&UseForm::Role, ":numref:`!fig-a`"),
        Some("fig-a".to_string())
    );
    let wrapped = "See :ref:`the\n   guide <install>` now.\n";
    let place = use_name_place(&UseForm::Role, wrapped, at(1, 5), at(2, 20)).expect("a place");
    assert_eq!(
        (place.start, place.end, place.written.as_str()),
        (at(2, 11), at(2, 18), "install")
    );
}

#[test]
fn test_use_name_place_finds_a_named_reference_s_name() {
    // Given
    let named = UseForm::Named(TargetName::new("install"));

    // When / Then
    assert_eq!(name_in(&named, "install_"), Some("install".to_string()));
    assert_eq!(name_in(&named, "`Install`_"), Some("Install".to_string()));
    assert_eq!(
        name_in(&named, "`the guide <install_>`_"),
        Some("install".to_string())
    );
    assert_eq!(
        name_in(&named, ".. _docs: install_"),
        Some("install".to_string())
    );
    assert_eq!(
        name_in(&named, ".. _docs: `the guide`_"),
        Some("the guide".to_string())
    );
    assert_eq!(name_in(&named, "`site <https://example.org>`_"), None);
}

#[test]
fn test_a_bare_reference_gains_backquotes_for_a_name_that_needs_them() {
    // Given
    let named = UseForm::Named(TargetName::new("install"));
    let bare = use_name_place(&named, "install_", at(1, 1), at(1, 9)).expect("a place");
    let quoted = use_name_place(&named, "`install`_", at(1, 1), at(1, 11)).expect("a place");

    // When / Then
    assert_eq!(bare.replacement("setup-guide"), "setup-guide");
    assert_eq!(bare.replacement("setup guide"), "`setup guide`");
    assert_eq!(quoted.replacement("setup guide"), "setup guide");
}

#[test]
fn test_use_name_place_finds_an_id_on_a_relation_line_and_an_exact_name() {
    // Given
    let text = ".. req:: Two\n   :links: REQ_0, REQ_1\n";
    let id = EntityId::new("REQ_1").unwrap();

    // When
    let relation = use_name_place(&UseForm::RelationId(id), text, at(2, 4), at(2, 24));
    let exact = use_name_place(&UseForm::Name, "REQ_1", at(1, 1), at(1, 6));

    // Then
    assert_eq!(
        relation.map(|place| (place.start, place.end)),
        Some((at(2, 19), at(2, 24)))
    );
    assert_eq!(exact.map(|place| place.written), Some("REQ_1".to_string()));
}

#[test]
fn test_use_name_place_is_none_for_a_toctree_entry() {
    // When / Then
    assert_eq!(name_in(&UseForm::TocEntry, "setup"), None);
}

#[test]
fn test_definition_name_place_is_the_range_the_parser_recorded() {
    // When
    let place = definition_name_place(
        "Intro\n\n.. _install:\n",
        NameRange {
            start: at(3, 5),
            end: at(3, 12),
        },
    );

    // Then
    assert_eq!(
        place.map(|place| place.written),
        Some("install".to_string())
    );
}

#[test]
fn test_is_simple_reference_name_follows_docutils() {
    // When / Then
    for simple in ["install", "setup-guide", "a.b", "py:mod", "c+1"] {
        assert!(is_simple_reference_name(simple), "{simple}");
    }
    for quoted in ["", "two words", "-lead", "trail-", "a--b", "a_", "c++1"] {
        assert!(!is_simple_reference_name(quoted), "{quoted}");
    }
}

#[test]
fn test_refusals_say_what_to_do() {
    // When / Then
    assert_eq!(
        RenameRefusal::Document.to_string(),
        "a document is renamed by renaming its file"
    );
    assert_eq!(
        RenameRefusal::Stale {
            path: PathBuf::from("/w/a.rst"),
            line: 3,
            name: "install".to_string(),
        }
        .to_string(),
        "line 3 of /w/a.rst no longer reads as 'install'; save it and try again"
    );
}

//! What a role's target may be completed with: every target of the kind the
//! role names, from the project index and the other sites' inventories, each
//! with the spellings it might be written in.
//!
//! Nothing here decides whether a spelling resolves, nor which one to write:
//! that is the resolver's to say, asked by [`super::select`]. A candidate
//! only lists the spellings worth asking about — the shortest first, its
//! full spelling last.

use lsp_types::CompletionItemKind;
use rinx_ast::{
    CObjectType, InlineNode, InventorySelector, ObjectType, PyObjectType, StdObjectType,
};
use rinx_entity::EntitySchema;
use rinx_index::{ExternalInventory, ProjectIndex};
use rinx_renderer::{InventoryEntryTypes, inventory_entry_types};

use crate::completion::doc_name::{absolute_doc_name, relative_doc_name};

/// One target a role's text may be completed with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Candidate {
    /// The spellings it might be written in, shortest first and in full last.
    pub forms: Vec<String>,
    /// How the editor marks it.
    pub kind: CompletionItemKind,
    /// Whether it is another site's: offered after this site's own.
    pub external: bool,
}

/// What the document being completed in asks of the index: its path, to
/// name documents relative to it, and whether the author began a document
/// name with `/`, to name them from the source root.
pub(super) struct Asking<'a> {
    pub doc_path: &'a str,
    pub absolute: bool,
}

/// Every candidate for the target of `reference` — a node standing for the
/// role being typed — under `schema`, from `index` and its inventories.
pub(super) fn candidates(
    reference: &InlineNode,
    index: &ProjectIndex,
    schema: &EntitySchema,
    asking: &Asking<'_>,
) -> Vec<Candidate> {
    let mut found = Vec::new();
    let selector = selector(reference);
    if selector.allows_local() {
        local_candidates(reference, index, schema, asking, &mut found);
    }
    if let Some(entry_types) = inventory_entry_types(reference) {
        for inventory in &index.external_inventories {
            if selector_admits(selector, inventory) {
                inventory_candidates(inventory, &entry_types, selector, &mut found);
            }
        }
    }
    found
}

/// The candidates this site defines for `reference`'s kind of target.
fn local_candidates(
    reference: &InlineNode,
    index: &ProjectIndex,
    schema: &EntitySchema,
    asking: &Asking<'_>,
    found: &mut Vec<Candidate>,
) {
    match reference {
        InlineNode::Reference { .. } => labels(index, found),
        InlineNode::DocReference { .. } => documents(index, asking, found),
        InlineNode::TermReference { .. } => terms(index, found),
        InlineNode::NumberReference { .. } => {
            found.extend(
                index
                    .numref_targets
                    .keys()
                    .map(|label| single(label.as_str(), CompletionItemKind::REFERENCE)),
            );
        }
        InlineNode::EquationReference { .. } => equations(index, found),
        InlineNode::EntityReference { role, .. } => {
            let Some(spec) = schema.role(role) else {
                return;
            };
            found.extend(
                index
                    .entities
                    .iter()
                    .filter(|(_, record)| spec.accepts(&record.type_name))
                    .map(|(id, _)| single(id.as_str(), CompletionItemKind::REFERENCE)),
            );
        }
        InlineNode::OptionReference { .. } => {
            domain_objects(index, |object_type| object_type == CMDOPTION, found);
        }
        InlineNode::DomainObjectReference { object_type, .. } => {
            let accepted = object_type.role_alias_candidates();
            domain_objects(index, |defined| accepted.contains(&defined), found);
        }
        InlineNode::AnyReference { .. } => {
            labels(index, found);
            documents(index, asking, found);
            terms(index, found);
            equations(index, found);
            domain_objects(index, |_| true, found);
        }
        _ => {}
    }
}

const CMDOPTION: ObjectType = ObjectType::Std(StdObjectType::Cmdoption);

/// Every label, as a `:ref:` names it.
fn labels(index: &ProjectIndex, found: &mut Vec<Candidate>) {
    found.extend(
        index
            .targets
            .keys()
            .map(|label| single(label.as_str(), CompletionItemKind::REFERENCE)),
    );
}

/// Every document, named as the asking document reads it.
fn documents(index: &ProjectIndex, asking: &Asking<'_>, found: &mut Vec<Candidate>) {
    let names: std::collections::BTreeSet<&String> = index
        .documents
        .iter()
        .chain(index.document_titles.keys())
        .collect();
    found.extend(names.into_iter().map(|target| {
        let name = if asking.absolute {
            absolute_doc_name(target)
        } else {
            relative_doc_name(asking.doc_path, target)
        };
        single(&name, CompletionItemKind::FILE)
    }));
}

/// Every glossary term.
fn terms(index: &ProjectIndex, found: &mut Vec<Candidate>) {
    found.extend(
        index
            .glossary_terms
            .keys()
            .map(|term| single(term.as_str(), CompletionItemKind::TEXT)),
    );
}

/// Every labelled equation.
fn equations(index: &ProjectIndex, found: &mut Vec<Candidate>) {
    found.extend(
        index
            .equations
            .keys()
            .map(|label| single(label.as_str(), CompletionItemKind::REFERENCE)),
    );
}

/// Every domain object one of whose types `accepts`, spelled as its
/// definition spelled it.
fn domain_objects(
    index: &ProjectIndex,
    accepts: impl Fn(ObjectType) -> bool,
    found: &mut Vec<Candidate>,
) {
    for (name, types) in &index.domain_objects {
        let Some(&object_type) = types.keys().find(|&&defined| accepts(defined)) else {
            continue;
        };
        let spelled = index
            .domain_object_spellings
            .get(name)
            .map_or(name.as_str(), String::as_str);
        found.push(Candidate {
            forms: object_forms(object_type, spelled),
            kind: object_kind(object_type),
            external: false,
        });
    }
}

/// The candidates `inventory` lists among `entry_types`. A target is spelled
/// as written, then — unless the role names this inventory itself — behind
/// the inventory's name, which reaches it past an earlier inventory listing
/// the same name.
fn inventory_candidates(
    inventory: &ExternalInventory,
    entry_types: &InventoryEntryTypes,
    selector: &InventorySelector,
    found: &mut Vec<Candidate>,
) {
    for (entry_type, targets) in &inventory.targets {
        let listed = match entry_types {
            InventoryEntryTypes::Every => true,
            InventoryEntryTypes::Listed(types) => types.iter().any(|t| t == entry_type.as_str()),
        };
        if !listed {
            continue;
        }
        let kind = entry_kind(entry_type.as_str());
        for target in targets.values() {
            let mut forms = written_forms(entry_type.as_str(), &target.name);
            if !matches!(selector, InventorySelector::Named(_)) {
                forms.push(format!("{}:{}", inventory.name.as_str(), target.name));
            }
            found.push(Candidate {
                forms,
                kind,
                external: true,
            });
        }
    }
}

/// The spellings a target of `entry_type` named `name` might be written in.
fn written_forms(entry_type: &str, name: &str) -> Vec<String> {
    match entry_type {
        "std:cmdoption" => option_forms(name),
        "std:label" | "std:doc" | "std:term" => vec![name.to_string()],
        _ => dotted_suffixes(name),
    }
}

/// The spellings an object of `object_type` spelled `spelled` might be
/// written in.
fn object_forms(object_type: ObjectType, spelled: &str) -> Vec<String> {
    if object_type == CMDOPTION {
        option_forms(spelled)
    } else {
        dotted_suffixes(spelled)
    }
}

/// `name` and each of its dotted suffixes, shortest first: `run`,
/// `Runner.run`, `pkg.Runner.run`.
fn dotted_suffixes(name: &str) -> Vec<String> {
    let mut forms: Vec<String> = name
        .match_indices('.')
        .map(|(at, _)| name[at + 1..].to_string())
        .filter(|suffix| !suffix.is_empty())
        .rev()
        .collect();
    forms.push(name.to_string());
    forms
}

/// The spellings of an option indexed as `program.option`: the option alone,
/// for where the program is the ambient one, then the two words an
/// `:option:` names another program's option with — for each `.` the name
/// could split at, since a program's name may hold one too.
fn option_forms(qualified: &str) -> Vec<String> {
    let mut bare = Vec::new();
    let mut with_program = Vec::new();
    for (at, _) in qualified.match_indices('.') {
        let (program, option) = (&qualified[..at], &qualified[at + 1..]);
        if !program.is_empty() && !option.is_empty() {
            bare.push(option.to_string());
            with_program.push(format!("{program} {option}"));
        }
    }
    bare.reverse();
    bare.extend(with_program);
    bare.push(qualified.to_string());
    bare
}

/// A candidate with one spelling.
fn single(name: &str, kind: CompletionItemKind) -> Candidate {
    Candidate {
        forms: vec![name.to_string()],
        kind,
        external: false,
    }
}

/// Where `reference` may look its target up: this site, the inventories, or
/// one of them.
fn selector(reference: &InlineNode) -> &InventorySelector {
    match reference {
        InlineNode::Reference { inventory, .. }
        | InlineNode::DocReference { inventory, .. }
        | InlineNode::TermReference { inventory, .. }
        | InlineNode::OptionReference { inventory, .. }
        | InlineNode::DomainObjectReference { inventory, .. }
        | InlineNode::AnyReference { inventory, .. } => inventory,
        _ => &InventorySelector::Any,
    }
}

/// Whether `selector` lets a reference reach into `inventory`.
fn selector_admits(selector: &InventorySelector, inventory: &ExternalInventory) -> bool {
    match selector {
        InventorySelector::Named(name) => &inventory.name == name,
        InventorySelector::Any | InventorySelector::ExternalOnly => true,
    }
}

/// How the editor marks an object of `object_type`.
fn object_kind(object_type: ObjectType) -> CompletionItemKind {
    match object_type {
        ObjectType::Py(PyObjectType::Function) | ObjectType::C(CObjectType::Function) => {
            CompletionItemKind::FUNCTION
        }
        ObjectType::Py(PyObjectType::Method) => CompletionItemKind::METHOD,
        ObjectType::Py(PyObjectType::Class | PyObjectType::Exception) => CompletionItemKind::CLASS,
        ObjectType::Py(PyObjectType::Module) => CompletionItemKind::MODULE,
        ObjectType::Py(PyObjectType::Data) => CompletionItemKind::VARIABLE,
        ObjectType::Py(PyObjectType::Attribute) | ObjectType::C(CObjectType::Member) => {
            CompletionItemKind::FIELD
        }
        ObjectType::C(CObjectType::Macro) => CompletionItemKind::CONSTANT,
        ObjectType::C(CObjectType::Struct | CObjectType::Union) => CompletionItemKind::STRUCT,
        ObjectType::C(CObjectType::Type) => CompletionItemKind::TYPE_PARAMETER,
        ObjectType::Std(StdObjectType::Cmdoption) => CompletionItemKind::PROPERTY,
    }
}

/// How the editor marks an inventory entry of `entry_type`.
fn entry_kind(entry_type: &str) -> CompletionItemKind {
    match entry_type {
        "std:doc" => CompletionItemKind::FILE,
        "std:term" => CompletionItemKind::TEXT,
        "std:cmdoption" => CompletionItemKind::PROPERTY,
        "py:function" | "c:function" => CompletionItemKind::FUNCTION,
        "py:method" => CompletionItemKind::METHOD,
        "py:class" | "py:exception" => CompletionItemKind::CLASS,
        "py:module" => CompletionItemKind::MODULE,
        "py:attribute" | "py:property" | "c:member" => CompletionItemKind::FIELD,
        _ => CompletionItemKind::REFERENCE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::{TargetName, TargetSearchOrder};
    use rinx_inventory::{EntryType, Inventory, InventoryEntry, InventoryName};

    fn asking() -> Asking<'static> {
        Asking {
            doc_path: "guide/notes.rst",
            absolute: false,
        }
    }

    fn object_reference(object_type: ObjectType, inventory: InventorySelector) -> InlineNode {
        InlineNode::DomainObjectReference {
            object_type,
            name: "x".to_string(),
            display: "x".to_string(),
            link: true,
            search_order: TargetSearchOrder::default(),
            span: None,
            inventory,
        }
    }

    fn label_reference() -> InlineNode {
        InlineNode::Reference {
            display: None,
            target: "x".to_string(),
            span: None,
            inventory: InventorySelector::Any,
        }
    }

    fn python_inventory() -> ExternalInventory {
        let entry = |name: &str, entry_type: &str| InventoryEntry {
            name: name.to_string(),
            entry_type: EntryType::new(entry_type).unwrap(),
            priority: 1,
            uri: format!("{name}.html"),
            display_name: None,
        };
        ExternalInventory::new(
            InventoryName::new("python").unwrap(),
            "https://docs.python.org/3/".to_string(),
            Inventory {
                project: "Python".to_string(),
                version: "3.12".to_string(),
                entries: vec![entry("dict", "py:class"), entry("intro", "std:label")],
            },
        )
    }

    fn forms(found: &[Candidate]) -> Vec<Vec<&str>> {
        found
            .iter()
            .map(|c| c.forms.iter().map(String::as_str).collect())
            .collect()
    }

    #[test]
    fn test_candidates_of_a_domain_role_are_the_objects_its_type_accepts() {
        // Given — a function, a method, and an exception `:class:` accepts
        let mut index = ProjectIndex::default();
        index.insert_domain_object(ObjectType::Py(PyObjectType::Function), "pkg.run", "a.rst");
        index.insert_domain_object(ObjectType::Py(PyObjectType::Exception), "pkg.Oops", "a.rst");
        let reference =
            object_reference(ObjectType::Py(PyObjectType::Class), InventorySelector::Any);

        // When
        let found = candidates(&reference, &index, &EntitySchema::empty(), &asking());

        // Then
        assert_eq!(forms(&found), [vec!["Oops", "pkg.Oops"]]);
        assert_eq!(found[0].kind, CompletionItemKind::CLASS);
    }

    #[test]
    fn test_candidates_add_an_inventory_s_entries_after_the_site_s_own() {
        // Given
        let mut index = ProjectIndex::default();
        index.insert_domain_object(ObjectType::Py(PyObjectType::Class), "pkg.Box", "a.rst");
        index.external_inventories.push(python_inventory());
        let reference =
            object_reference(ObjectType::Py(PyObjectType::Class), InventorySelector::Any);

        // When
        let found = candidates(&reference, &index, &EntitySchema::empty(), &asking());

        // Then
        assert_eq!(
            forms(&found),
            [vec!["Box", "pkg.Box"], vec!["dict", "python:dict"]]
        );
        assert!(found[1].external);
    }

    #[test]
    fn test_candidates_of_a_role_naming_an_inventory_are_its_entries_alone() {
        // Given
        let mut index = ProjectIndex::default();
        index.insert_domain_object(ObjectType::Py(PyObjectType::Class), "pkg.Box", "a.rst");
        index.external_inventories.push(python_inventory());
        let reference = object_reference(
            ObjectType::Py(PyObjectType::Class),
            InventorySelector::Named(InventoryName::new("python").unwrap()),
        );

        // When
        let found = candidates(&reference, &index, &EntitySchema::empty(), &asking());

        // Then
        assert_eq!(forms(&found), [vec!["dict"]]);
    }

    #[test]
    fn test_candidates_of_a_label_reference_are_labels_and_listed_labels() {
        // Given
        let mut index = ProjectIndex::default();
        index
            .targets
            .insert(TargetName::new("install"), "setup.rst".to_string());
        index.external_inventories.push(python_inventory());

        // When
        let found = candidates(
            &label_reference(),
            &index,
            &EntitySchema::empty(),
            &asking(),
        );

        // Then
        assert_eq!(
            forms(&found),
            [vec!["install"], vec!["intro", "python:intro"]]
        );
    }

    #[test]
    fn test_candidates_name_documents_as_the_asking_document_reads_them() {
        // Given
        let mut index = ProjectIndex::default();
        index.documents.insert("setup.rst".to_string());
        let reference = InlineNode::DocReference {
            display: None,
            target: "x".to_string(),
            link: true,
            span: None,
            inventory: InventorySelector::Any,
        };

        // When
        let relative = candidates(&reference, &index, &EntitySchema::empty(), &asking());
        let absolute = candidates(
            &reference,
            &index,
            &EntitySchema::empty(),
            &Asking {
                doc_path: "guide/notes.rst",
                absolute: true,
            },
        );

        // Then
        assert_eq!(forms(&relative), [vec!["../setup"]]);
        assert_eq!(forms(&absolute), [vec!["/setup"]]);
    }

    #[test]
    fn test_candidates_of_an_equation_reference_are_the_equations() {
        // Given
        let mut index = ProjectIndex::default();
        index.equations.insert(
            TargetName::new("euler"),
            rinx_index::EquationLocation::new("math.rst", 1),
        );
        let reference = InlineNode::EquationReference {
            label: "x".to_string(),
            span: None,
        };

        // When
        let found = candidates(&reference, &index, &EntitySchema::empty(), &asking());

        // Then
        assert_eq!(forms(&found), [vec!["euler"]]);
    }

    #[test]
    fn test_candidates_of_an_any_reference_are_every_kind_of_target() {
        // Given — a label and a function
        let mut index = ProjectIndex::default();
        index
            .targets
            .insert(TargetName::new("install"), "setup.rst".to_string());
        index.insert_domain_object(ObjectType::Py(PyObjectType::Function), "pkg.run", "a.rst");
        let reference = InlineNode::AnyReference {
            display: None,
            target: "x".to_string(),
            link: true,
            span: None,
            inventory: InventorySelector::Any,
        };

        // When
        let found = candidates(&reference, &index, &EntitySchema::empty(), &asking());

        // Then
        assert_eq!(forms(&found), [vec!["install"], vec!["run", "pkg.run"]]);
    }

    #[test]
    fn test_candidates_of_a_number_reference_are_the_numbered_labels() {
        // Given
        let mut index = ProjectIndex::default();
        index.numref_targets.insert(
            TargetName::new("fig-arch"),
            rinx_index::NumrefTarget {
                doc_path: "a.rst".to_string(),
                subject: rinx_index::NumrefSubject::Element {
                    ordinal: 0,
                    kind: rinx_ast::EnumerableKind::Figure,
                },
            },
        );
        let reference = InlineNode::NumberReference {
            title: None,
            target: "x".to_string(),
            link: true,
            span: None,
        };

        // When
        let found = candidates(&reference, &index, &EntitySchema::empty(), &asking());

        // Then
        assert_eq!(forms(&found), [vec!["fig-arch"]]);
    }

    #[test]
    fn test_dotted_suffixes_lists_the_shortest_first() {
        // When / Then
        assert_eq!(
            dotted_suffixes("pkg.Runner.run"),
            ["run", "Runner.run", "pkg.Runner.run"]
        );
        assert_eq!(dotted_suffixes("run"), ["run"]);
    }

    #[test]
    fn test_option_forms_lists_the_option_then_the_program_and_option() {
        // When / Then
        assert_eq!(option_forms("dis.-o"), ["-o", "dis -o", "dis.-o"]);
        assert_eq!(option_forms("-O"), ["-O"]);
    }

    #[test]
    fn test_written_forms_keeps_a_label_whole() {
        // When / Then
        assert_eq!(written_forms("std:label", "a.b"), ["a.b"]);
        assert_eq!(
            written_forms("py:function", "os.getcwd"),
            ["getcwd", "os.getcwd"]
        );
    }

    #[test]
    fn test_object_kind_marks_a_method_as_one() {
        // When / Then
        assert_eq!(
            object_kind(ObjectType::Py(PyObjectType::Method)),
            CompletionItemKind::METHOD
        );
    }

    #[test]
    fn test_entry_kind_marks_a_listed_class_as_one() {
        // When / Then
        assert_eq!(entry_kind("py:class"), CompletionItemKind::CLASS);
        assert_eq!(entry_kind("std:label"), CompletionItemKind::REFERENCE);
    }

    #[test]
    fn test_selector_admits_only_the_inventory_it_names() {
        // Given
        let inventory = python_inventory();
        let other = InventorySelector::Named(InventoryName::new("numpy").unwrap());

        // When / Then
        assert!(selector_admits(&InventorySelector::Any, &inventory));
        assert!(!selector_admits(&other, &inventory));
    }
}

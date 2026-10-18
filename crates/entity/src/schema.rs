use std::collections::BTreeMap;

use serde::Serialize;

use crate::backlinks::{BacklinkSpec, BacklinkTable, derive_backlinks};
use crate::entity_type::EntityType;
use crate::error::SchemaError;
use crate::role::{BUILTIN_ROLE, RoleSpec};

/// A project's entity meta-model: its types, its roles, and what follows.
///
/// Constructed only through [`EntitySchema::new`] or [`EntitySchema::empty`],
/// so a schema in hand has already had its cross-references checked and its
/// back-link table derived. Nothing downstream re-validates, and nothing can
/// observe a half-built one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntitySchema {
    types: Vec<EntityType>,
    roles: Vec<RoleSpec>,
    backlinks: BacklinkTable,
    hash: String,
    import_keys: BTreeMap<String, String>,
}

/// The shape hashed to identify a schema, independent of the presentation
/// details of the file it was written in.
///
/// [`EntitySchema::import_keys`] is deliberately **not** part of it. The hash
/// exists so a library and the site agree on the vocabulary the *index* and
/// the *renderer* read — that is what `entity.schema-mismatch` catches — and
/// an import key is read only while parsing. Adding it would make the hash
/// sensitive to something no later phase can observe, so leave it out.
#[derive(Serialize)]
struct SchemaFingerprint<'a> {
    types: &'a [EntityType],
    roles: &'a [RoleSpec],
}

impl EntitySchema {
    /// Builds a schema from already-parsed declarations.
    ///
    /// # Errors
    ///
    /// Returns every back-link derivation fault found. Declaration-level
    /// checks belong to [`crate::load`], which runs them before calling this.
    pub fn new(types: Vec<EntityType>, roles: Vec<RoleSpec>) -> Result<Self, Vec<SchemaError>> {
        let backlinks = derive_backlinks(&types)?;
        let hash = fingerprint(&types, &roles);
        Ok(Self {
            types,
            roles,
            backlinks,
            hash,
            import_keys: BTreeMap::new(),
        })
    }

    /// Records the `[import_keys]` table, as written.
    ///
    /// A builder rather than a parameter on [`Self::new`] because it changes
    /// nothing the constructor checks: the values are paths, whose meaning
    /// depends on where the schema file itself sits, and only the worker knows
    /// that. It resolves them; see `commands::entity_schema`.
    #[must_use]
    pub fn with_import_keys(mut self, import_keys: BTreeMap<String, String>) -> Self {
        self.import_keys = import_keys;
        self
    }

    /// The `[import_keys]` table: an alias a `.. needimport::` may write,
    /// mapped to the file it stands for, exactly as the schema spelled it.
    #[must_use]
    pub fn import_keys(&self) -> &BTreeMap<String, String> {
        &self.import_keys
    }

    /// The `[import_keys]` table resolved into the paths the parser's file
    /// loader takes, for a schema whose table was written in the file at
    /// `written_in` — a source-root-relative path.
    ///
    /// Two transformations, both of which must happen exactly once, in a
    /// phase that knows where that file sits:
    ///
    /// 1. **Anchored at the file**, by the rule every other written path in
    ///    this build follows (`rinx_ast::resolve_from_document`): a leading
    ///    `/` means the source root, anything else is relative to the file
    ///    that wrote it. That keeps a schema relocatable together with the
    ///    data files beside it, and takes sphinx-needs' own
    ///    `/needs_import.json` spelling verbatim.
    /// 2. **Prefixed with `/`** afterwards, so the parser can hand it to its
    ///    file loader with no anchor of its own and get source-root
    ///    resolution: a project-level setting names a file from the
    ///    project's root, not from whichever document happens to mention it.
    #[must_use]
    pub fn resolved_import_keys(&self, written_in: &str) -> BTreeMap<String, String> {
        self.import_keys
            .iter()
            .map(|(alias, written)| {
                let resolved = rinx_ast::resolve_from_document(written, written_in);
                (alias.clone(), format!("/{}", resolved.to_string_lossy()))
            })
            .collect()
    }

    /// A borrowable empty schema with the process's lifetime.
    ///
    /// Exists because [`crate::EntitySchema`] is threaded through the parser
    /// by reference, and a context built without one still needs something to
    /// point at. Allocated once.
    #[must_use]
    pub fn empty_ref() -> &'static Self {
        static EMPTY: std::sync::OnceLock<EntitySchema> = std::sync::OnceLock::new();
        EMPTY.get_or_init(Self::empty)
    }

    /// The schema a project without one uses.
    ///
    /// Every lookup misses, so the parser recognises no entity directives and
    /// the pipeline behaves exactly as it did before entities existed.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            types: Vec::new(),
            roles: Vec::new(),
            backlinks: BacklinkTable::new(),
            hash: fingerprint(&[], &[]),
            import_keys: BTreeMap::new(),
        }
    }

    /// Reports whether this schema declares no entity types at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.types.is_empty()
    }

    /// Looks up an entity type by its directive name.
    #[must_use]
    pub fn entity_type(&self, name: &str) -> Option<&EntityType> {
        self.types.iter().find(|t| t.name == name)
    }

    /// Looks up a role by its spelling.
    ///
    /// The built-in `:entity:` role resolves here without being declared, so a
    /// schema that names no roles is still linkable. A declared role of the
    /// same name wins, letting a project narrow it if it wants to.
    #[must_use]
    pub fn role(&self, name: &str) -> Option<RoleSpec> {
        if let Some(declared) = self.roles.iter().find(|r| r.name == name) {
            return Some(declared.clone());
        }
        if name == BUILTIN_ROLE {
            return Some(RoleSpec::builtin());
        }
        None
    }

    /// Whether a role of this name exists in any letter case — the built-in
    /// `:entity:` included.
    ///
    /// Not a looser [`Self::role`]: a role reference is still matched by its
    /// exact spelling. This answers the question a document's own
    /// `.. role::` asks, whose names are case-insensitive, and which must not
    /// define a role overlapping an entity role in any spelling.
    #[must_use]
    pub fn has_role_ignoring_case(&self, name: &str) -> bool {
        name.eq_ignore_ascii_case(BUILTIN_ROLE)
            || self
                .roles
                .iter()
                .any(|role| role.name.eq_ignore_ascii_case(name))
    }

    /// The back-links an entity of `type_name` can receive.
    #[must_use]
    pub fn backlinks_for(&self, type_name: &str) -> &[BacklinkSpec] {
        self.backlinks
            .get(type_name)
            .map_or(&[], |specs| specs.as_slice())
    }

    /// A hex digest identifying this schema's content.
    ///
    /// Recorded in each parsed document so the index phase can tell that a
    /// library was parsed against a different schema than the site is indexing
    /// with — a build misconfiguration that would otherwise produce quietly
    /// wrong output rather than an error.
    #[must_use]
    pub fn hash(&self) -> &str {
        &self.hash
    }

    #[must_use]
    pub fn types(&self) -> &[EntityType] {
        &self.types
    }

    #[must_use]
    pub fn roles(&self) -> &[RoleSpec] {
        &self.roles
    }
}

impl Default for EntitySchema {
    fn default() -> Self {
        Self::empty()
    }
}

/// Hashes a schema's declarations into a stable hex digest.
fn fingerprint(types: &[EntityType], roles: &[RoleSpec]) -> String {
    use sha2::{Digest, Sha256};
    use std::fmt::Write as _;

    let canonical = serde_json::to_string(&SchemaFingerprint { types, roles })
        .expect("entity schema declarations are always serializable");
    let digest = Sha256::digest(canonical.as_bytes());
    digest.iter().fold(String::new(), |mut acc, b| {
        let _ = write!(acc, "{b:02x}");
        acc
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::argument::ArgumentSpec;
    use crate::id::IdSpec;
    use crate::relation::RelationSpec;

    fn bare_type(name: &str) -> EntityType {
        EntityType {
            name: name.to_string(),
            label: None,
            argument: ArgumentSpec::default(),
            id: IdSpec::default(),
            template: None,
            attributes: Vec::new(),
            sections: Vec::new(),
            relations: Vec::new(),
        }
    }

    fn linking_type(name: &str, to: &str) -> EntityType {
        let mut entity_type = bare_type(name);
        entity_type.relations = vec![RelationSpec {
            name: "links".to_string(),
            label: None,
            to: Some(vec![to.to_string()]),
            required: false,
            multiple: true,
            incoming: Some("linked_by".to_string()),
            incoming_label: Some("Linked by".to_string()),
        }];
        entity_type
    }

    #[test]
    fn test_empty_schema_recognises_nothing() {
        // Given
        let schema = EntitySchema::empty();

        // When / Then
        assert!(schema.is_empty());
        assert!(schema.entity_type("req").is_none());
        assert!(schema.types().is_empty());
        assert!(schema.roles().is_empty());
    }

    #[test]
    fn test_schema_looks_up_a_declared_type() {
        // Given
        let schema = EntitySchema::new(vec![bare_type("req")], Vec::new()).unwrap();

        // When
        let found = schema.entity_type("req");

        // Then
        assert_eq!(found.map(|t| t.name.as_str()), Some("req"));
        assert!(!schema.is_empty());
    }

    #[test]
    fn test_schema_resolves_the_builtin_role_without_a_declaration() {
        // Given
        let schema = EntitySchema::new(vec![bare_type("req")], Vec::new()).unwrap();

        // When
        let role = schema.role("entity").unwrap();

        // Then
        assert!(role.accepts("req"));
    }

    #[test]
    fn test_schema_lets_a_declared_role_narrow_the_builtin_name() {
        // Given
        let narrowed = RoleSpec {
            name: "entity".to_string(),
            types: Some(vec!["req".to_string()]),
        };
        let schema =
            EntitySchema::new(vec![bare_type("req"), bare_type("spec")], vec![narrowed]).unwrap();

        // When
        let role = schema.role("entity").unwrap();

        // Then
        assert!(role.accepts("req"));
        assert!(!role.accepts("spec"));
    }

    #[test]
    fn test_has_role_ignoring_case_finds_a_declared_role_in_any_case() {
        // Given a role declared with a capital
        let declared = RoleSpec {
            name: "Req".to_string(),
            types: None,
        };
        let schema = EntitySchema::new(vec![bare_type("req")], vec![declared]).unwrap();

        // When / Then — `role` would miss every spelling but `Req`
        assert!(schema.role("req").is_none());
        assert!(schema.has_role_ignoring_case("req"));
        assert!(schema.has_role_ignoring_case("REQ"));
    }

    #[test]
    fn test_has_role_ignoring_case_finds_the_builtin_role() {
        // Given
        let schema = EntitySchema::empty();

        // When / Then
        assert!(schema.has_role_ignoring_case("Entity"));
    }

    #[test]
    fn test_has_role_ignoring_case_misses_an_undeclared_role() {
        // Given
        let schema = EntitySchema::new(vec![bare_type("req")], Vec::new()).unwrap();

        // When / Then
        assert!(!schema.has_role_ignoring_case("need"));
    }

    #[test]
    fn test_schema_misses_an_undeclared_role() {
        // Given
        let schema = EntitySchema::new(vec![bare_type("req")], Vec::new()).unwrap();

        // When
        let role = schema.role("need");

        // Then
        assert!(role.is_none());
    }

    #[test]
    fn test_schema_exposes_the_derived_backlinks_of_a_type() {
        // Given
        let schema = EntitySchema::new(
            vec![linking_type("req", "spec"), bare_type("spec")],
            Vec::new(),
        )
        .unwrap();

        // When
        let on_target = schema.backlinks_for("spec");
        let on_source = schema.backlinks_for("req");

        // Then
        assert_eq!(on_target.len(), 1);
        assert_eq!(on_target[0].label, "Linked by");
        assert!(on_source.is_empty());
    }

    #[test]
    fn test_schema_reports_backlink_faults_rather_than_building() {
        // Given — two labels for one back-link on a shared target
        let mut first = linking_type("req", "spec");
        let mut second = linking_type("test", "spec");
        second.relations[0].name = "verifies".to_string();
        second.relations[0].incoming_label = Some("Verified by".to_string());
        first.relations[0].incoming_label = Some("Linked by".to_string());

        // When
        let result = EntitySchema::new(vec![first, second, bare_type("spec")], Vec::new());

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn test_schema_hash_is_stable_for_identical_declarations() {
        // Given
        let first = EntitySchema::new(vec![bare_type("req")], Vec::new()).unwrap();
        let second = EntitySchema::new(vec![bare_type("req")], Vec::new()).unwrap();

        // When / Then
        assert_eq!(first.hash(), second.hash());
        assert!(!first.hash().is_empty());
    }

    #[test]
    fn test_schema_hash_changes_with_the_declarations() {
        // Given
        let one_type = EntitySchema::new(vec![bare_type("req")], Vec::new()).unwrap();
        let two_types =
            EntitySchema::new(vec![bare_type("req"), bare_type("spec")], Vec::new()).unwrap();
        let with_role = EntitySchema::new(
            vec![bare_type("req")],
            vec![RoleSpec {
                name: "need".to_string(),
                types: None,
            }],
        )
        .unwrap();

        // When / Then
        assert_ne!(one_type.hash(), two_types.hash());
        assert_ne!(one_type.hash(), with_role.hash());
    }

    #[test]
    fn test_empty_ref_is_the_empty_schema_and_is_shared() {
        // Given / When
        let first = EntitySchema::empty_ref();
        let second = EntitySchema::empty_ref();

        // Then
        assert!(first.is_empty());
        assert!(std::ptr::eq(first, second));
    }

    #[test]
    fn test_default_schema_is_the_empty_one() {
        // Given / When
        let schema = EntitySchema::default();

        // Then
        assert_eq!(schema, EntitySchema::empty());
    }

    #[test]
    fn test_import_keys_do_not_change_the_schema_fingerprint() {
        // Given one schema, and the same schema with an import key added
        let plain = EntitySchema::new(Vec::new(), Vec::new()).expect("valid");
        let hash = plain.hash().to_string();
        let keyed = EntitySchema::new(Vec::new(), Vec::new())
            .expect("valid")
            .with_import_keys(BTreeMap::from([(
                "upstream".to_string(),
                "/needs.json".to_string(),
            )]));

        // Then the fingerprint is unmoved. It exists so a library and the site
        // agree on the vocabulary the *index* and *renderer* read, and an
        // import key is read only while parsing — making the hash sensitive to
        // it would report `entity.schema-mismatch` for a difference no later
        // phase can observe.
        assert_eq!(keyed.hash(), hash);
    }

    #[test]
    fn test_a_declared_type_does_change_the_schema_fingerprint() {
        // Given an empty schema and one declaring a type
        let empty = EntitySchema::empty();
        let with_type = EntitySchema::new(
            vec![crate::entity_type::EntityType {
                name: "req".to_string(),
                label: None,
                argument: crate::argument::ArgumentSpec::default(),
                id: crate::id::IdSpec::default(),
                template: None,
                attributes: Vec::new(),
                sections: Vec::new(),
                relations: Vec::new(),
            }],
            Vec::new(),
        )
        .expect("valid");

        // Then the hash moves — the pin above narrows what the fingerprint
        // ignores, it does not disable it
        assert_ne!(with_type.hash(), empty.hash());
    }

    /// A schema declaring `import_keys`, for the resolution tests below.
    fn keyed_schema(pairs: &[(&str, &str)]) -> EntitySchema {
        let table = pairs
            .iter()
            .map(|(alias, path)| format!("{alias} = \"{path}\""))
            .collect::<Vec<_>>()
            .join("\n");
        crate::load_schema(
            &format!("[import_keys]\n{table}\n[[entity_type]]\nname = \"req\"\n"),
            &crate::NoReservedNames,
        )
        .expect("the fixture schema is valid")
    }

    #[test]
    fn test_a_bare_import_key_resolves_beside_the_schema_file() {
        // Given a schema nested in the tree, naming a file with no leading `/`
        let schema = keyed_schema(&[("upstream", "needs.json")]);

        // When its keys are resolved
        let resolved = schema.resolved_import_keys("examples/entities/entities.toml");

        // Then the file is found beside the schema, which is what lets a
        // schema and its data files move through the tree together
        assert_eq!(
            resolved.get("upstream").map(String::as_str),
            Some("/examples/entities/needs.json")
        );
    }

    #[test]
    fn test_a_slash_prefixed_import_key_resolves_at_the_source_root() {
        // Given sphinx-needs' own spelling, which is source-root-relative
        let schema = keyed_schema(&[("imported_project", "/needs_import.json")]);

        // When its keys are resolved
        let resolved = schema.resolved_import_keys("deep/nested/entities.toml");

        // Then the leading `/` wins over the schema's location, so a converted
        // `ubproject.toml` value carries across verbatim
        assert_eq!(
            resolved.get("imported_project").map(String::as_str),
            Some("/needs_import.json")
        );
    }

    #[test]
    fn test_every_resolved_import_key_is_prefixed_for_the_loader() {
        // Given keys written both ways
        let schema = keyed_schema(&[("a", "beside.json"), ("b", "/root.json")]);

        // When they are resolved
        let resolved = schema.resolved_import_keys("docs/entities.toml");

        // Then both come out `/`-prefixed, so the parser can hand them to the
        // file loader with no anchor of its own and get source-root resolution
        assert!(
            resolved.values().all(|path| path.starts_with('/')),
            "{resolved:?}"
        );
    }

    #[test]
    fn test_a_schema_without_import_keys_resolves_to_an_empty_map() {
        // Given a schema declaring none
        let schema =
            crate::load_schema("[[entity_type]]\nname = \"req\"\n", &crate::NoReservedNames)
                .expect("valid");

        // When its keys are resolved
        // Then there are none, and every lookup simply misses
        assert!(schema.resolved_import_keys("entities.toml").is_empty());
    }
}

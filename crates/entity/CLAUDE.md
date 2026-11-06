# Notes on entities (`rinx_entity`; guide: `docs/entities.rst`)

- A schema declares a project's own directive types. It is a **parse-time input** (it is what makes `.. req::` a directive), so `entity_schema` is set on both `rinx_library` and `rinx_site`; a mismatch is `entity.schema-mismatch`.
- The four kinds of declaration are not interchangeable: *attributes* are typed values, indexed and never parsed as RST; *sections* are parsed RST, not indexed; *relations* are declared on the carrying type, with back-links always derived; *roles* are optional sugar.
- This crate owns the meta-model; `rinx_ast`'s `entity/` owns instance data, since that is what a `.ast` holds (the dependency is `ast → entity`).
- Every value, whether written, imported (`.. needimport::`) or updated (`.. entity-update::`), goes through `parse_attribute_value`, so the three routes cannot validate differently.
- `schemas/entities.schema.json` is generated (`cargo run -p rinx -- entity_json_schema`) and checked in; a test compares.

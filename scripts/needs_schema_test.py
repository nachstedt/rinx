import needs_schema
from needs_schema import RawTable


def convert(needs: RawTable, schemas: RawTable | None = None) -> tuple[str, needs_schema.Report]:
    """Convert a `[needs]` table on its own, returning (toml_text, report)."""
    return needs_schema.convert({"needs": needs}, schemas)


def report_categories(report: needs_schema.Report) -> set[str]:
    return {category for category, _detail in report}


class TestEntityType:
    def test_need_type_becomes_an_entity_type_with_a_title_argument(self) -> None:
        # Given one need type carrying a title and an id prefix
        needs = {"types": [{"directive": "req", "title": "Requirement", "prefix": "R_"}]}

        # When
        text, _report = convert(needs)

        # Then the directive name, label and prefix carry over, and the
        # argument fills a `title` attribute
        assert 'name = "req"' in text
        assert 'label = "Requirement"' in text
        assert 'id = { prefix = "R_" }' in text
        assert 'argument = { fields = ["title"] }' in text
        assert 'name = "title"' in text

    def test_id_required_is_carried_onto_every_type(self) -> None:
        # Given a project that forces authors to write an explicit :id:
        needs = {"id_required": True, "types": [{"directive": "req", "prefix": "R_"}]}

        # When
        text, _report = convert(needs)

        # Then
        assert 'id = { prefix = "R_", required = true }' in text

    def test_a_type_without_a_title_falls_back_to_its_directive_name(self) -> None:
        # Given a need type declaring no title
        needs = {"types": [{"directive": "spec"}]}

        # When
        text, _report = convert(needs)

        # Then
        assert 'name = "spec"\nlabel = "spec"' in text


class TestAttributeFromField:
    def test_field_without_a_schema_is_an_untyped_string(self) -> None:
        # Given
        field = {"nullable": True}

        # When
        attribute = needs_schema.attribute_from_field("contact", field)

        # Then
        assert attribute == {"name": "contact", "label": "Contact", "type": "string"}

    def test_integer_boolean_and_array_schemas_map_onto_their_types(self) -> None:
        # Given three fields typed by their JSON-Schema fragment
        cases = {
            "integer": "int",
            "number": "int",
            "boolean": "bool",
            "array": "list<string>",
        }

        for json_type, expected in cases.items():
            # When
            attribute = needs_schema.attribute_from_field("value", {"schema": {"type": json_type}})

            # Then
            assert attribute["type"] == expected, json_type

    def test_enum_values_become_strings_even_when_written_as_numbers(self) -> None:
        # Given sphinx-needs' `effort` field, whose enum holds integers
        field = {
            "description": "Story points",
            "schema": {"type": "integer", "enum": [1, 2, 3, 5]},
        }

        # When
        attribute = needs_schema.attribute_from_field("effort", field)

        # Then the type is an enum and every value is a string, which is the
        # only spelling our model has
        assert attribute["type"] == "enum"
        assert attribute["values"] == ["1", "2", "3", "5"]

    def test_an_enum_array_becomes_a_list_of_enum(self) -> None:
        # Given
        field = {"schema": {"type": "array", "enum": ["a", "b"]}}

        # When
        attribute = needs_schema.attribute_from_field("kinds", field)

        # Then
        assert attribute["type"] == "list<enum>"
        assert attribute["values"] == ["a", "b"]

    def test_description_is_preferred_over_a_derived_label(self) -> None:
        # Given a field that documents itself
        field = {"description": "Automotive Safety Integrity Level"}

        # When
        attribute = needs_schema.attribute_from_field("asil", field)

        # Then
        assert attribute["label"] == "Automotive Safety Integrity Level"


class TestGlobalFields:
    def test_sphinx_needs_builtins_are_declared_even_when_the_project_is_not(self) -> None:
        # Given a project declaring no fields of its own
        # When
        fields = needs_schema.global_fields({})

        # Then the built-in need options are still there — a `:tags:` in the
        # corpus is sphinx-needs' vocabulary, not an unknown attribute
        assert fields["tags"]["type"] == "list<string>"
        assert fields["collapse"]["type"] == "bool"

    def test_a_projects_own_declaration_overrides_the_builtin(self) -> None:
        # Given a project that types `status` itself
        needs = {"fields": {"status": {"schema": {"type": "boolean"}}}}

        # When
        fields = needs_schema.global_fields(needs)

        # Then
        assert fields["status"]["type"] == "bool"

    def test_statuses_table_turns_status_into_an_enum(self) -> None:
        # Given a project listing its allowed statuses
        needs = {"statuses": [{"name": "open"}, {"name": "closed"}]}

        # When
        fields = needs_schema.global_fields(needs)

        # Then
        assert fields["status"]["type"] == "enum"
        assert fields["status"]["values"] == ["open", "closed"]

    def test_id_and_title_are_not_attributes(self) -> None:
        # Given a project declaring fields our model handles structurally
        needs = {"fields": {"id": {}, "title": {}, "type": {}, "owner": {}}}

        # When
        fields = needs_schema.global_fields(needs)

        # Then only the ordinary one survives — `id` is the entity id and
        # `title` comes from the directive argument
        assert "id" not in fields
        assert "title" not in fields
        assert "type" not in fields
        assert "owner" in fields


class TestRelationsFromLinks:
    def test_link_becomes_a_relation_with_a_derived_backlink_option(self) -> None:
        # Given one link type with both labels
        needs = {"links": {"reqs": {"outgoing": "specifies", "incoming": "specified by"}}}
        report = []

        # When
        relations = needs_schema.relations_from_links(needs, report)

        # Then the TOML key is the option spelling, the two sphinx-needs
        # strings are labels, and the back-link takes sphinx-needs' own
        # `<name>_back` spelling
        assert relations == [
            {
                "name": "reqs",
                "label": "specifies",
                "multiple": True,
                "incoming": "reqs_back",
                "incoming_label": "specified by",
            }
        ]

    def test_relation_accepts_any_target_type(self) -> None:
        # Given a link type — sphinx-needs never scopes one to a target type
        needs = {"links": {"links": {}}}

        # When
        relations = needs_schema.relations_from_links(needs, [])

        # Then `to` is left off, which is how our model spells "any type"
        assert "to" not in relations[0]

    def test_a_link_computed_by_a_dynamic_function_is_reported(self) -> None:
        # Given sphinx-test-reports' auto-linked `runs`
        needs = {"links": {"runs": {"predicates": [["type=='test'", "[[tr_link()]]"]]}}}
        report = []

        # When
        needs_schema.relations_from_links(needs, report)

        # Then the relation is still declared, and the fact that nothing fills
        # it in is reported rather than hidden
        assert "link predicates" in report_categories(report)


class TestSchemaNarrowing:
    def test_unconditionally_required_option_is_narrowed_onto_one_type(self) -> None:
        # Given a schemas.json rule requiring `role` on `person` only
        schemas = {
            "schemas": [
                {
                    "id": "person-has-role",
                    "select": {"$ref": "#/$defs/type-person"},
                    "validate": {"local": {"required": ["role"]}},
                }
            ]
        }

        # When
        constraints = needs_schema.constraints_by_type(schemas, [])

        # Then
        assert constraints["person"].required == {"role"}

    def test_narrowing_marks_the_attribute_required_on_that_type_alone(self) -> None:
        # Given two types and a rule scoped to one of them
        needs = {
            "types": [{"directive": "person"}, {"directive": "team"}],
            "fields": {"role": {}},
        }
        schemas = {
            "schemas": [
                {
                    "id": "person-has-role",
                    "select": {"$ref": "#/$defs/type-person"},
                    "validate": {"local": {"required": ["role"]}},
                }
            ]
        }

        # When
        text, _report = convert(needs, schemas)

        # Then `person`'s block carries the required marker and `team`'s does not
        _preamble, person, team = text.split("[[entity_type]]")
        assert 'name = "person"' in person
        assert 'name = "role"' in person
        assert "required = true" in person
        assert 'name = "team"' in team
        assert "required = true" not in team

    def test_a_conditional_rule_is_reported_rather_than_honoured(self) -> None:
        # Given a rule that requires an option only for documents under a path
        schemas = {
            "schemas": [
                {
                    "id": "automotive-req-status-required",
                    "select": {"allOf": [{"$ref": "#/$defs/type-req"}]},
                    "validate": {"local": {"required": ["status"]}},
                }
            ]
        }
        report = []

        # When
        constraints = needs_schema.constraints_by_type(schemas, report)

        # Then nothing is narrowed — honouring half of a conditional rule would
        # be worse than not honouring it — and the rule is reported
        assert constraints == {}
        assert "conditional schema rule" in report_categories(report)

    def test_a_network_rule_is_reported(self) -> None:
        # Given a cross-entity constraint
        schemas = {
            "schemas": [
                {
                    "id": "spec-links-to-req",
                    "select": {"$ref": "#/$defs/type-spec"},
                    "validate": {"network": {"links": {}}},
                },
            ]
        }
        report = []

        # When
        needs_schema.constraints_by_type(schemas, report)

        # Then
        assert report_categories(report) == {"network schema rule"}


def value_rules(report: needs_schema.Report) -> needs_schema.Report:
    """The report's value-rule entries alone.

    That is without the unrelated ones every conversion carries (such as the
    generic `.. need::` directive).
    """
    return [entry for entry in report if entry[0] == "value schema rule"]


def type_rule(rule_id: str, type_name: str, local: RawTable) -> RawTable:
    """A `schemas.json` holding one rule selecting exactly `type_name`."""
    return {
        "schemas": [
            {
                "id": rule_id,
                "select": {"$ref": f"#/$defs/type-{type_name}"},
                "validate": {"local": local},
            }
        ]
    }


class TestValueRule:
    def test_an_id_pattern_becomes_the_types_id_pattern(self) -> None:
        # Given the shape of the demo corpus' eleven `*-id-pattern` rules
        needs = {"types": [{"directive": "req", "prefix": "R_"}]}
        schemas = type_rule(
            "req-id-pattern", "req", {"properties": {"id": {"pattern": "^R_[0-9]+$"}}}
        )

        # When
        text, report = convert(needs, schemas)

        # Then the pattern is carried over onto the id, and
        # nothing is left to report
        assert 'id = { prefix = "R_", pattern = "^R_[0-9]+$" }' in text
        assert value_rules(report) == []

    def test_a_pattern_escapes_its_backslashes_for_toml(self) -> None:
        # Given
        needs = {"types": [{"directive": "req"}]}
        schemas = type_rule(
            "req-id-pattern", "req", {"properties": {"id": {"pattern": r"^R_\d+$"}}}
        )

        # When
        text, _report = convert(needs, schemas)

        # Then the TOML string decodes back to the regex as written
        assert r'id = { pattern = "^R_\\d+$" }' in text

    def test_a_pattern_on_a_string_field_becomes_the_attributes_pattern(self) -> None:
        # Given
        needs = {"types": [{"directive": "person"}], "fields": {"email": {}}}
        schemas = type_rule("person-email", "person", {"properties": {"email": {"pattern": "@"}}})

        # When
        text, report = convert(needs, schemas)

        # Then
        assert 'name = "email"\nlabel = "Email"\ntype = "string"\npattern = "@"' in text
        assert value_rules(report) == []

    def test_an_enum_on_a_string_field_turns_it_into_an_enum(self) -> None:
        # Given
        needs = {"types": [{"directive": "hazard"}], "fields": {"asil": {}}}
        schemas = type_rule(
            "hazard-asil", "hazard", {"properties": {"asil": {"enum": ["QM", "A"]}}}
        )

        # When
        text, report = convert(needs, schemas)

        # Then
        assert 'type = "enum"\nvalues = ["QM", "A"]' in text
        assert value_rules(report) == []

    def test_the_narrowing_applies_to_the_selected_type_alone(self) -> None:
        # Given two types and a pattern scoped to one of them
        needs = {
            "types": [{"directive": "person"}, {"directive": "team"}],
            "fields": {"email": {}},
        }
        schemas = type_rule("person-email", "person", {"properties": {"email": {"pattern": "@"}}})

        # When
        text, _report = convert(needs, schemas)

        # Then
        _preamble, person, team = text.split("[[entity_type]]")
        assert 'pattern = "@"' in person
        assert "pattern" not in team

    def test_minimums_of_one_and_empty_fragments_need_nothing_emitted(self) -> None:
        # Given the demo corpus' `person-has-role`, `team-has-persons`,
        # `impl-has-implements-links` and `test-has-spec-or-impl` shapes
        needs = {
            "types": [{"directive": "person"}],
            "fields": {"role": {}},
            "links": {"persons": {}, "links": {}},
        }
        schemas = type_rule(
            "person-rules",
            "person",
            {
                "properties": {
                    "role": {"type": "string", "minLength": 1},
                    "persons": {"type": "array", "minItems": 1},
                    "links": {},
                },
                "required": ["role"],
            },
        )

        # When
        text, report = convert(needs, schemas)

        # Then `required` still applies, and nothing is reported as missing
        assert "required = true" in text
        assert value_rules(report) == []

    def test_an_inexpressible_keyword_is_reported_by_name(self) -> None:
        # Given a minimum our model has no vocabulary for
        schemas = type_rule(
            "person-long-role",
            "person",
            {"properties": {"role": {"minLength": 3, "pattern": "."}}},
        )
        report = []

        # When
        constraints = needs_schema.constraints_by_type(schemas, report)

        # Then the pattern is still taken, and only the keyword is reported
        assert constraints["person"].patterns == {"role": "."}
        assert report == [
            (
                "value schema rule",
                (
                    "person-long-role: `role` uses `minLength: 3`, which the "
                    "entity model cannot express"
                ),
            )
        ]

    def test_a_second_id_pattern_for_one_type_is_reported(self) -> None:
        # Given two rules each constraining `req`'s id differently
        first = type_rule("a", "req", {"properties": {"id": {"pattern": "^A"}}})
        second = type_rule("b", "req", {"properties": {"id": {"pattern": "^B"}}})
        schemas = {"schemas": first["schemas"] + second["schemas"]}
        report = []

        # When
        constraints = needs_schema.constraints_by_type(schemas, report)

        # Then the first is kept, and the conflict is reported
        assert constraints["req"].id_pattern == "^A"
        assert report_categories(report) == {"value schema rule"}

    def test_a_pattern_on_a_link_is_reported(self) -> None:
        # Given a pattern on a link, whose values are entity ids
        needs = {"types": [{"directive": "spec"}], "links": {"reqs": {}}}
        schemas = type_rule("spec-reqs", "spec", {"properties": {"reqs": {"pattern": "^R_"}}})

        # When
        _text, report = convert(needs, schemas)

        # Then
        [(_category, detail)] = value_rules(report)
        assert "link `reqs`" in detail

    def test_a_pattern_on_a_non_text_field_is_reported(self) -> None:
        # Given a pattern on an integer field
        needs = {
            "types": [{"directive": "req"}],
            "fields": {"effort": {"schema": {"type": "integer"}}},
        }
        schemas = type_rule("req-effort", "req", {"properties": {"effort": {"pattern": "^[0-9]$"}}})

        # When
        text, report = convert(needs, schemas)

        # Then nothing is emitted, and the rule is reported
        assert "pattern" not in text
        [(_category, detail)] = value_rules(report)
        assert "`int`" in detail

    def test_a_constraint_on_an_undeclared_field_is_reported(self) -> None:
        # Given a rule naming a field the project never declares
        needs = {"types": [{"directive": "req"}]}
        schemas = type_rule("req-ghost", "req", {"properties": {"ghost": {"pattern": "."}}})

        # When
        _text, report = convert(needs, schemas)

        # Then
        [(_category, detail)] = value_rules(report)
        assert "does not declare" in detail


class TestOptionNameClash:
    def test_a_name_that_is_both_a_field_and_a_link_becomes_the_relation(self) -> None:
        # Given `spec` declared as a field and as a link type — our model
        # forbids one name being an attribute and a relation on one type
        needs = {
            "types": [{"directive": "test"}],
            "fields": {"spec": {}},
            "links": {"spec": {"outgoing": "specs"}},
        }

        # When
        text, _report = convert(needs)

        # Then it appears once, as the relation
        assert text.count('name = "spec"') == 1
        block = text[text.index("[[entity_type.relation]]") :]
        assert 'name = "spec"' in block


class TestRoles:
    def test_one_role_per_type_plus_the_sphinx_needs_spelling(self) -> None:
        # Given two types
        names = ["req", "spec"]

        # When
        declared = needs_schema.roles(names)

        # Then each type gets its own role, and `:need:` resolves to any of them
        assert declared == [
            {"name": "req", "types": ["req"]},
            {"name": "spec", "types": ["spec"]},
            {"name": "need", "types": ["req", "spec"]},
        ]

    def test_a_type_named_need_is_not_shadowed_by_a_second_need_role(self) -> None:
        # Given a project that names a type `need`
        names = ["need"]

        # When
        declared = needs_schema.roles(names)

        # Then only one role of that name is emitted — a duplicate is a schema
        # error
        assert declared == [{"name": "need", "types": ["need"]}]


class TestUnsupportedConstructs:
    def test_constraints_variants_and_global_options_are_reported(self) -> None:
        # Given a configuration using the three sphinx-needs features our model
        # has no vocabulary for
        needs = {
            "constraints": {"release_set": {}},
            "variants": {"var": "x"},
            "global_options": {"status": "open"},
            "fields": {"jira": {"parse_variants": True}},
        }

        # When
        report = needs_schema.unsupported_constructs(needs)

        # Then
        assert {"constraints", "variants", "global options"} <= report_categories(report)

    def test_the_generic_need_directive_is_reported(self) -> None:
        # Given a configuration that does not declare `need` as a type of its
        # own — `.. need::` then names its type in an option, which
        # one-directive-per-type cannot express
        # When
        report = needs_schema.unsupported_constructs({"types": [{"directive": "req"}]})

        # Then
        assert "generic need directive" in report_categories(report)

    def test_a_project_declaring_need_as_a_type_is_using_the_spelling(self) -> None:
        # Given a project whose own vocabulary includes a type called `need`
        # When
        report = needs_schema.unsupported_constructs({"types": [{"directive": "need"}]})

        # Then nothing is reported — it is an ordinary type, not the built-in
        assert "generic need directive" not in report_categories(report)


class TestTomlRendering:
    def test_strings_bools_and_lists_render_as_toml(self) -> None:
        # Given the three value shapes the schema uses
        # When / Then
        assert needs_schema.toml_value('a"b') == '"a\\"b"'
        assert needs_schema.toml_value(True) == "true"
        assert needs_schema.toml_value(False) == "false"
        assert needs_schema.toml_value(["a", "b"]) == '["a", "b"]'

    def test_the_generated_file_announces_that_it_is_generated(self) -> None:
        # Given any conversion
        # When
        text, _report = convert({"types": [{"directive": "req"}]})

        # Then a reader who opens it in the cloned corpus is told not to edit it
        assert "GENERATED by scripts/needs_schema.py" in text


class TestImportKeys:
    """`[needs.import_keys]` maps a name a `.. needimport::` may write onto the file it stands for.

    This build reads the same table from the entity schema, so the conversion is
    a straight copy.
    """

    def test_import_keys_are_carried_over_verbatim(self) -> None:
        # Given the table the sphinx-needs demo declares
        needs = {"import_keys": {"imported_project": "/needs_import.json"}}

        # When it is converted
        text, _report = convert(needs)

        # Then the value crosses unchanged. sphinx-needs writes it
        # source-root-relative with a leading `/`, which is exactly how this
        # build resolves a path in an entity schema, so there is nothing to
        # translate.
        assert "[import_keys]" in text
        assert 'imported_project = "/needs_import.json"' in text

    def test_the_table_is_emitted_before_the_first_entity_type(self) -> None:
        # Given a project declaring both an import key and a type
        needs = {
            "import_keys": {"upstream": "/u.json"},
            "types": [{"directive": "req", "title": "Requirement"}],
        }

        # When it is converted
        text, _report = convert(needs)

        # Then the top-level table comes first. Written after an array of
        # tables it would still be top-level, but it reads as if it were
        # nested — and between `[[entity_type]]` and `[[entity_type.attribute]]`
        # it would break the nesting outright.
        assert text.index("[import_keys]") < text.index("[[entity_type]]")

    def test_a_project_without_import_keys_emits_no_table(self) -> None:
        # Given a project that declares none
        needs = {"types": [{"directive": "req", "title": "Requirement"}]}

        # When it is converted
        text, _report = convert(needs)

        # Then nothing is emitted, rather than an empty table
        assert "[import_keys]" not in text

    def test_import_keys_are_not_reported_as_unconvertible(self) -> None:
        # Given a project declaring an import key
        needs = {"import_keys": {"upstream": "/u.json"}}

        # When it is converted
        _text, report = convert(needs)

        # Then it is absent from the report: the construct is supported, and
        # listing it would overstate what a migrating project loses.
        assert "import keys" not in report_categories(report)

    def test_import_keys_reads_the_table_on_its_own(self) -> None:
        # Given the table, and a project without one
        # When each is read
        # Then the helper is a plain copy, and missing means empty
        assert needs_schema.import_keys({"import_keys": {"a": "/a.json"}}) == {"a": "/a.json"}
        assert needs_schema.import_keys({}) == {}

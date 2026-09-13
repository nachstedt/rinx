import unittest

import needs_schema


def convert(needs, schemas=None):
    """Convert a `[needs]` table on its own, returning (toml_text, report)."""
    return needs_schema.convert({"needs": needs}, schemas)


def report_categories(report):
    return {category for category, _detail in report}


class EntityTypeTest(unittest.TestCase):
    def test_need_type_becomes_an_entity_type_with_a_title_argument(self):
        # Given one need type carrying a title and an id prefix
        needs = {"types": [{"directive": "req", "title": "Requirement", "prefix": "R_"}]}

        # When
        text, _report = convert(needs)

        # Then the directive name, label and prefix carry over, and the
        # argument fills a `title` attribute
        self.assertIn('name = "req"', text)
        self.assertIn('label = "Requirement"', text)
        self.assertIn('id = { prefix = "R_" }', text)
        self.assertIn('argument = { fields = ["title"] }', text)
        self.assertIn('name = "title"', text)

    def test_id_required_is_carried_onto_every_type(self):
        # Given a project that forces authors to write an explicit :id:
        needs = {"id_required": True, "types": [{"directive": "req", "prefix": "R_"}]}

        # When
        text, _report = convert(needs)

        # Then
        self.assertIn('id = { prefix = "R_", required = true }', text)

    def test_a_type_without_a_title_falls_back_to_its_directive_name(self):
        # Given a need type declaring no title
        needs = {"types": [{"directive": "spec"}]}

        # When
        text, _report = convert(needs)

        # Then
        self.assertIn('name = "spec"\nlabel = "spec"', text)


class AttributeFromFieldTest(unittest.TestCase):
    def test_field_without_a_schema_is_an_untyped_string(self):
        # Given
        field = {"nullable": True}

        # When
        attribute = needs_schema.attribute_from_field("contact", field)

        # Then
        self.assertEqual(
            attribute, {"name": "contact", "label": "Contact", "type": "string"}
        )

    def test_integer_boolean_and_array_schemas_map_onto_their_types(self):
        # Given three fields typed by their JSON-Schema fragment
        cases = {
            "integer": "int",
            "number": "int",
            "boolean": "bool",
            "array": "list<string>",
        }

        for json_type, expected in cases.items():
            # When
            attribute = needs_schema.attribute_from_field(
                "value", {"schema": {"type": json_type}}
            )

            # Then
            self.assertEqual(attribute["type"], expected, json_type)

    def test_enum_values_become_strings_even_when_written_as_numbers(self):
        # Given sphinx-needs' `effort` field, whose enum holds integers
        field = {
            "description": "Story points",
            "schema": {"type": "integer", "enum": [1, 2, 3, 5]},
        }

        # When
        attribute = needs_schema.attribute_from_field("effort", field)

        # Then the type is an enum and every value is a string, which is the
        # only spelling our model has
        self.assertEqual(attribute["type"], "enum")
        self.assertEqual(attribute["values"], ["1", "2", "3", "5"])

    def test_an_enum_array_becomes_a_list_of_enum(self):
        # Given
        field = {"schema": {"type": "array", "enum": ["a", "b"]}}

        # When
        attribute = needs_schema.attribute_from_field("kinds", field)

        # Then
        self.assertEqual(attribute["type"], "list<enum>")
        self.assertEqual(attribute["values"], ["a", "b"])

    def test_description_is_preferred_over_a_derived_label(self):
        # Given a field that documents itself
        field = {"description": "Automotive Safety Integrity Level"}

        # When
        attribute = needs_schema.attribute_from_field("asil", field)

        # Then
        self.assertEqual(attribute["label"], "Automotive Safety Integrity Level")


class GlobalFieldsTest(unittest.TestCase):
    def test_sphinx_needs_builtins_are_declared_even_when_the_project_is_not(self):
        # Given a project declaring no fields of its own
        # When
        fields = needs_schema.global_fields({})

        # Then the built-in need options are still there — a `:tags:` in the
        # corpus is sphinx-needs' vocabulary, not an unknown attribute
        self.assertEqual(fields["tags"]["type"], "list<string>")
        self.assertEqual(fields["collapse"]["type"], "bool")

    def test_a_projects_own_declaration_overrides_the_builtin(self):
        # Given a project that types `status` itself
        needs = {"fields": {"status": {"schema": {"type": "boolean"}}}}

        # When
        fields = needs_schema.global_fields(needs)

        # Then
        self.assertEqual(fields["status"]["type"], "bool")

    def test_statuses_table_turns_status_into_an_enum(self):
        # Given a project listing its allowed statuses
        needs = {"statuses": [{"name": "open"}, {"name": "closed"}]}

        # When
        fields = needs_schema.global_fields(needs)

        # Then
        self.assertEqual(fields["status"]["type"], "enum")
        self.assertEqual(fields["status"]["values"], ["open", "closed"])

    def test_id_and_title_are_not_attributes(self):
        # Given a project declaring fields our model handles structurally
        needs = {"fields": {"id": {}, "title": {}, "type": {}, "owner": {}}}

        # When
        fields = needs_schema.global_fields(needs)

        # Then only the ordinary one survives — `id` is the entity id and
        # `title` comes from the directive argument
        self.assertNotIn("id", fields)
        self.assertNotIn("title", fields)
        self.assertNotIn("type", fields)
        self.assertIn("owner", fields)


class RelationsFromLinksTest(unittest.TestCase):
    def test_link_becomes_a_relation_with_a_derived_backlink_option(self):
        # Given one link type with both labels
        needs = {"links": {"reqs": {"outgoing": "specifies", "incoming": "specified by"}}}
        report = []

        # When
        relations = needs_schema.relations_from_links(needs, report)

        # Then the TOML key is the option spelling, the two sphinx-needs
        # strings are labels, and the back-link takes sphinx-needs' own
        # `<name>_back` spelling
        self.assertEqual(
            relations,
            [
                {
                    "name": "reqs",
                    "label": "specifies",
                    "multiple": True,
                    "incoming": "reqs_back",
                    "incoming_label": "specified by",
                }
            ],
        )

    def test_relation_accepts_any_target_type(self):
        # Given a link type — sphinx-needs never scopes one to a target type
        needs = {"links": {"links": {}}}

        # When
        relations = needs_schema.relations_from_links(needs, [])

        # Then `to` is left off, which is how our model spells "any type"
        self.assertNotIn("to", relations[0])

    def test_a_link_computed_by_a_dynamic_function_is_reported(self):
        # Given sphinx-test-reports' auto-linked `runs`
        needs = {"links": {"runs": {"predicates": [["type=='test'", "[[tr_link()]]"]]}}}
        report = []

        # When
        needs_schema.relations_from_links(needs, report)

        # Then the relation is still declared, and the fact that nothing fills
        # it in is reported rather than hidden
        self.assertIn("link predicates", report_categories(report))


class SchemaNarrowingTest(unittest.TestCase):
    def test_unconditionally_required_option_is_narrowed_onto_one_type(self):
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
        required = needs_schema.required_options_by_type(schemas, [])

        # Then
        self.assertEqual(required, {"person": {"role"}})

    def test_narrowing_marks_the_attribute_required_on_that_type_alone(self):
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
        self.assertIn('name = "person"', person)
        self.assertIn('name = "role"', person)
        self.assertIn("required = true", person)
        self.assertIn('name = "team"', team)
        self.assertNotIn("required = true", team)

    def test_a_conditional_rule_is_reported_rather_than_honoured(self):
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
        required = needs_schema.required_options_by_type(schemas, report)

        # Then nothing is narrowed — honouring half of a conditional rule would
        # be worse than not honouring it — and the rule is reported
        self.assertEqual(required, {})
        self.assertIn("conditional schema rule", report_categories(report))

    def test_value_and_network_rules_are_reported(self):
        # Given an id pattern and a cross-entity constraint
        schemas = {
            "schemas": [
                {
                    "id": "req-id-pattern",
                    "select": {"$ref": "#/$defs/type-req"},
                    "validate": {"local": {"properties": {"id": {"pattern": "^R_"}}}},
                },
                {
                    "id": "spec-links-to-req",
                    "select": {"$ref": "#/$defs/type-spec"},
                    "validate": {"network": {"links": {}}},
                },
            ]
        }
        report = []

        # When
        needs_schema.required_options_by_type(schemas, report)

        # Then
        self.assertEqual(
            report_categories(report), {"value schema rule", "network schema rule"}
        )


class OptionNameClashTest(unittest.TestCase):
    def test_a_name_that_is_both_a_field_and_a_link_becomes_the_relation(self):
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
        self.assertEqual(text.count('name = "spec"'), 1)
        block = text[text.index("[[entity_type.relation]]"):]
        self.assertIn('name = "spec"', block)


class RolesTest(unittest.TestCase):
    def test_one_role_per_type_plus_the_sphinx_needs_spelling(self):
        # Given two types
        types = [{"name": "req"}, {"name": "spec"}]

        # When
        declared = needs_schema.roles(types)

        # Then each type gets its own role, and `:need:` resolves to any of them
        self.assertEqual(
            declared,
            [
                {"name": "req", "types": ["req"]},
                {"name": "spec", "types": ["spec"]},
                {"name": "need", "types": ["req", "spec"]},
            ],
        )

    def test_a_type_named_need_is_not_shadowed_by_a_second_need_role(self):
        # Given a project that names a type `need`
        types = [{"name": "need"}]

        # When
        declared = needs_schema.roles(types)

        # Then only one role of that name is emitted — a duplicate is a schema
        # error
        self.assertEqual(declared, [{"name": "need", "types": ["need"]}])


class UnsupportedConstructsTest(unittest.TestCase):
    def test_constraints_variants_and_global_options_are_reported(self):
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
        self.assertLessEqual(
            {"constraints", "variants", "global options"}, report_categories(report)
        )

    def test_the_generic_need_directive_is_reported(self):
        # Given a configuration that does not declare `need` as a type of its
        # own — `.. need::` then names its type in an option, which
        # one-directive-per-type cannot express
        # When
        report = needs_schema.unsupported_constructs({"types": [{"directive": "req"}]})

        # Then
        self.assertIn("generic need directive", report_categories(report))

    def test_a_project_declaring_need_as_a_type_is_using_the_spelling(self):
        # Given a project whose own vocabulary includes a type called `need`
        # When
        report = needs_schema.unsupported_constructs({"types": [{"directive": "need"}]})

        # Then nothing is reported — it is an ordinary type, not the built-in
        self.assertNotIn("generic need directive", report_categories(report))


class TomlRenderingTest(unittest.TestCase):
    def test_strings_bools_and_lists_render_as_toml(self):
        # Given the three value shapes the schema uses
        # When / Then
        self.assertEqual(needs_schema.toml_value("a\"b"), '"a\\"b"')
        self.assertEqual(needs_schema.toml_value(True), "true")
        self.assertEqual(needs_schema.toml_value(False), "false")
        self.assertEqual(needs_schema.toml_value(["a", "b"]), '["a", "b"]')

    def test_the_generated_file_announces_that_it_is_generated(self):
        # Given any conversion
        # When
        text, _report = convert({"types": [{"directive": "req"}]})

        # Then a reader who opens it in the cloned corpus is told not to edit it
        self.assertIn("GENERATED by scripts/needs_schema.py", text)


if __name__ == "__main__":
    unittest.main()


class ImportKeysTest(unittest.TestCase):
    """`[needs.import_keys]` maps a name a `.. needimport::` may write onto the
    file it stands for. This build reads the same table from the entity schema,
    so the conversion is a straight copy."""

    def test_import_keys_are_carried_over_verbatim(self):
        # Given the table the sphinx-needs demo declares
        needs = {"import_keys": {"imported_project": "/needs_import.json"}}

        # When it is converted
        text, _report = convert(needs)

        # Then the value crosses unchanged. sphinx-needs writes it
        # source-root-relative with a leading `/`, which is exactly how this
        # build resolves a path in an entity schema, so there is nothing to
        # translate.
        self.assertIn("[import_keys]", text)
        self.assertIn('imported_project = "/needs_import.json"', text)

    def test_the_table_is_emitted_before_the_first_entity_type(self):
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
        self.assertLess(text.index("[import_keys]"), text.index("[[entity_type]]"))

    def test_a_project_without_import_keys_emits_no_table(self):
        # Given a project that declares none
        needs = {"types": [{"directive": "req", "title": "Requirement"}]}

        # When it is converted
        text, _report = convert(needs)

        # Then nothing is emitted, rather than an empty table
        self.assertNotIn("[import_keys]", text)

    def test_import_keys_are_not_reported_as_unconvertible(self):
        # Given a project declaring an import key
        needs = {"import_keys": {"upstream": "/u.json"}}

        # When it is converted
        _text, report = convert(needs)

        # Then it is absent from the report: the construct is supported, and
        # listing it would overstate what a migrating project loses.
        self.assertNotIn("import keys", report_categories(report))

    def test_import_keys_reads_the_table_on_its_own(self):
        # Given the table, and a project without one
        # When each is read
        # Then the helper is a plain copy, and missing means empty
        self.assertEqual(
            needs_schema.import_keys({"import_keys": {"a": "/a.json"}}),
            {"a": "/a.json"},
        )
        self.assertEqual(needs_schema.import_keys({}), {})

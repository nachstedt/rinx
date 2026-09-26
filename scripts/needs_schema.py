"""Convert a sphinx-needs configuration into a rinx entity schema.

sphinx-needs projects that use ubCode keep their configuration in a declarative
`ubproject.toml` (announced to Sphinx by `needs_from_toml`), optionally with a
`schemas.json` holding JSON-Schema validation rules. Both are data, so the
vocabulary they declare — need types, custom fields, link types — converts into
an `entities.toml` mechanically. `scripts/benchmark_entities.py` runs this over
the useblocks demo corpus so the benchmark builds a real sphinx-needs project
against our own entity model.

The conversion is deliberately lossy in one direction only: anything with no
equivalent in our model is collected into a *report* rather than dropped, so the
benchmark can print "here is what sphinx-needs expresses that we do not". That
list is as much the point of this file as the schema it emits.

Everything here is pure — dicts in, `(text, report)` out. Reading and writing
files is the caller's job.
"""

import json
import re

# sphinx-needs' own built-in need options. A project declares only its *extra*
# fields in `[needs.fields]`, so without these every `:tags:` or `:collapse:` in
# the corpus would be reported as an unknown attribute — noise about
# sphinx-needs' vocabulary rather than about ours. A project's own declaration
# of the same name wins over the entry here.
BUILTIN_FIELDS = {
    "status": "string",
    "tags": "list<string>",
    "collapse": "bool",
    "hide": "bool",
    "delete": "bool",
    "duration": "string",
    "completion": "string",
    "constraints": "list<string>",
    "layout": "string",
    "style": "string",
    "template": "string",
    "pre_template": "string",
    "post_template": "string",
}

# Options sphinx-needs treats specially and our model does not express as an
# attribute at all: `id` *is* the entity id, and `title` is filled from the
# directive argument.
RESERVED_FIELDS = {"id", "title", "type"}

# `[needs.links.<name>]` declares the outgoing option only; sphinx-needs derives
# the back-link option as `<name>_back`. We declare that spelling explicitly, so
# a document writing `:links_back:` resolves rather than warning.
BACKLINK_SUFFIX = "_back"


def convert(ubproject, schemas=None):
    """Convert a parsed `ubproject.toml` (+ optional `schemas.json`) into an
    `entities.toml` text and a report of what could not be carried over.

    Returns `(toml_text, report)`, where `report` is a list of
    `(category, detail)` pairs."""
    needs = ubproject.get("needs", {})
    report = []

    constraints = constraints_by_type(schemas or {}, report)
    fields = global_fields(needs)
    relations = relations_from_links(needs, report)

    types = []
    for raw_type in needs.get("types", []):
        types.append(
            entity_type(raw_type, needs, fields, relations, constraints, report)
        )

    report.extend(unsupported_constructs(needs))
    return render_schema(types, roles(types), import_keys(needs)), report


# ── Entity types ──────────────────────────────────────────────────────────────

def entity_type(raw_type, needs, fields, relations, constraints, report):
    """Build one entity type from a `[[needs.types]]` entry.

    sphinx-needs scopes neither fields nor links to a type, so every type gets
    every one of them; `schemas.json` is the only thing that can narrow a name
    down to one type — `required`, a pattern, or an enum on it alone. A
    narrowing naming something this type cannot carry it on is reported."""
    name = raw_type["directive"]
    here = constraints.get(name) or new_constraints()
    required_here = here["required"]

    attributes = [{"name": "title", "label": "Title", "type": "string"}]
    for field_name, attribute in fields.items():
        attribute = dict(attribute)
        if field_name in required_here:
            attribute["required"] = True
        attributes.append(attribute)

    declared = {a["name"] for a in attributes}
    scoped_relations = []
    for relation in relations:
        # A name cannot be an attribute and a relation on the same type; the
        # relation wins, since a link carries a reference rather than a value.
        if relation["name"] in declared:
            attributes = [a for a in attributes if a["name"] != relation["name"]]
        relation = dict(relation)
        if relation["name"] in required_here:
            relation["required"] = True
        scoped_relations.append(relation)

    attributes = [
        narrow_attribute(attribute, here, report) for attribute in attributes
    ]
    relation_names = {relation["name"] for relation in scoped_relations}
    attribute_names = {attribute["name"] for attribute in attributes}
    for field_name, rule_id in sorted(here["sources"].items()):
        if field_name in attribute_names:
            continue
        if field_name in relation_names:
            report.append(
                (
                    "value schema rule",
                    f"{rule_id}: constrains the value of link `{field_name}`, "
                    "whose values are entity ids rather than text",
                )
            )
        else:
            report.append(
                (
                    "value schema rule",
                    f"{rule_id}: constrains `{field_name}`, which `{name}` "
                    "does not declare",
                )
            )

    entity = {
        "name": name,
        "label": raw_type.get("title", name),
        "argument": {"fields": ["title"]},
        "attribute": attributes,
        "relation": scoped_relations,
    }
    id_spec = {}
    if raw_type.get("prefix"):
        id_spec["prefix"] = raw_type["prefix"]
    if needs.get("id_required"):
        id_spec["required"] = True
    if here["id_pattern"]:
        id_spec["pattern"] = here["id_pattern"]
    if id_spec:
        entity["id"] = id_spec
    return entity


def narrow_attribute(attribute, constraints, report):
    """Apply one type's value narrowings from `schemas.json` to an attribute.

    Returns the attribute, marked or retyped as the rules say. A pattern goes
    only onto a `string`, the one type both models read as a single piece of
    text; an enum turns a `string` into an `enum`, or narrows an existing one.
    Anything else is reported rather than carried over half-honoured."""
    name = attribute["name"]
    pattern = constraints["patterns"].get(name)
    values = constraints["enums"].get(name)
    if pattern is None and values is None:
        return attribute
    rule_id = constraints["sources"][name]
    attribute = dict(attribute)
    if pattern is not None:
        if attribute["type"] == "string":
            attribute["pattern"] = pattern
        else:
            report.append(
                (
                    "value schema rule",
                    f"{rule_id}: a pattern on `{name}`, whose type is "
                    f"`{attribute['type']}` rather than text",
                )
            )
    if values is not None:
        if attribute["type"] in ("string", "enum"):
            attribute["type"] = "enum"
            attribute["values"] = values
        else:
            report.append(
                (
                    "value schema rule",
                    f"{rule_id}: an enum on `{name}`, whose type is "
                    f"`{attribute['type']}`",
                )
            )
    return attribute


def global_fields(needs):
    """The attribute declaration for every option a need may carry: the
    sphinx-needs built-ins, overridden by the project's own `[needs.fields]`."""
    fields = {}
    for name, value_type in BUILTIN_FIELDS.items():
        fields[name] = {"name": name, "label": label_for(name), "type": value_type}

    for name, field in (needs.get("fields") or {}).items():
        if name in RESERVED_FIELDS:
            continue
        fields[name] = attribute_from_field(name, field)

    statuses = needs.get("statuses")
    if statuses and "status" in fields:
        fields["status"] = {
            "name": "status",
            "label": "Status",
            "type": "enum",
            "values": [s["name"] for s in statuses],
        }
    return fields


def attribute_from_field(name, field):
    """Map one `[needs.fields.<name>]` entry onto an attribute declaration.

    The value type comes from the entry's JSON-Schema fragment, which is the
    only place sphinx-needs states one; a field without a `schema` is an
    untyped string."""
    schema = field.get("schema") or {}
    enum = schema.get("enum")
    json_type = schema.get("type")

    attribute = {"name": name, "label": field.get("description") or label_for(name)}
    if enum is not None:
        attribute["type"] = "list<enum>" if json_type == "array" else "enum"
        # An enum's values are strings in our model even when sphinx-needs
        # writes them as numbers (`effort = [1, 2, 3, 5, ...]`).
        attribute["values"] = [str(value) for value in enum]
    elif json_type in ("integer", "number"):
        attribute["type"] = "int"
    elif json_type == "boolean":
        attribute["type"] = "bool"
    elif json_type == "array":
        attribute["type"] = "list<string>"
    else:
        attribute["type"] = "string"
    return attribute


def relations_from_links(needs, report):
    """Map `[needs.links.<name>]` onto relation declarations.

    The TOML key is the option spelling; `outgoing`/`incoming` are *labels*, and
    the incoming option's own spelling is sphinx-needs' derived `<name>_back`.
    `to` is left off deliberately — sphinx-needs links are not scoped to a
    target type, and omitting it accepts any."""
    relations = []
    for name, link in (needs.get("links") or {}).items():
        relations.append(
            {
                "name": name,
                "label": link.get("outgoing", name),
                "multiple": True,
                "incoming": name + BACKLINK_SUFFIX,
                "incoming_label": link.get("incoming", name + BACKLINK_SUFFIX),
            }
        )
        if link.get("predicates"):
            report.append(
                (
                    "link predicates",
                    f"[needs.links.{name}] computes targets with a dynamic "
                    f"function; the link is declared, but nothing fills it in",
                )
            )
        if link.get("copy"):
            report.append(
                ("link copy", f"[needs.links.{name}] sets copy = true")
            )
    return relations


def roles(types):
    """One role per entity type, plus sphinx-needs' own `:need:` spelling over
    all of them. Roles are sugar — every entity is a `:ref:` target regardless —
    so the point of declaring them is the type check and the spelling a
    migrating project already writes."""
    names = [entity["name"] for entity in types]
    declared = [{"name": name, "types": [name]} for name in names]
    if "need" not in names:
        declared.append({"name": "need", "types": list(names)})
    return declared


# ── schemas.json ──────────────────────────────────────────────────────────────

TYPE_REF = re.compile(r"^#/\$defs/type-(?P<name>[A-Za-z0-9_]+)$")


def new_constraints():
    """What `schemas.json` can narrow onto one need type."""
    return {
        # Option names that must be given.
        "required": set(),
        # The pattern every id of the type must match.
        "id_pattern": None,
        # Option name -> the pattern its value must match.
        "patterns": {},
        # Option name -> the values it is restricted to.
        "enums": {},
        # Option name -> the rule that constrained its value, for the report
        # when the type turns out to have nothing to put the constraint on.
        "sources": {},
    }


def constraints_by_type(schemas, report):
    """Read `schemas.json` for the rules our model can express: those selecting
    exactly one need type, requiring an option on it or constraining an
    option's value.

    Every other rule — a conditional `allOf` select, a `network` (cross-entity)
    constraint — lands in the report instead. Our schema has no vocabulary for
    them, and silently honouring the `required` half of a conditional rule
    would be worse than not honouring it at all."""
    by_type = {}
    for rule in schemas.get("schemas", []):
        rule_id = rule.get("id", "<unnamed>")
        ref = (rule.get("select") or {}).get("$ref")
        match = TYPE_REF.match(ref) if isinstance(ref, str) else None
        local = (rule.get("validate") or {}).get("local") or {}
        network = (rule.get("validate") or {}).get("network")

        if match is None:
            report.append(
                ("conditional schema rule", f"{rule_id}: selects on a condition, not a type")
            )
            continue
        if network:
            report.append(
                ("network schema rule", f"{rule_id}: constrains linked entities")
            )
        constraints = by_type.setdefault(match.group("name"), new_constraints())
        for name, fragment in (local.get("properties") or {}).items():
            translate_property(rule_id, name, fragment, constraints, report)
        for name in local.get("required", []):
            constraints["required"].add(name)
    return by_type


# JSON-Schema keywords whose meaning the entity model already enforces, and so
# need nothing emitted: a field's declared type already fixes `type`, and a
# value of `""` or `[]` is treated as unset — in sphinx-needs' own validation
# as much as here, since its `reduce_need` strips both before checking — so a
# minimum of one is either implied by `required` or has no effect without it.
ALREADY_ENFORCED = {
    "type": ("string", "array"),
    "minLength": (1,),
    "minItems": (1,),
}


def translate_property(rule_id, name, fragment, constraints, report):
    """Carry one `properties.<name>` fragment of a type-scoped rule over onto
    that type's constraints, reporting each keyword that has no equivalent.

    `{}` constrains nothing and yields nothing. The report names the keyword
    rather than the rule alone, so it says what is actually missing."""
    if not isinstance(fragment, dict):
        report.append(
            ("value schema rule", f"{rule_id}: `{name}` is not a JSON-Schema object")
        )
        return
    leftover = {}
    for keyword, value in fragment.items():
        if value in ALREADY_ENFORCED.get(keyword, ()):
            continue
        if keyword == "pattern" and name == "id":
            if constraints["id_pattern"] not in (None, value):
                report.append(
                    (
                        "value schema rule",
                        f"{rule_id}: a second id pattern for one type; "
                        f"`{constraints['id_pattern']}` is kept",
                    )
                )
            else:
                constraints["id_pattern"] = value
        elif keyword == "pattern":
            constraints["patterns"][name] = value
            constraints["sources"][name] = rule_id
        elif keyword == "enum" and name != "id":
            constraints["enums"][name] = [str(item) for item in value]
            constraints["sources"][name] = rule_id
        else:
            leftover[keyword] = value
    for keyword, value in leftover.items():
        report.append(
            (
                "value schema rule",
                f"{rule_id}: `{name}` uses `{keyword}: {json.dumps(value)}`, "
                "which the entity model cannot express",
            )
        )


# ── Constructs with no equivalent ─────────────────────────────────────────────

def import_keys(needs):
    """The `[needs.import_keys]` table: names a `.. needimport::` may write
    instead of a path.

    Carried over verbatim, values included. sphinx-needs writes them
    source-root-relative with a leading `/`, which is exactly how this build
    resolves a path in an entity schema, so there is nothing to translate — and
    nothing to report as unconvertible either."""
    return dict(needs.get("import_keys") or {})


def unsupported_constructs(needs):
    """Everything in the sphinx-needs configuration our model cannot express.

    This is the honest half of the conversion: each entry is a question about
    the entity model, not a defect in the corpus."""
    report = []
    if needs.get("constraints"):
        report.append(
            (
                "constraints",
                f"{len(needs['constraints'])} [needs.constraints] rule(s): "
                "value constraints evaluated at build time",
            )
        )
    if needs.get("variants") or needs.get("variant_data_file"):
        report.append(
            ("variants", "needs variants select option values per build configuration")
        )
    for name, field in (needs.get("fields") or {}).items():
        if field.get("parse_variants"):
            report.append(("variants", f"[needs.fields.{name}] parses variant syntax"))
    if needs.get("global_options"):
        report.append(
            ("global options", "[needs.global_options] assigns defaults by filter")
        )
    declared = {raw_type.get("directive") for raw_type in needs.get("types", [])}
    if "need" not in declared:
        # sphinx-needs' built-in `.. need::` selects its type in a `:type:`
        # option. A project that declares `need` as a type of its own is using
        # the spelling, not the mechanism, so reporting it there would be noise.
        report.append(
            (
                "generic need directive",
                "sphinx-needs' `.. need::` names its type in a `:type:` option; "
                "our model has one directive per type",
            )
        )
    return report


# ── TOML rendering ────────────────────────────────────────────────────────────

def render_schema(types, declared_roles, keys=None):
    """Render the converted model as `entities.toml` text.

    Written by hand rather than through a TOML library so the benchmark keeps
    its zero-dependency `py_binary` — the shape emitted here is a fixed handful
    of tables, and `Cargo.bazel.lock`-style repinning for a formatting
    convenience is not worth it.

    `keys` is the `[import_keys]` table, which defaults to none so the existing
    callers that render a schema alone keep working."""
    lines = [
        "#:schema ../../schemas/entities.schema.json",
        "#",
        "# GENERATED by scripts/needs_schema.py from a sphinx-needs ubproject.toml.",
        "# Edits are lost on the next `bazel run //scripts:benchmark_entities`.",
        "",
    ]
    if keys:
        # Emitted before the first `[[entity_type]]`. A top-level table written
        # after an array of tables is legal TOML but reads as if it were nested,
        # and one written between `[[entity_type]]` and
        # `[[entity_type.attribute]]` would break the nesting outright.
        lines.append("# Files a `.. needimport::` may name instead of a path.")
        lines.append("[import_keys]")
        for alias, path in sorted(keys.items()):
            lines.append(f"{alias} = {toml_value(path)}")
        lines.append("")
    for entity in types:
        lines.append("[[entity_type]]")
        lines.append(f"name = {toml_value(entity['name'])}")
        lines.append(f"label = {toml_value(entity['label'])}")
        lines.append(f"argument = {inline_table(entity['argument'])}")
        if "id" in entity:
            lines.append(f"id = {inline_table(entity['id'])}")
        lines.append("")
        for attribute in entity["attribute"]:
            lines.append("[[entity_type.attribute]]")
            lines.extend(table_body(attribute))
            lines.append("")
        for relation in entity["relation"]:
            lines.append("[[entity_type.relation]]")
            lines.extend(table_body(relation))
            lines.append("")

    for role in declared_roles:
        lines.append("[[role]]")
        lines.extend(table_body(role))
        lines.append("")

    return "\n".join(lines)


def table_body(table):
    """`key = value` lines for one table, in declaration order."""
    return [f"{key} = {toml_value(value)}" for key, value in table.items()]


def inline_table(table):
    """One `{ key = value, ... }` inline table."""
    body = ", ".join(f"{key} = {toml_value(value)}" for key, value in table.items())
    return "{ " + body + " }"


def toml_value(value):
    """Render a string, bool or list of strings as TOML.

    JSON's string and array syntax is a subset of TOML's for these shapes, so
    `json.dumps` escapes correctly."""
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, list):
        return "[" + ", ".join(toml_value(item) for item in value) + "]"
    return json.dumps(value)


def label_for(name):
    """A human-readable heading for an option that declares no description."""
    return name.replace("_", " ").capitalize()

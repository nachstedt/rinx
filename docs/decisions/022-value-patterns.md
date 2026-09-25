# 22. Value patterns: a naming convention for ids and text attributes

## Status

Accepted.

## Context

The entity benchmark (`bazel run //scripts:benchmark_entities`) converts
useblocks' sphinx-needs demo into an entity schema. It reported 15 of the
demo's `schemas.json` rules as "value schema rule: constrains option values
(pattern/enum)". The converter put every rule with `validate.local.properties`
under that label, whatever those properties held. Read one by one, they fell
into three groups:

- **Eleven id patterns** (`req-id-pattern`, `hazard-id-pattern`, …), each of
  the form `{"properties": {"id": {"pattern": "^HAZ_[A-Z0-9_]+$"}}}`. The
  entity model had nothing to express these with. This was the real gap.
- **Three minimums of one** — `person-has-role` (`minLength: 1`),
  `team-has-persons` and `impl-has-implements-links` (`minItems: 1`). The model
  already enforces these. sphinx-needs' own validation (`reduce_need` in
  `sphinx_needs/schema/core.py`) strips an unset field and an empty link list
  before checking. So a minimum of one either repeats `required` or, without
  `required`, has no effect. Here, `""` is a missing value and `[]` is no
  target, which is the same thing.
- **One empty constraint** — `test-has-spec-or-impl`, whose properties are all
  `{}`. It accepts anything, so it constrains nothing.

So the decision was only about the first group, and about carrying the other
two over without reporting them as missing.

## Decision

### 1. `pattern`, on `id` and on the three text attribute types

`[entity_type] id = { pattern = "…" }` constrains a type's ids.
`[[entity_type.attribute]] pattern = "…"` constrains a `string`, `text` or
`list<string>` value; for the list, each item must match. A pattern on any
other type is refused when the schema loads (`SchemaError::PatternOnNonTextType`).
This follows the existing rule for `values`, which is refused on a non-enum
type: both keys look like they constrain the value, and on those types nothing
would ever check them.

The name is JSON Schema's. So are the semantics: the expression is *searched
for* (`Regex::is_match`), not matched against the whole value. The corpus'
rules are already written `^…$`, so a migrated rule keeps its meaning without
being rewritten.

### 2. `ValuePattern`: compiled once, re-validated on load

`rinx_entity::ValuePattern` is the parse-don't-validate type for this.
The only way to build one is `ValuePattern::new`, which compiles the
expression. It serializes as its source text, and its `Deserialize` compiles
again, as `HashedContent` re-hashes. Equality is on the source. Because it
serializes as its source, a pattern enters the schema fingerprint with no extra
code. That is correct: the parser applies the pattern, so changing it must
re-parse, and a library and a site that disagree about it must be reported as
`entity.schema-mismatch`.

The dialect is the `regex` crate's, not ECMA-262. It covers everything a naming
convention uses (anchors, classes, alternation, counted repetition), but not
lookaround or backreferences. A pattern using either fails to compile. That is
a load-time `SchemaError::InvalidPattern` naming the type and what declared the
pattern, never a pattern that silently matches nothing. None of the demo's
eleven needs either. `fancy-regex` would close the gap at the cost of
backtracking in the parse action; that cost can wait for a project that
actually needs it.

### 3. On an attribute, the pattern is part of the type

The pattern is carried in the variant, as `AttributeType::String { pattern }`,
`Text { pattern }` and `StringList { pattern }`, rather than as a field on
`AttributeSchema`. `AttributeType` already documents the rule this follows:
each kind carries only the data it needs. A pattern on an `int` then cannot be
represented at all, and the loader check in §1 is the only place that has to
think about it.

The payoff is where the check runs. Every attribute value in the build,
whether written as an option, imported by `.. needimport::` or set by
`.. entity-update::`, goes through `parse_attribute_value`. That function
already refuses a value outside an enum, and it now refuses a pattern mismatch
(`AttributeParseError::PatternMismatch`) the same way. No caller changed, so
the three routes cannot disagree. A mismatch is `entity.invalid-attribute-value`
and the value is left unset, exactly as for an enum. The value does not fit
its type.

### 4. On an id, a separate check with its own code, and the id is kept

An id is not a value that can be left unset. `entity.invalid-id` replaces an
unusable id with a generated fallback, and doing that here would break every
link to an id that is perfectly legal. So `IdSpec::check_pattern` runs *after*
the id is determined. Its mismatch is `entity.id-pattern-mismatch`, and the
entity keeps its id. The fix is a rename only the author can make.

It has its own code rather than sharing `entity.invalid-id` because the two
are different findings. One is a broken identity; the other is a broken
convention. A project migrating a large corpus may reasonably want to silence
the convention on its own with a `.. noqa:`. The demo marks most of these rules
`severity: info`, and this build has no severities, so the code is the only
thing that can separate them.

The check applies to the **final** id, however it was determined: explicit,
composed with `from`, generated, or imported, prefix included. That matches
sphinx-needs, which validates the need as it ends up. A generated id is a hash
and will rarely match a convention, so `docs/entities.md` advises
`required = true` beside a pattern. The loader does not require it, because a
pattern loose enough to accept a generated id is legitimate.

The check itself is `entity_fields.rs::report_id_pattern`, called from both
`entity.rs` and `needimport.rs`. That module exists so a written and an
imported entity cannot validate differently. A written entity's mismatch lands
on its `:id:` line when there is one, and on the directive otherwise. An
imported one lands on the `.. needimport::` line, like every import finding,
and the message names the id. `.. entity-update::` cannot change an id, so it
needs no check.

### 5. The converter translates per property, and reports by keyword

`scripts/needs_schema.py` now reads each `properties.<name>` fragment of a
type-scoped rule one keyword at a time:

- `pattern` on `id` becomes `id.pattern`.
- `pattern` on a `string` field becomes that attribute's `pattern`.
- `enum` on a `string` field turns it into an `enum`, and on an `enum` field
  narrows its values, for the selected type alone.
- `type: string|array`, `minLength: 1`, `minItems: 1` and `{}` need nothing
  emitted, for the reasons in Context.

Anything else is reported under "value schema rule", naming the property and
the keyword (``role` uses `minLength: 3``) rather than the rule alone. A
narrowing that lands on nothing it can apply to is reported with the reason:
a link, a non-text field, or a field the type does not declare. So is a second,
different id pattern for one type.

## Consequences

- On the demo corpus the "value schema rule" section is empty. The eleven id
  patterns are enforced, and any need breaking one now shows up as
  `entity.id-pattern-mismatch` in the benchmark's warnings. That is the rules
  working, not a regression.
- Every schema's fingerprint changes once, because `AttributeType`'s
  serialized shape now carries `pattern`. That costs one full re-parse, as any
  schema edit does.
- Constructing a plain text type now spells its absent pattern.
  `AttributeType::string()`, `text()` and `string_list()` keep that short.

## Not done

- **Conditional rules** (`select` with an `allOf` over a type *and* a
  document path) are still reported. They need a selector vocabulary, not a
  value constraint. The demo's two enums live only inside such rules.
- **Network rules** (`validate.network`, "every `reqs` target is a `req`")
  are still reported, although `relation.to` already covers the simple form.
  Mapping them is a converter change for a later increment.
- **Other JSON-Schema keywords** — `const`, `maxLength`, `maxItems`,
  `minimum` and so on — are reported by name. None appears in the corpus.

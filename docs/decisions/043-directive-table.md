# 43. The directives are a table, and one reader splits every block

## Status

Accepted.

## Context

Roadmap #25 is directive and option completion in the editor. Its known risk
was that no table of directives existed: the dispatcher was a chain of
`if name == …` arms, the names an entity schema may not take were a second
hand-kept list beside it (`BUILTIN_DIRECTIVE_NAMES`), and each of ~36
directive parsers split its own block into argument, options and content —
`unindent_body_lines`, `scan_option_lines`, `ctx.nested(body_start, 0)` —
with the reStructuredText surface (the marker line, indentation) spread across
all of them. A completion list built from a second copy would drift from what
the parser accepts, and MyST, a planned second syntax, would have had to
re-implement every directive.

## Decision

### The spec is data, beside its parser

Each directive module holds a `DirectiveSpec` (`toctree.rs`'s `SPEC`, …):
its names and purpose, the arguments it takes, whether it takes content, and
its options — whether it takes any is derived from them. It is built from the leaf
`spec/vocabulary.rs` alone and holds no behaviour, so any reader of any
syntax, completion and documentation can share it.

### A registry pairs each spec with its interpreter

`spec/registry.rs`'s `DIRECTIVES` lists one entry per directive: its spec and
its `interpret_*`, of one shared signature over a `DirectiveInvocation`. The
dispatcher finds a written name's entry and calls its interpreter. A directive
is identified by its spec, so there is no enum of directive kinds to keep in
step with the names. A module whose spec and interpreter are missing from the
list is dead code, which `cargo clippy` refuses — the completeness the
exhaustive `match` of an earlier draft gave. `is_builtin_directive_name` is
derived from the list plus two tables of the vocabularies resolved against the
default domain: `OBJECT_DIRECTIVES` and `SCOPE_DIRECTIVES`.

### Reading the syntax is one reader; interpreting is one function per directive

`directives/reader.rs` reads a reStructuredText block into an invocation by
docutils' own rule (`RSTState.parse_directive_block`,
`parse_directive_arguments`, docutils 0.21.2): the text after the `::` and the
indented lines below it form the block; when the directive takes arguments or
options, the argument block runs to the first blank line and its options, from
the first field marker on, are taken off its end; a directive taking no
argument reads the text left there as the start of its content; the argument
block splits on whitespace, the last argument keeping the rest under
`final_argument_whitespace`. The block's shared indentation is stripped, as
docutils strips the smallest one. Every interpreter reads the invocation, never
the source, so a MyST reader producing the same invocation reuses every one.

What were special cases are instances of the rule: content on an admonition's
marker line, the generic admonition's title, a version change's second
argument, a domain object's signatures continued below the marker — Sphinx's
`ObjectDescription` takes one argument keeping the rest, split into one
signature per line — and an `.. index::`'s entries.

### The shapes are transcribed

Each shape is read from the directive's class in docutils 0.21.2, Sphinx 9.0.4,
sphinx-design 0.7.0, sphinx-needs 9.1.0, sphinxcontrib-plantuml 0.31 or
sphinx-simplepdf 1.7.0 (`required_arguments`, `optional_arguments`,
`final_argument_whitespace`, `option_spec`, `has_content`). Where this build
accepts something else, the spec states what it accepts: `.. entity-table::`
and `.. entity-flow::` take no argument, `.. entity-flow::` and
`.. entity-sequence::` no content, `.. entity-update::` content sphinx-needs'
`needextend` does not, and `.. role::` reads its role as an argument where
docutils reads the content's first line — which splits a block identically.

### A shape error is reported, and the directive read anyway

Where docutils discards a directive — too few arguments, surplus words without
`final_argument_whitespace`, content for a directive taking none — this build
reports it under one of three shared codes, `directive.missing-argument`,
`directive.too-many-arguments` and `directive.content-not-permitted`, and reads
the directive from what was written: a missing argument is absent, surplus
words are ignored, refused content is not interpreted. The codes each
directive had for the same mistake are removed rather than aliased
(`directive.version-argument-missing`, `directive.title-argument-missing`,
`directive.namespace-push-argument-missing`, `image.missing-uri`,
`image.content-not-allowed`, `sectnum.unexpected-argument`,
`include.missing-path`, `needimport.missing-path`,
`button-link.missing-target`, `if-builder.missing-builder`,
`code-block.empty-language`, `default-role.unexpected-content`), since the
mistake is the same whichever directive it is written in. A directive refusing
content may still say what to write instead: the shape's `content_hint`,
appended to the refusal (`.. image::` points at `.. figure::`,
`.. entity-flow::` at `:filter:` and `.. entity-diagram::`).

One leniency is this build's own: text directly below the options, before a
blank line, starts the content, where docutils refuses the block as an invalid
option block.

### Options are an enum per directive, generated from one table

Each directive declares its options with `directive_options!`
(`directives/options.rs`), one row per option: its variant, its spellings —
the canonical one first, an alias beside it (`link_types`, `link-types`) — what
its value is (`OptionValue`: a flag, text, a length, a choice computed from the
enum the value is read into, …) and what it does in one sentence. The macro
generates the enum, its lookup by spelling, and the `OptionSpec`s the spec's
`options` field holds. A shared set — the five table presentation options, the
code blocks' common ones, a picture's, the chart figure options, the
object-description flags, sphinx-needs' shorthand filters — is an enum of its
own, included as a wrapped variant (`Table(TableOption)`), so one reader
handles it for every directive taking it.

`classify_options` turns the invocation's option lines into the enum, and the
parser matches on it with no `_` arm: an option listed and not read, or read
and not listed, does not compile, and completion cannot offer an option the
parser does not take.

An option the reference implementation takes and this build does not is a row
too, written `_ => (["name"], Value, "doc", advice)`: it has no variant, so no
parser reads it, and `classify_options` reports it as
`directive.unsupported-option` with the advice — what to write instead — while
completion still knows the name. Every other option is
`directive.unknown-option`. Both are one code whichever directive it is
written in, for the reason the argument codes are: the mistake is the same.
The per-directive codes are removed rather than aliased
(`directive.toctree-unknown-option` and eleven siblings, `include.`,
`doctest.`, `uml.` and `needimport.unknown-option`,
`doctest.option-not-supported`, the five `*.unsupported-option` codes,
`uml.save-unsupported`, `uml.option-needs-template`), superseding what
ADR-012, ADR-014, ADR-016, ADR-018, ADR-020 and ADR-021 say about them.

The option sets are transcribed from the same classes as the shapes. Where one
spelling serves several directives with different options, the spec is split:
`.. code-block::` and `.. code::`, the plain and the templated diagrams, and
the doctest directives (`doctest`/`testoutput`, `testcode`,
`testsetup`/`testcleanup`) each have one per option set.

### A row names the type its value is read into

A row may name a type, and its variant then carries the value, parsed:
`Align(ImageAlign) => (["align"], EntityFlowInvalidAlign, "Where the chart
sits.")`. The type implements `OptionValueType` (`directives/option_value.rs`):
it reads a written value or says why it is none, and it says what completion
offers, so what is offered and what is accepted cannot drift apart — before,
a row's `Choice` list and the arm's `ImageAlign::parse` were two statements of
it, and disagreed (six values offered, three accepted). `classify_options`
reads each value and reports one that does not read on its line, under the
row's code or, when the row names none, the shared
`directive.invalid-option-value`, in one shape: `{directive}: :{name}:
{problem}`. The interpreter still receives one `(option, line)` pair per line
written and keeps its exhaustive `match`; a bad value never reaches it.

A value that does not read and was not written at all is a missing value,
reported as `directive.empty-option-value` whichever option it is: one
mistake, one code. That replaces sixteen codes of their own
(`toctree.empty-name`, `math.empty-label`, `sectnum.empty-prefix`, the eight
`*.empty-option-value`, …), and lets an option whose emptiness used to have a
code apart from its other failures — grid's `:margin:`, dropdown's `:icon:` —
be typed like any other.

Two mistakes of the option block are reported centrally too, and the line is
still handed over, so what it means stays the directive's to decide:

- an option written again under the same spelling is
  `directive.duplicate-option`, which docutils refuses outright. Two spellings
  of one option are no repeat: docutils keys options by spelling, and CPython
  writes `:noindex:` and `:no-index:` together on purpose, for two Sphinx
  versions;
- a flag written with a value is `directive.flag-with-value`, which docutils'
  `directives.flag` refuses; the flag still applies.

The value types are written once and shared: text, counts, a `DepthLimit`
whose zero means unlimited (`:maxdepth:`, both `:depth:`s), a non-empty text
or label, classes, a percentage, a length, a boolean, every choice enum the
AST already has, and `Located<T>` for a value checked later against what only
the content says — `:widths:` against the column count, now reported on the
`:widths:` line, and `:emphasize-lines:` against the block's lines.

A value is still read by its directive, from the line handed over with a
variant that carries none, where one type cannot say what is valid:

- the valid values depend on where the directive stands (an image's `:align:`
  inside a substitution, a role's `:language:` on its base);
- one option has two failure codes for two written values (the CSV dialect
  characters: not a character, and not ASCII);
- the reading needs the schema, reports several things, or keeps the text for
  a later step (filters, relations, columns, `:labels:`, `:extra:`, doctest's
  `:options:`, a `:caption:` whose bare form `.. literalinclude::` reads);
- a shared group answers to its owner's codes (the chart figure options).

### A spec says when it takes options beyond the listed ones

`DirectiveSpec::accepts_further` marks a directive whose interpreter reads
option lines beyond the ones it lists: `.. entity-update::` lists `:strict:`
and reads every other line as a field mutation checked against the schema,
and `.. needservice::` takes whatever options the external service defines.
A spec takes options exactly when it declares some or accepts further ones
(`DirectiveSpec::takes_options`), so the two cannot disagree; a registry test
checks that only those two accept further ones.

### Every directive has a spec

The directives whose name is not fixed have specs too, so the reader splits
every block by a spec, and completion lists every directive from one kind of
record:

- **A domain object** has a static spec per object type, beside its parser
  (`py/function.rs`'s `FUNCTION_SPEC`, …), its options the type's option enum.
  Its names are the bare spellings in its domain (`function`; `member` and
  `var`): `OBJECT_DIRECTIVES` pairs each object type with the domain it is
  qualified in, and the names are the spec's, never a second list. The scope
  directives (`.. py:currentmodule::`, `.. c:namespace::`, …) likewise, taking
  no options. `domain_directive_specs` lists both with their domain, so a
  caller resolves `function` against the default domain as the dispatcher
  does.
- **An entity type's** spec is the schema's, known only once it loads. A spec
  is therefore `DirectiveSpec<'a>`, a built-in's `'static`, and an entity
  type's is lent by an `EntitySpec` built from the `EntityType`: `:id:`, then
  its attributes and relations as options, borrowing their names and labels.
  A type's sections are directives written in its content, not options, so
  they are not listed. A choice the schema declares is
  `OptionValue::Declared`, borrowing the type's list, where a built-in's
  choice is computed from an enum. The dispatcher builds the spec where it
  reads the block, rather than once beside the schema: it is a handful of
  borrowed records, and building it there keeps `ParseCtx` and every caller
  constructing one unchanged.

Two consequences shape the vocabulary. An option is a canonical `name` and
its `aliases` rather than one list of spellings, so an entity's option borrows
its name from the schema with no list to own; and a spec's options are an
`OptionSet` of its own options and the shared groups, so an entity's own list
needs no slice of groups beside it.

## Consequences

- Text directly below a marker continues the argument of a directive taking
  one, as in docutils. Measured: CPython's `Doc/` writes it 3400 times, every
  one a version change's explanation or an `.. index::`'s further entries —
  both readings this build already had — and never for a directive whose
  content would become its argument; this repository's documents likewise.
  `//examples:site` builds byte-identical.
- An argument may wrap onto the next line, for every directive taking one:
  `.. button-link::` (ADR-018's point 5 is superseded) and an image's address,
  which, as docutils' `directives.uri`, drops unescaped whitespace and keeps an
  escaped space.
- `.. py:module::` with a line directly below it reports the surplus words
  docutils refuses, rather than reading them as content; `.. index::` without
  an argument, `.. highlight::` without a language and `.. currentmodule::`
  without a module report the missing argument.
- `.. glossary::`, `.. index::` and the entity directives read their options
  through the reader, so an option they take none of is reported.
- No directive splits its own block any more. `collect_argument_continuation_lines`,
  `directive_content` and `take_option_block` are gone.
- An option an admonition, `.. seealso::` or an entity view does not take was
  ignored silently and is now reported; so are `:class:` and `:name:` on an
  admonition, which Sphinx takes and this build does not draw.
- `.. py:method::` reads `:abstract:` as Sphinx does, a spelling of
  `:abstractmethod:`; `:setup:` on a `.. needimport::`, which sphinx-needs 9.1
  does not take, is unknown rather than unsupported.
- Typed values: the value messages take the shared shape, a `:widths:` list
  of the wrong length is reported on its own line, and about thirty
  hand-written value checks and their helpers are gone
  (`parse_positive_depth`, `parse_widths_option`, `parse_positive_integer`,
  per-directive `report_invalid_choice`s). CPython's `lsp --check` is
  unchanged, diagnostic for diagnostic, and `//examples:site` builds
  byte-identical.
- The options measured the same way as the reader below: CPython's warnings
  are unchanged, 2628, and identical diagnostic for diagnostic but for 159
  `:no-typesetting:` on an object, now `directive.unsupported-option` where it
  was unknown; `//examples:site` builds byte-identical.
- Measured with `rinx lsp --check` over CPython's `Doc/` (556 documents, in
  10 s before and after): one warning more, 2628 against 2627 — a
  `.. availability::` in `library/socket.rst` written at a smaller indent
  than the rest of its `.. data::` body, which the shared indent now reads as
  docutils does instead of losing it — and three directives inside
  `.. list-table::` cells reported on their own line, two lines lower than
  before, since a list table's content is now positioned below its options.

## Alternatives considered

- **A mirror table for the editor, checked by tests.** An option the parser
  accepts and the table omits would go unnoticed; the table must be the
  parser's own.
- **Directive families as dispatch groups** (marker content, splicing, …),
  then **a flat enum of kinds.** Both existed in earlier drafts. The families
  mixed a real difference in calling convention with a split made only to keep
  a `match` short, and the enum named every directive a second time; with one
  invocation and a registry entry, neither is needed.
- **The parse function inside the spec.** It would tie the spec to behaviour,
  and to one syntax.
- **Exceptions for the quirky directives.** Every one turned out to be an
  instance of docutils' rule.

## Not done

- Completion itself, which reads these specs (roadmap #25).
- An entity *section* inside an entity, and a substitution definition's inner
  directive, still split their own block.

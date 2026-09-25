# 14. Flowcharts of the entity graph

## Status

Accepted.

## Context

ADR-011 gave the entity model a way to *list* the graph and ADR-012 a way to
*draw* it — but only by writing a template. sphinx-needs' own corpus draws the
graph far more often without one: after `needtable` and `needuml` were covered,
`needflow: 12` was the largest remaining construct in useblocks' demo, and
every one of those twelve is the same sentence — *show me these needs and the
links between them*.

ADR-012 closed by saying `needflow` "is the same question with a different
presentation" and would reuse both the filter language and the diagram
expander. This is that increment, and the interesting part is which of the two
it turned out to reuse.

## Decision

### 1. `.. entity-flow::` is a question, not a template

The directive parses to `Directive::EntityFlow`, a sibling of
`Directive::EntityTable` rather than of `Directive::Uml`. Both carry a filter,
a handful of presentation options and no content at all; what separates them is
how the answer is drawn — rows, or a picture a build action compiles.

`.. needflow::` is the reserved alias, following the `entity-table`/`needtable`
and `entity-diagram`/`needuml` precedent: our name is primary, a migrating
project keeps its documents, and a diagnostic quotes the name the author wrote.
Both names join `BUILTIN_DIRECTIVE_NAMES`, so an entity schema can no longer
declare a section by either.

**Lowering it to a `Uml` node holding generated Jinja was considered and
rejected.** It would have been less code — `{% for id in filter(...) %}{{
flow(id) }}{% endfor %}` is nearly the whole feature — and it would have been
wrong in three ways that all have the same shape: the author would be diagnosed
about text they never wrote. The filter would be re-parsed at *render* time
from a string synthesized at parse time, so a syntax error would lose the
column it breaks at, which is the entire reason `rusty_sphinx_filter` is parsed
by the parser (ADR-011). Every failure would report as `uml.template-error`
against a line of generated template. And `:relations:` needs to ask the schema
what it declares, which the template surface cannot do.

### 2. The diagnostic family is `entity-flow.*`

For ADR-012 §2's reason, applied the other way round. That ADR named the
diagram family `uml.*` because one node covers plain PlantUML too, and
`entity-diagram.invalid-align` on a `.. plantuml::` names an entity that is not
there. Here the opposite holds: a flowchart has no template, so
`uml.empty-result` on one names a construct that is not there. A code names the
construct.

The consequence is a second error type, `FlowError`, with two variants — a
filter that matched nothing, and a `:config:` naming no preamble. Only two,
because everything else a flowchart can get wrong was already checked while
parsing, where the option line's own position was still in hand.

### 3. Everything *after* the text exists is shared, deliberately

This is the constraint the whole change is organized around: there must be one
population of diagram hashes. A flowchart's text therefore goes through the
identical path a written diagram's does — `@startuml` wrapping, the `:config:`
preamble, the refusal to compile an empty picture, the hash, the `.puml` file,
the per-document compile action, `validate_images`. Three pieces moved to make
that literal rather than parallel:

- `crates/uml/src/assemble.rs` — the wrap/preamble/refuse/hash step, shared by
  `expand` and `build_flow`. `AssemblyError` is deliberately small and mapped
  onto each caller's own error type, which is what lets §2 hold.
- `crates/uml/src/node.rs` — the `rectangle` a single entity is drawn as,
  shared by the template function `flow(id)` and the generated picture. An
  entity should look the same and land on the same anchor whichever drew it.
- `crates/renderer/src/blocks/diagram_figure.rs` — the `<img>`, wrapper,
  caption and `:debug:` block. A reader should not be able to tell a generated
  picture from a written one, and a page already styled for one needs no new
  rule for the other.

No Bazel rule changed. Compilation was already a site action over
`diagram_ast_files`, and a flowchart is simply another diagram in a document of
a library that set `diagrams = True` — including the opt-in, which
`cmd_parse` now enforces for it under `entity-flow.diagrams-disabled`.

Sharing `node.rs` fixed a latent bug in `flow(id)` on the way: an entity id may
carry `-`, `.` and `:`, and an unquoted `REQ-1` is read by PlantUML as a
subtraction. Aliases are now quoted when they need it. The example site has
such an id (`REQ_req-4444f5cf`), which is how it surfaced.

### 4. Defaults that follow this build's schema, not sphinx-needs' vocabulary

Three places where matching sphinx-needs exactly would have been wrong here:

- **`:relations:` defaults to every relation the schema declares**, where
  sphinx-needs' `:link_types:` defaults to `links`. A schema here names its own
  relations and `links` may well not be one of them, so that default would
  routinely draw an edgeless picture. `:link_types:` is accepted as the
  sphinx-needs spelling of the option.
- **An edge whose target the filter excluded is not drawn.** A filter selects a
  subgraph; drawing the edge would make PlantUML invent an unlabelled box for
  every entity the author filtered *out*.
- **`:direction:` accepts `TB` and `LR` only.** PlantUML has two layout
  directions; `RL` and `BT` are graphviz' spellings. An author writing one is
  not making a typo, so the diagnostic says what PlantUML can do rather than
  offering a list.

### 5. What is refused by name

`:show_filters:`, `:show_legend:`, `:highlight:`, `:border_color:`,
`:filter-func:`, `:engine:`, the `:root_id:` family, and the legacy `:tags:`,
`:status:` and `:types:` filters are each reported as
`entity-flow.unsupported-option` with what to write instead. Silence is the one
behaviour that cannot be right: an author who asked for a legend and received a
picture without one has no way to find out why.

The `:root_id:`/`:root_direction:`/`:root_depth:` family is the most likely of
these to be implemented later — it is a bounded walk over the same relation
edges this already follows — and is refused rather than approximated meanwhile.

### 6. `:config:` gained sphinx-needs' two built-in preambles

`needs_flow_configs` ships `lefttoright` and `toptobottom`, so a sphinx-needs
document may write `:config: toptobottom` against a `conf.py` that never
mentions either — and half of the demo corpus's `.. needflow::` directives do.
Refusing those as "no preamble named `toptobottom` is declared" would be a true
statement about *our* config and useless to the author, so both names resolve
with nothing declared, and a `[uml_configs]` entry of the same name redefines
one. This applies to every diagram directive's `:config:`, not only a
flowchart's.

## Consequences

- `needflow` joins `needtable`, `needuml` and `needarch` as supported;
  `needpie`, `needbar` and `needsequence` remain unimplemented, and each is now
  a presentation problem alone. (`needpie` and `needsequence` have since been
  answered by ADR-017 and ADR-020.)
- A flowchart is linear in the size of the project per directive, since it
  builds a `Snapshot` like every templated diagram does. A project with
  thousands of entities and many flowcharts would want that snapshot hoisted
  per document — the same note ADR-012 already carries.
- `:name:` registers as an ordinary `:ref:` target, so a flowchart is
  linkable like a table or a figure.
- A flowchart's content depends on the entity graph rather than on any
  document's text, which gives the cache firewall a third case worth asserting:
  editing an entity in a document the flowchart never mentions must recompile
  it. `tests/test_diagram_cache_firewall.sh` covers it.

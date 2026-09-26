.. _entities:

Entities
========

An **entity** is a typed, identified, attributed thing that can be referenced
from prose and can point at other such things.

That shape is not new. A sphinx-needs requirement is one. CPython's
``.. audit-event::`` is one. A Python class, as far as the documentation build is
concerned, is one. What differs between them is vocabulary, not structure — so
rinx lets a project declare its own vocabulary in a schema file and
treats the result as a first-class construct: parsed, validated, indexed
project-wide, cross-referenceable, and rendered.

Sphinx-needs, in this model, is *a schema*, not a feature.

There is a worked schema and a set of documents using it under
``examples/entities/``; every example in this guide is taken from there, so the
two cannot drift apart.

The four kinds of declaration
-----------------------------

An entity type declares four different kinds of thing, and knowing which to
reach for is most of learning the model.

.. list-table::
   :header-rows: 1

   * - kind
     - what it is
     - parsed as RST?
     - in the index?
     - filterable?
   * - **attribute**
     - a value — a status, a priority, a list of tags
     - no
     - yes
     - yes
   * - **section**
     - a document — prose with headings, directives, links
     - **yes**
     - no
     - no
   * - **relation**
     - an edge to another entity
     - no
     - yes
     - yes
   * - **role**
     - how prose *points at* an entity
     - —
     - —
     - —

The rule in one line:

   **Attributes are values; sections are documents.**

If the thing you want to record is a string, a number, a choice from a fixed
set, or a list of those, it is an attribute. If it is prose that deserves
paragraphs, a code sample, a nested ``.. note::`` or a cross-reference, it is a
section. A requirement's ``status`` is an attribute; its verification criteria
are a section.

This is why the two are separate concepts rather than one field kind with a
``parsed = true`` flag: they differ in every later phase, not just at parse time.

Sections are the part of this model that goes beyond what sphinx-needs offers.

Declaring a type
----------------

.. code-block:: toml

   [[entity_type]]
   name     = "req"                      # the directive name: `.. req::`
   label    = "Requirement"              # shown by the built-in rendering
   argument = { fields = ["title"] }     # what `.. req:: <text>` means
   id       = { prefix = "REQ_" }

     [[entity_type.attribute]]
     name    = "status"
     label   = "Status"
     type    = "enum"
     values  = ["open", "in_progress", "closed"]
     default = "open"

     [[entity_type.section]]
     name     = "verification-criteria"
     label    = "Verification criteria"
     required = true

     [[entity_type.relation]]
     name           = "links"
     label          = "Links to"
     to             = ["spec", "impl"]
     multiple       = true
     incoming       = "linked_by"
     incoming_label = "Linked by"

and in a document:

.. code-block:: rst

   .. req:: The system shall boot within two seconds
      :id: REQ_001
      :owner: platform
      :status: in_progress
      :tags: boot, kernel
      :links: SPEC_001, IMPL_001

      The unnamed leading prose is the *content section*.

      .. verification-criteria::

         Measured with the boot harness on the reference board.

         .. note::

            A section's body is fully parsed, which is what lets it carry this.

Attribute types
~~~~~~~~~~~~~~~

``string``, ``text``, ``int``, ``bool``, ``enum`` (with ``values``), ``list<string>``,
``list<enum>`` (with ``values``).

A ``bool`` written bare — ``:deprecated:`` with no value — is ``true``, which is the
ordinary RST spelling of a flag. A list is comma-separated. Giving ``values`` for
a non-enum type, or omitting it for an enum one, is refused when the schema
loads: both spellings look like they constrain the value and only one does.

The three text types — ``string``, ``text`` and ``list<string>`` — also take a
``pattern``, a regular expression the value must match (for a list, each item
must):

.. code-block:: toml

   [[entity_type.attribute]]
   name    = "ticket"
   type    = "string"
   pattern = "^JIRA-[0-9]+$"

A value that does not match is reported as ``entity.invalid-attribute-value`` and
left unset, exactly like a value outside an enum's ``values`` — whether it was
written as an option, imported from a ``needs.json`` or set by an
``.. entity-update::``. The pattern is *searched for*, as JSON Schema's ``pattern``
keyword is, so write ``^…$`` to constrain the whole value. See "Patterns" under
"Identity" for the dialect. A ``pattern`` on any other type is refused when the
schema loads.

The ``title`` convention
~~~~~~~~~~~~~~~~~~~~~~~~

Nothing in the schema privileges any attribute name. The *rendering* convention
is that an attribute called ``title`` becomes the entity's heading instead of a
row in its field table. Declare it like any other attribute; a type that has no
title — ``audit-event``, say — simply does not declare one, and its links fall
back to showing the id.

The argument
~~~~~~~~~~~~

``argument.fields`` says what the text after ``::`` means.

- ``{ fields = ["title"] }`` — the whole argument is one value. The sphinx-needs
  shape.
- ``{ fields = ["name", "args", "version"], split = "comma" }`` — a
  comma-separated signature. CPython's ``.. audit-event::`` shape.
- omitted — the type takes no argument, and one given is diagnosed.

Fewer parts than declared fields is fine; the trailing fields are simply unset,
and a ``required`` one among them is then reported by name rather than by comma
count. *More* parts than declared is an error: there is nowhere to put them,
and dropping them silently is exactly the degradation these diagnostics exist
to catch.

Identity
--------

An entity's id is determined three ways, in this order:

1. **An explicit ``:id:``.** Used exactly as written. An illegal one is reported
   rather than repaired — silently rewriting it would leave the author's
   ``:id:`` and their ``:links:`` naming different things.
2. **Derived from attributes**, via ``id = { from = ["name"] }``. The named
   values are joined with ``_``, with any character an id forbids replaced by
   ``_``. This is how ``.. audit-event:: os.system, ...`` gets the id ``os.system``.
3. **Generated**, from a hash of the document path, the type and the source
   line.

An ``id.prefix`` is prepended to the derived and generated forms, never to an
explicit one. ``id = { required = true }`` refuses to generate at all.

Ids are **case-sensitive** and may hold letters, digits, ``_``, ``-``, ``.`` and ``:``.
``REQ_001`` and ``req_001`` are different entities. That matches sphinx-needs.

Generated ids are deterministic, which is not negotiable: a parsed document is
a build-system output cached on its inputs. The trade-off is that inserting a
line above an entity changes its generated id. That only affects entities whose
ids were never shown to anyone — give an entity an explicit ``:id:`` if you mean
to link to it.

Patterns
~~~~~~~~

``id = { pattern = "^REQ_[0-9]+$" }`` declares the naming convention a type's ids
should follow — sphinx-needs expresses the same thing as an ``id`` property in a
``schemas.json`` rule. It is checked against the **final** id, however it was
determined: explicit, derived, generated or imported, prefix included. A
generated id is a hash and will rarely match a convention, so a type declaring
a pattern usually wants ``required = true`` as well.

A mismatch is ``entity.id-pattern-mismatch``, reported on the ``:id:`` line when
there is one and on the directive otherwise. Unlike ``entity.invalid-id``, the
entity **keeps** its id: it is still a legal one and every link to it still
resolves — what is broken is the convention, and the fix is a rename only the
author can make. The separate code also lets a project migrating a large corpus
silence the convention on its own with a ``.. noqa:``, as sphinx-needs lets such a
rule be ``severity = "info"``.

Patterns, here and on attributes, are compiled when the schema loads, in the
dialect of Rust's ``regex`` crate. That is JSON Schema's ECMA-262 syntax for
everything a naming convention needs — anchors, classes, alternation,
repetition — but **not** lookaround or backreferences; a pattern using them is
refused when the schema loads rather than silently matching nothing.

Cardinality
-----------

``required`` and ``multiple`` are declarable on all three kinds.

.. list-table::
   :header-rows: 1

   * - kind
     - ``required`` means
     - ``multiple`` means
   * - attribute
     - the option must be given, unless it has a ``default``
     - n/a — use a ``list<…>`` type
   * - section
     - the sub-directive must appear at least once
     - it may appear more than once, kept in document order
   * - relation
     - at least one target must be named
     - more than one may be named; otherwise the option takes exactly one

Each violation has its own diagnostic, so you are told which rule you broke:
``entity.missing-required-attribute``, ``entity.missing-required-section``,
``entity.duplicate-section``, ``entity.missing-required-relation``,
``entity.multiple-relation-targets``.

An attribute that is both ``required`` and ``default``-ed is refused at load: it
could never be missing, so one of the two is a mistake.

Sections
--------

A section is a sub-directive written inside an entity's body. Its content is
fully-parsed RST.

- The unnamed prose before the first section directive is the **content
  section**. It is one of the sections, not a field beside them, so document
  order is preserved and a section named ``content`` cannot be confused with it.
- Sections **do not nest**: a section directive inside a section is diagnosed
  rather than silently accepted.
- Entities *may* nest inside a section, and everything inside one — targets,
  glossary terms, further entities — is indexed normally.
- A section name may not shadow a directive the build already has, so you
  cannot declare a section called ``note``. This is checked when the schema
  loads.
- A section is recognised only inside a type that declares it. Written anywhere
  else it is reported (``entity.unknown-section``,
  ``entity.section-outside-entity``) rather than degrading into an unknown
  directive that renders as nothing.

Section content is deliberately **not** stored in the project index: a listing
directive needs fields, not paragraphs, and keeping prose out is what bounds
the index on a project with thousands of requirements.

Relations and back-links
------------------------

A relation is declared on the type that **carries** it, beside that type's
attributes. There is no ``from`` list, because the source is wherever the option
is written — which removes a whole class of error where a declared source
drifts from actual use.

.. code-block:: toml

     [[entity_type.relation]]
     name           = "verifies"
     to             = ["req"]        # allowed target types; omit for any
     required       = true
     multiple       = true
     incoming       = "verified_by"  # the back-link derived on the target
     incoming_label = "Verified by"

**Back-links are derived, not declared.** A ``req`` never says it can be
``verified_by`` something; that falls out of ``test.verifies`` pointing at it. So
reading the ``req`` block alone does not tell you it will show a "Verified by"
list — the accepted cost of declaring relations where they are used.

Derivation happens project-wide while the index is built, because an entity's
incoming edges come from documents it has never heard of.

What must agree
~~~~~~~~~~~~~~~

A shared **outgoing** name constrains nothing. ``req.links`` and ``spec.links`` are
independent declarations that may differ in ``to``, in ``label``, and even in
``incoming`` — they render on different types and feed different back-links.

The constraints are on the derived side:

1. Two relations whose ``incoming`` names meet **on a shared target type** must
   agree on ``incoming_label``. That target has one back-link fed by both, so two
   labels leave it with no coherent heading. If their ``to`` sets are disjoint,
   no entity ever sees both and they may differ freely.
2. A derived back-link name must not collide with an attribute, section or
   relation the target type declares itself.

There is no reserved-word rule: the rendering keeps attributes, sections,
outgoing links and incoming links in separate namespaces, so a back-link cannot
shadow ``id`` or ``title`` however it is named.

Omitting ``incoming`` makes a relation one-directional.

Roles
-----

**A role is not required for an entity to be linkable.** Every entity registers
an ordinary target, so ``:ref:`REQ_001``` reaches one with no role machinery
at all, and the built-in ``:entity:`REQ_001``` role accepts every type without
being declared.

What a declared role adds is exactly two things:

1. **A type check on the link.** ``:req:`SPEC_003``` is reported as pointing
   at the wrong kind of thing. It still links — the target exists, and a dead
   link would help nobody — but you are told.
2. **The spelling an existing project already writes**, which is what lets a
   sphinx-needs project migrate by choosing a schema rather than by editing
   every document.

Roles are declared at the schema's top level, not inside a type, because the
relation is many-to-many: ``:need:`` refers to four types at once.

.. code-block:: toml

   [[role]]
   name  = "req"
   types = ["req"]        # a typed reference

   [[role]]
   name  = "need"         # the sphinx-needs spelling
   types = ["req", "spec", "impl", "test"]

   [[role]]
   name  = "anything"     # omit `types` to accept any entity

A reference with no explicit title shows the entity's title, falling back to
its id. ``:req:`the boot budget <REQ_001>``` overrides that.

Presentation
------------

With no configuration, an entity renders as a titled block carrying:

- its id as an anchor (``id="entity-REQ_001"``), which is what every link to it
  targets;
- a header with the type label, the title and the id;
- a table of its attribute values, under their declared labels;
- its sections, its prose first and then its named ones **in the order the
  schema declares them**, each under its declared label;
- its outgoing relations and its derived incoming back-links, as labelled
  lists of links.

The CSS classes are ``entity``, ``entity-<type>``, ``entity-header``,
``entity-attributes``, ``entity-content``, ``entity-section``,
``entity-section-<name>``, ``entity-links``, ``entity-link-label`` and
``entity-link-list``. Note that they describe the *generic* shape, not any one
type: a project that declares a new ``.. req::`` needs no CSS of its own.

Both stylesheets in the repository carry these rules — ``assets/default.css``,
the theme ``templates/default.html`` links, and the inline ``<style>`` in
``examples/custom_template.html``, which the example site uses instead. Styling a
new construct means updating both, or the example site silently keeps rendering
it unstyled.

Per-type templates
~~~~~~~~~~~~~~~~~~

A type may name a MiniJinja template instead, which is what lets
``.. audit-event::`` read as prose while a ``.. req::`` renders as a box:

.. code-block:: toml

   [[entity_type]]
   name     = "audit-event"
   template = "entity_audit_event.html"

The schema names the file; the site supplies it:

.. code-block:: python

   rinx_site(
       entity_templates = ["//examples/entities:entity_audit_event.html"],
   )

The split is ADR-001's rule — a sandboxed build relocates files, so a path in
config would break. A template a type names but the site does not supply fails
the build rather than silently falling back.

The template receives:

.. list-table::
   :header-rows: 1

   * - name
     - what it is
   * - ``id``, ``type``, ``label``, ``title``
     - the entity's identity; ``title`` may be absent
   * - ``anchor``
     - the ``id`` attribute a link to this entity targets
   * - ``content``
     - the content section, **as rendered HTML**
   * - ``sections``
     - ``{name: [html, …]}`` — a list per name, since a section may be ``multiple``
   * - ``section_list``
     - ``[{name, label, body}, …]`` — the same bodies **in render order**
   * - ``attributes``
     - ``{name: value}``, as strings
   * - ``outgoing``, ``incoming``
     - ``{relation: [{id, href}, …]}``
   * - ``labels``
     - ``{attributes, sections, relations}`` — the headings the schema declares

Four separate namespaces, so a back-link named ``title`` cannot shadow the
entity's own — which is why no reserved words are needed. Hyphens in section
and relation names become underscores (``sections.verification_criteria``),
because a hyphen is a subtraction in a Jinja expression.

Section bodies arrive as **already-rendered HTML**: MiniJinja cannot render
RST, so the node tree never reaches a template. They are marked safe and are
not escaped again; every other value is ordinary text and is escaped as usual.

``labels`` exists so a template renders a heading under the name the schema
declares instead of holding a second copy that can drift from it. Its
``relations`` map covers both directions, and the incoming half is derived the
way the built-in rendering derives it — a back-link is declared on the type at
the *other* end, so ``req`` never mentions the ``implemented_by`` that lands on it.

``section_list`` exists because ``sections`` is keyed by name and a map carries no
order at all. It holds what the built-in rendering shows, in the order it shows
it, so a template can match the built-in box; use ``sections`` to reach one
section by name.

Build wiring
------------

The schema is a **parse-time** input: it is what makes ``.. req::`` a directive
rather than an unknown name. So it is declared on every library whose documents
use it, *and* on the site that assembles them.

.. code-block:: python

   rinx_library(
       name = "docs",
       srcs = glob(["*.rst"]),
       entity_schema = "entities.toml",
   )

   rinx_site(
       name = "site",
       entity_schema = "//examples/entities:entities.toml",
       deps = [":docs"],
   )

They must be the same file. A document parsed against a *different* schema than
the site indexes with is reported as ``entity.schema-mismatch`` rather than
producing quietly wrong output — including the likelier mistake of declaring it
on the library and forgetting it on the site.

Only libraries that actually use entities need to declare it. A library parsed
against no schema at all is not reported: most libraries in a multi-library site
use no entities and have no reason to name one. A library that *does* use them
and forgets to declare it is not silently lost either — its directives are never
recognised, so it surfaces as unknown directives and dangling references, which
point at the offending line rather than at the whole document.

The cost of reaching parse time is real and accepted: **editing the schema
re-parses every document in every library**, not just re-index and re-render.
That is the price of parse-time diagnostics.

A project that declares no schema behaves exactly as it did before this feature
existed, byte for byte.

Editor support for the schema file
----------------------------------

TOML has no schema language of its own, so ``schemas/entities.schema.json`` is a
JSON Schema over TOML's data model. Editors with TOML support — ``taplo``, and
through it VS Code's *Even Better TOML* — pick it up two ways: ``.taplo.toml``
associates every ``**/entities.toml`` in this repository, and a file outside it
can point at the schema itself:

.. code-block:: toml

   #:schema ../../schemas/entities.schema.json
   [[entity_type]]
   name = "req"

That gives completions for every key and every attribute type spelling, and
flags a mistyped key while you write it.

**It validates the grammar, not the semantics.** The schema knows about table
structure, field names and types, required versus optional, the seven ``type``
spellings, the two ``split`` values, and unknown keys. It cannot know that a
relation's ``to`` names a declared entity type, that ``argument.fields`` names a
declared attribute, that two types share a name, or any of the other rules in
the *Cardinality* and *Relations* sections above — JSON Schema has no way to
say "this string must name something declared elsewhere in this document".

So a clean editor means the shape is right, not that the schema loads. The
loader remains the authority, and it is what the build runs.

The schema is **generated** from the types the loader deserializes into, and
checked in. After changing them:

.. code-block:: bash

   cargo run -p rinx -- entity_json_schema > schemas/entities.schema.json

A test regenerates and compares, so a stale file fails the build; another
validates ``examples/entities/entities.toml`` against it, so the schema, the
example and the loader cannot drift apart.

Listing entities
----------------

*(Why it is built this way: ``docs/decisions/011-entity-listing.md``.)*

A ``.. entity-table::`` asks the graph a question and renders the answer as a
table. Its rows come from the project index rather than from the document it is
written in, so it lists entities declared anywhere in the site.

.. code-block:: rst

   .. entity-table::
      :filter: type == "req" and status != "closed"
      :columns: id, title, status, verified_by
      :sort: title

``.. needtable::`` is accepted as a second spelling of the same directive, so a
project migrating from sphinx-needs keeps its documents. Both names are
reserved, which means an entity schema may not declare a type or section called
either. The two render identically and share one set of diagnostic codes: an
author who wrote ``.. needtable::`` still suppresses with
``entity-table.invalid-filter``.

The field vocabulary
~~~~~~~~~~~~~~~~~~~~

A field name is any of five built-ins, or anything the schema declares — an
attribute, an outgoing relation, or a **derived back-link**, which is nowhere
declared and still filterable and showable. One name works in ``:filter:``,
``:columns:`` and ``:sort:`` alike, and a name no type declares is
``entity-table.unknown-field`` at parse time rather than a column that silently
renders empty.

.. list-table::
   :header-rows: 1

   * - field
     - value
   * - ``id``
     - the entity's id
   * - ``type``
     - the *directive name* — ``req``
   * - ``type_name``
     - the schema's *label* — ``Requirement``
   * - ``title``
     - the title, absent when the type maps none
   * - ``docname``
     - the document the entity was written in

``type`` and ``type_name`` read backwards, and are sphinx-needs' own names kept
deliberately, so a migrating project's filters mean here what they meant there.
A built-in wins over a same-named attribute.

Since one table may list several types, a field only some of them declare is
perfectly good: it is simply missing on the others, which is what makes
``status == "open"`` skip an entity whose type has no status rather than fail.

The filter language
~~~~~~~~~~~~~~~~~~~

Python's spelling, over the subset that real filters use:

.. list-table::
   :header-rows: 1

   * - ..
     - ..
   * - comparison
     - ``==``, ``!=``
   * - containment
     - ``in``, ``not in`` — substring over text, membership over a list
   * - presence
     - ``is None``, ``is not None``
   * - combination
     - ``and``, ``or``, ``not``, parentheses
   * - affixes
     - ``field.startswith("…")``, ``field.endswith("…")`` — the only two methods
   * - values
     - field names, ``"strings"``, ``'strings'``, whole numbers, ``True``/``False``
   * - bare field
     - true when the value is non-empty, as in Python

Missing values have a defined answer everywhere rather than an error: a missing
field equals nothing (so ``status == "open"`` is false), ``!=`` is the exact
negation of ``==``, and ``in`` against a missing haystack is false. The same holds
for the two affix tests, which are measured over the text ``in`` already sees —
so ``"F" in id`` and ``id.startswith("F")`` cannot disagree about what a
non-text field is worth.

``startswith`` and ``endswith`` are the one place this language admits Python's
``.``, and they earned it by measurement: eight wedges of the benchmark corpus'
pie charts select by id prefix, and refusing them left each one counting the
whole project. Every *other* method and every bare attribute access is still
refused by name.

**Everything outside this is refused by name, with a position.** These are all
valid Python and none of them is silently ignored:

.. list-table::
   :header-rows: 1

   * - written
     - reported
   * - ``len(tags) > 0``
     - function calls are not supported, pointing at ``len``
   * - ``title.lower() == "x"``
     - ``.lower()`` is not supported; only ``startswith`` and ``endswith`` are
   * - ``need.id == "REQ_1"``
     - attribute access is not supported; name the field on its own
   * - ``[[copy('id')]]``
     - dynamic functions are not supported
   * - ``[n for n in needs]``
     - comprehensions are not supported
   * - ``status != None``
     - comparing to ``None``; use ``is not None``
   * - ``a && b``
     - use ``and``
   * - ``priority > 2``
     - ordering comparisons are not supported

A filter that fails to parse leaves the table listing *everything* rather than
nothing: the diagnostic already says what is wrong, and an empty table on top of
it would hide what the author was reaching for.

There is deliberately no ``filter_func`` and no dynamic function: both are
sphinx-needs calling Python, and this build has no interpreter.

Options
~~~~~~~

.. list-table::
   :header-rows: 1

   * - option
     - meaning
   * - ``:filter:``
     - which entities to list; omitted lists every one
   * - ``:columns:``
     - the fields to show, in order. Default: ``id``, ``type``, ``title``
   * - ``:sort:``
     - one field to order by; omitted orders by id
   * - ``:widths:`` / ``:colwidths:``
     - two spellings of one option — giving both is an error rather than a guess
   * - ``:width:``, ``:align:``, ``:class:``, ``:name:``
     - as on every other table directive
   * - ``:style:``
     - only ``table``; ``datatables`` is reported, since it is a JavaScript grid

The ``id`` and ``title`` columns link to the entity, and a column of relations or
back-links links to each target. A list-valued column becomes links only when
every item names an entity the index knows, so a ``tags`` column stays text.

Ordering by id when no ``:sort:`` is given is not an implementation detail: a
rendered page is a build artefact cached on its inputs, so the default order has
to be deterministic rather than merely stable within one run.

What it does not do
~~~~~~~~~~~~~~~~~~~

A table whose filter matches nothing renders its headings and reports
``entity-table.empty-result`` — an empty listing is far more often a filter that
no longer matches than a deliberate statement. The live preview stays quiet
about it when no project index is available, since every table would be empty
through no fault of the author.

``needflow``, ``needsequence``, ``needpie`` and ``needbar`` *are* implemented — see
"Flowcharts of the graph", "Sequence diagrams of the graph" and "Charting the
graph" below, which ask about this same graph and draw the answer instead of
tabulating it. ``needlist`` is not. It is the same question again with a
different presentation, and would reuse this filter language unchanged.

Flowcharts of the graph
-----------------------

``.. entity-flow::`` — sphinx-needs spells it ``.. needflow::`` — draws the entities
a filter selects and the relations between them. It asks the question
``.. entity-table::`` asks and answers it as a picture instead of as rows, so
there is nothing to write: the directive takes options only, and its ``PlantUML``
is generated.

.. code-block:: rst

   .. entity-flow::
      :filter: type == "req" or type == "spec"
      :relations: links
      :show-link-names:
      :direction: LR
      :caption: Requirements and the specifications they link to

.. list-table::
   :header-rows: 1

   * - option
     - what it does
   * - ``:filter:``
     - which entities to draw; omitted, the whole project
   * - ``:relations:``
     - which relations become edges, in the order written; sphinx-needs spells it ``:link_types:``
   * - ``:show-link-names:``
     - label each edge with its relation
   * - ``:direction:``
     - ``TB`` (the default) or ``LR``
   * - ``:config:``
     - a ``PlantUML`` preamble: sphinx-needs' built-in ``lefttoright``/``toptobottom``, or one declared under ``[uml_configs]``
   * - ``:debug:``
     - also show the generated source, below the picture
   * - ``:caption:`` ``:align:`` ``:width:`` ``:scale:`` ``:class:`` ``:name:``
     - as on every diagram

Each node is the same clickable rectangle ``flow(id)`` draws in a written
diagram, so clicking one lands on exactly the anchor a ``:ref:`` to that entity
would, and a ``:name:`` makes the picture itself a ``:ref:`` target.

Three defaults differ from sphinx-needs', because a schema here declares its own
vocabulary:

- **An omitted ``:relations:`` draws every relation the schema declares**, where
  sphinx-needs defaults to ``links`` — a name a schema here need not have at all.
- **An edge whose target the filter excluded is not drawn.** A filter selects a
  subgraph, and drawing the edge would make ``PlantUML`` invent an unlabelled box
  for every entity the author filtered out.
- ``:direction:`` takes ``TB`` or ``LR`` only. ``PlantUML`` has two layout
  directions; graphviz' ``RL`` and ``BT`` are reported rather than ignored.

A filter matching nothing is ``entity-flow.empty-result`` and no picture is
compiled — ``PlantUML`` rejects an empty diagram, so the build would otherwise
fail with a syntax error naming a generated file nobody wrote.

sphinx-needs' ``:show_filters:``, ``:show_legend:``, ``:highlight:``,
``:border_color:``, ``:filter-func:``, ``:engine:``, the ``:root_id:`` family and the
legacy ``:tags:``/``:status:``/``:types:`` filters are each reported by name as
``entity-flow.unsupported-option``, with what to write instead, rather than
silently doing nothing.

A flowchart is a diagram, so its library needs ``diagrams = True`` exactly as the
written ones do — see the build wiring under "Diagramming entities" below. See
``docs/decisions/014-entity-flow.md``.

Sequence diagrams of the graph
------------------------------

``.. entity-sequence::`` — sphinx-needs spells it ``.. needsequence::`` — draws the
messages entities send each other. Where a flowchart *filters* the graph, a
sequence diagram *walks* it: from each ``:start:`` entity, a relation named in
``:relations:`` leads to a **message** entity, and the same relations lead on from
the message to its **receivers**. Every hop is an arrow labelled with the
message's title, and every receiver not yet seen is walked in turn, depth
first — so the arrows read in the order the walk reaches them, as they do in
sphinx-needs.

A schema declares the shape by giving both types the same relation:

.. code-block:: toml

   [[entity_type]]
   name = "component"
     [[entity_type.relation]]
     name = "calls"
     to = ["message"]

   [[entity_type]]
   name = "message"
     [[entity_type.relation]]
     name = "calls"
     to = ["component"]

.. code-block:: rst

   .. entity-sequence:: Startup
      :start: COMP_UI
      :relations: calls

.. list-table::
   :header-rows: 1

   * - option
     - what it does
   * - argument
     - the caption, as in sphinx-needs; ``:caption:`` overrides it
   * - ``:start:``
     - **required** — the entities the walk begins at, separated by ``,`` or ``;``
   * - ``:relations:``
     - **required** — the relations that carry messages; sphinx-needs spells it ``:link_types:``
   * - ``:filter:``
     - which *receivers* to keep; one it rejects gets no arrow and is not walked on from
   * - ``:max-items:``
     - the most messages to draw (``0`` for all); sphinx-needs spells it ``:max_items:``
   * - ``:config:``
     - a ``PlantUML`` preamble, as on every diagram
   * - ``:debug:``
     - also show the generated source, below the picture
   * - ``:align:`` ``:width:`` ``:scale:`` ``:class:`` ``:name:``
     - as on every diagram

Each lifeline links to its entity's anchor, as a flowchart's node does.

Where this departs from sphinx-needs:

- ``:relations:`` is mandatory. sphinx-needs defaults it to ``links``, a name a
  schema here need not have; and unlike a flowchart's, "every relation the
  schema declares" is no sensible default for a walk, since it would follow
  edges that are no message at all. A diagram without it is an error block
  whose diagnostic lists the relations the schema does declare.
- **Several starts share one walk.** sphinx-needs walks each ``:start:`` entry
  afresh, so a participant the first start already reached is declared again
  and its messages are drawn twice. Here a later start continues where the
  earlier ones left off.
- **Every lifeline carries its title.** sphinx-needs declares only senders, so
  a receiver that never sends — or the receiver of the last message
  ``:max-items:`` allowed — appears under its raw id.
- **Nothing aborts the build.** An unknown start is reported as
  ``entity-sequence.unknown-start`` and the other starts are still drawn.

A walk that draws no message is ``entity-sequence.empty-result`` and no picture
is compiled. A walk ``:max-items:`` cut short still draws, notes beneath the
picture how many of how many messages it shows, and reports
``entity-sequence.truncated`` so the build log says so too — silence it with a
``.. noqa:`` when the cap is deliberate.

sphinx-needs' ``:show_filters:``, ``:show_legend:``, ``:show_link_names:``,
``:highlight:``, ``:filter-func:``, ``:sort_by:``, ``:export_id:``,
``:filter_warning:``, ``:height:``, ``:engine:`` and the legacy
``:tags:``/``:status:``/``:types:`` filters are each reported by name as
``entity-sequence.unsupported-option``, with what to write instead.

Like a flowchart, a sequence diagram is compiled, so its library needs
``diagrams = True``. See ``docs/decisions/020-entity-sequence.md``.

Charting the graph
------------------

``.. entity-pie::`` — sphinx-needs spells it ``.. needpie::`` — asks the question
``.. entity-table::`` asks and answers it as *proportions*. Each line of its body
is one filter, and the wedge it draws is how many entities that filter selects.

.. code-block:: rst

   .. entity-pie:: Requirements by status
      :labels: Open, In progress, Closed
      :legend:
      :colors: #4c72b0, #dd852c, #55a868
      :caption: Where the requirements on this site stand

      type == "req" and status == "open"
      type == "req" and status == "in_progress"
      type == "req" and status == "closed"

Its shape is unlike either sibling's: an ``.. entity-table::`` takes no argument
and an ``.. entity-flow::`` takes no content, while a chart takes **both**. The
argument is the title, and ``:labels:`` pairs with the content lines **by
position**.

.. list-table::
   :header-rows: 1

   * - option
     - what it does
   * - ``:labels:``
     - names the wedges, in the order their filters were written
   * - ``:filter:``
     - narrows the entities *before* any wedge counts them, so a chart can be scoped once
   * - ``:legend:``
     - draw a key naming each wedge with its count
   * - ``:colors:``
     - the wedge colours, as CSS hex (``#4c72b0``, ``#abc``) or a basic keyword; short lists repeat
   * - ``:text_color:``
     - the colour of the percentages drawn on the wedges
   * - ``:caption:`` ``:align:`` ``:width:`` ``:scale:`` ``:class:`` ``:name:``
     - as on every picture

A body line that is a plain **number** is used as the wedge's size directly,
for a chart whose data does not come from the graph at all. A chart of numbers
alone never reads the index.

**A chart needs no ``diagrams = True``.** It is the one picture here that is not
compiled: its SVG is drawn while the page is rendered, so it creates no build
action, writes no ``.puml``, and starts no JVM. Keep charts in an ordinary
library — ``examples/entities/charts.rst`` does, beside the tables rather than in
``diagram_docs``.

Three things are reported rather than passed over:

- **A chart whose every wedge counts zero** is ``entity-pie.empty-result`` and no
  picture is drawn, for the reason an empty table is reported: a filter that no
  longer matches is far likelier than a deliberate statement. As with a table,
  the live preview stays quiet about it when no project index is available.
- **A body with no content line at all** is ``entity-pie.no-slices``, reported
  while parsing — the body is this document's own text, so no index is needed.
- ``:labels:`` disagreeing with the wedge count is
  ``entity-pie.label-count-mismatch``. They pair by position, so a mismatch means
  at least one wedge is named wrongly, and the labels alone do not show it. The
  wedges are drawn either way.

sphinx-needs' ``:explode:``, ``:shadow:``, ``:style:`` and ``:filter-func:`` are each
reported by name as ``entity-pie.unsupported-option``, with what to write
instead. See ``docs/decisions/017-entity-pie.md``.

Bar charts
~~~~~~~~~~

``.. entity-bar::`` — sphinx-needs spells it ``.. needbar::`` — asks the same
question in two dimensions. Each body line is one **series** (a legend entry,
drawn in one colour), split on ``:separator:`` into one cell per **category**
along the axis. A cell is a filter whose count is the bar's height, or a number
written outright.

.. code-block:: rst

   .. entity-bar:: Work done and left to do
      :xlabels: FROM_DATA
      :ylabels: FROM_DATA
      :stacked:
      :show_top_sum:
      :legend:

      , Still to do, Done
      Requirements, type == "req" and status != "closed", type == "req" and status == "closed"
      Specifications, type == "spec" and status == "draft", type == "spec" and status == "approved"

.. list-table::
   :header-rows: 1

   * - option
     - what it does
   * - ``:xlabels:``
     - the category names, comma-separated — or ``FROM_DATA`` to take them from the body's first line
   * - ``:ylabels:``
     - the series names, comma-separated — or ``FROM_DATA`` to take them from each line's first cell
   * - ``:separator:``
     - what a body line is split on (default ``,``); for a filter that holds a comma
   * - ``:transpose:``
     - swap series and categories, labels included
   * - ``:stacked:``
     - pile a category's series on top of each other rather than side by side
   * - ``:horizontal:``
     - bars run rightwards, the first category at the top
   * - ``:show_sum:``
     - write each bar's value in its middle
   * - ``:show_top_sum:``
     - write each bar's value past its end — stacked, the stack's total
   * - ``:x_axis_title:`` ``:y_axis_title:``
     - name the axes
   * - ``:xlabels_rotation:`` ``:ylabels_rotation:`` ``:sum_rotation:``
     - turn the axes' labels or the written values by any whole number of degrees
   * - ``:filter:`` ``:legend:`` ``:colors:`` ``:text_color:``
     - as on a pie — but a short ``:colors:`` list is *continued by the palette* rather than repeated, as in sphinx-needs
   * - ``:caption:`` ``:align:`` ``:width:`` ``:scale:`` ``:class:`` ``:name:``
     - as on every picture

Like a pie, a bar chart is drawn while rendering and needs no
``diagrams = True``. Where sphinx-needs fails the build, this one reports and
draws: a short row is padded with zeros (``entity-bar.ragged-row``), a label list
of the wrong length keeps the data (``entity-bar.label-count-mismatch``), and a
rotation that is not whole degrees is ``entity-bar.invalid-rotation`` rather than
silently ignored. A body with no values is ``entity-bar.no-data``, a chart whose
every cell counts zero ``entity-bar.empty-result``, and ``:style:`` and the legacy
``:status:``/``:tags:``/``:types:``/``:cypher:`` are each
``entity-bar.unsupported-option``, with the ``:filter:`` to write instead. See
``docs/decisions/021-entity-bar.md``.

Diagramming entities
--------------------

``.. entity-diagram::`` — sphinx-needs spells it ``.. needuml::`` — is a PlantUML
diagram whose body is a Jinja template expanded against the entity graph, so a
picture can be *derived* from what the documents declare rather than restated
beside them.

.. code-block:: rst

   .. entity-diagram::
      :caption: Every requirement in the project

      @startuml
      {% for id in filter('type == "req"') %}
      {{ flow(id) }}
      {% endfor %}
      @enduml

The template surface:

.. list-table::
   :header-rows: 1

   * - written
     - what it does
   * - ``needs``
     - every entity, by id, in id order
   * - ``need``
     - the entity an ``.. entity-arch::`` sits inside
   * - ``need(id)``
     - one entity's fields
   * - ``filter(expr)``
     - the ids matching a filter, in id order
   * - ``flow(id)``
     - a clickable PlantUML node for one entity
   * - ``ref(id, text)``
     - a PlantUML link to one entity
   * - ``uml(id, key)``
     - another entity's diagram, expanded here
   * - ``imports(id, rel…)``
     - the diagrams of everything ``id`` points at

``filter()`` is the very language "Listing entities" describes, so a filter means
the same thing in a table and in a diagram. ``flow()`` and ``ref()`` build the same
href a ``:ref:`` to that entity would, so a clickable node lands on the anchor the
page actually has.

An entity's attributes, relations and derived back-links share one namespace
with the built-in ``id``, ``type``, ``type_name``, ``docname`` and ``title`` — so a
``.. req::``'s ``:owner:`` reads as ``need.owner``. A built-in name always wins, so a
schema declaring an attribute called ``id`` cannot change what ``need.id`` means.

Architecture diagrams
~~~~~~~~~~~~~~~~~~~~~

``.. entity-arch::`` (sphinx-needs: ``.. needarch::``) is written *inside* an entity
and binds it as ``need``. Its ``:key:`` stores the template on the entity so
another diagram can import it:

.. code-block:: rst

   .. req:: The bootloader shall verify the kernel signature
      :id: REQ_BOOT
      :owner: platform

      .. entity-arch::
         :key: overview

         component "{{ need.title }}" as {{ need.id }}

.. code-block:: rst

   .. entity-diagram::

      @startuml
      {{ uml('REQ_BOOT', 'overview') }}
      @enduml

This is what makes architecture diagrams compositional: a component draws
itself once, and every diagram that reaches it pulls that picture in. A diagram
written inside a *section* still knows its entity, even though a section body
deliberately forgets the entity's type.

An import that reaches itself — directly or around a cycle — is
``uml.recursive-import``, reported with the route it took. Importing the same
picture twice side by side is ordinary composition, not a cycle.

Options
~~~~~~~

``:caption:``, ``:align:``, ``:width:``, ``:scale:``, ``:class:`` and ``:name:`` place the
finished picture, exactly as they place an ``.. image::``; a ``:name:`` registers as
a ``:ref:`` target. ``:extra:`` binds comma-separated ``name: value`` pairs into the
template. ``:debug:`` shows the expanded PlantUML below the picture — the text
that was actually compiled, which is what you need when a diagram comes out
wrong. ``:config:`` names a preamble declared under ``[uml_configs]`` in the site's
``rinx.toml``, or one of the two sphinx-needs ships — ``lefttoright`` and
``toptobottom`` — which need nothing declared anywhere; a site entry of the same
name redefines one.

``@startuml``/``@enduml`` are added when they are missing, so a ``:key:`` fragment is
both importable and a diagram in its own right. A diagram that draws nothing —
a ``filter()`` matching no entity, say — is reported as ``uml.empty-result`` and
the page simply shows no picture, the same way an empty ``.. entity-table::``
renders its headings and warns.

``:save:`` is reported as ``uml.save-unsupported``: a sandboxed build action may
only write files declared before it runs. Build the site's ``diagram_sources``
output group to get every diagram's expanded source instead.

Build wiring
~~~~~~~~~~~~

Diagrams are **opt-in per library**: a library whose documents draw anything
sets ``diagrams = True``, and one that does not pays nothing for the feature.

.. code-block:: python

   rinx_library(
       name = "docs",
       srcs = glob(["*.rst"], exclude = ["diagrams.rst"]),
       entity_schema = "entities.toml",
       deps = [":diagram_docs"],
   )

   # The diagram documents, in a library of their own so the cost stays on them.
   rinx_library(
       name = "diagram_docs",
       srcs = ["diagrams.rst"],
       entity_schema = "entities.toml",
       diagrams = True,
   )

Forgetting the attribute is not silent: a diagram in a library without it
fails the parse as ``uml.diagrams-disabled`` — a flowchart as
``entity-flow.diagrams-disabled``, a sequence diagram as
``entity-sequence.diagrams-disabled`` — on the directive's own line, naming the
attribute to set.

A diagram's text depends on the whole entity graph, so it cannot be expanded
until every document has been indexed. Compilation therefore belongs to
``rinx_site``, not to any ``rinx_library`` — including for a plain
``.. plantuml::``, which goes the same way so that the set of diagrams the build
compiles and the set it validates cannot drift apart. The render action writes
each diagram's expanded source, and a per-document compile turns it into an
SVG; an edit that leaves a diagram's text unchanged starts no JVM. See
``docs/decisions/012-entity-diagrams.md``.

Migrating from sphinx-needs
---------------------------

``examples/entities/entities.toml`` reproduces the built-in vocabulary — ``req``,
``spec``, ``impl``, ``test``, their statuses and tags, ``links`` and its back-link, and
the ``:need:`` role — closely enough that documents using those constructs work
unchanged.

Deliberately **not** supported:

- **Python filter strings.** ``.. needtable::`` and its ``:filter:`` *are*
  supported — see "Listing entities" above — but through this build's own
  typed filter language rather than a Python interpreter. It covers the
  operators real filters use and diagnoses everything else by name, so a filter
  calling ``len()`` is reported rather than silently matching nothing.
- **The remaining listing directive** (``needlist``) — a later increment. It
  would reuse the same filter language. ``needtable``, ``needflow``,
  ``needsequence``, ``needpie``, ``needbar``, ``needuml`` and ``needarch`` *are*
  supported; see "Listing entities", "Flowcharts of the
  graph", "Sequence diagrams of the graph", "Charting the graph" and
  "Diagramming entities".
- **Dynamic functions** (``[[copy('id')]]``) and ``needservice``. The latter is
  refused by name as ``needservice.unsupported`` rather than reported as an
  unknown directive: it queries an external service while building, which a
  sandboxed action cannot do, and its warning names the route instead — save
  the needs to a ``needs.json`` (a last Sphinx build exports every
  service-fetched need in one), declare it in ``parse_data``, and read it with
  ``.. needimport::``. ``needimport``
  *is* supported — see "Importing from sphinx-needs" below — and so is
  ``needextend``, under this build's own name ``.. entity-update::``; see
  ``docs/decisions/019-entity-update.md``.

Importing from sphinx-needs
---------------------------

``.. needimport::`` reads a sphinx-needs ``needs.json`` and turns every need in it
into an entity of the importing document. It is a **migration bridge**: it
keeps an existing project's import lines working, and deliberately does not
take a name of this build's own — ``entity-import`` is left free for a richer
construct later. *(Why it is built this way:
``docs/decisions/016-needimport.md``.)*

.. code-block:: rst

   .. needimport:: needs.json
      :version: 2.0
      :ids: PLAT_REQ_1, PLAT_REQ_2
      :filter: "startup" in tags
      :id_prefix: EXT_
      :tags: imported, upstream

Because the file is read *while parsing*, an imported need is an ordinary
entity in every later phase: it is indexed, it is a ``:ref:`` target, its
relations contribute derived back-links, and ``.. entity-table::`` and
``.. entity-flow::`` list and draw it. There is no notion of an "external"
entity.

Options
~~~~~~~

.. list-table::
   :header-rows: 1

   * - option
     - what it does
   * - *(argument)*
     - the ``needs.json`` to read, relative to the document, or to the source root with a leading ``/``
   * - ``:version:``
     - which version block to import; defaults to the file's ``current_version``, or to its sole version
   * - ``:ids:``
     - import only the needs named, as a comma-separated list
   * - ``:filter:``
     - import only the needs matching, in the same filter language a listing directive uses
   * - ``:id_prefix:``
     - prepend a prefix to every imported id, rewriting the links *within this import* to match
   * - ``:tags:``
     - add these values to each imported entity's ``tags`` attribute

What the schema decides
~~~~~~~~~~~~~~~~~~~~~~~

The schema is the authority over the file, not the other way round:

- A need's ``type`` must name a declared entity type; one that does not is
  reported and skipped.
- Its other fields must be attributes or relations that type declares. Values
  go through the same validation a written ``:option:`` does, so an ``enum``
  cannot accept something when imported that it would refuse when typed.
- sphinx-needs' own bookkeeping keys are ignored by design — ``docname``,
  ``lineno``, ``is_need``, ``is_external``, ``sections``, ``parent_needs``, ``layout``,
  ``constraints*``, the whole ``type_*`` family and some thirty more. A field that
  is neither bookkeeping nor declared is **reported**, not dropped, since the
  likeliest cause is a project attribute the schema has not declared yet.
- A need's body comes from its ``content`` field, parsed as reStructuredText
  into the unnamed content section. Named sections cannot be imported: the
  format has no way to express one.
- **An unset field is silent.** A ``needs.json`` writes every registered option
  for every need, most of them empty, so an undeclared field with an empty
  value or an explicit ``null`` is ignored rather than reported — the author
  wrote neither. A field carrying a real value is still reported. A back-link
  this project *derives* is ignored for the same reason: the incoming side is
  computed project-wide, so a file's stored copy of it is discarded.

Importing by name
~~~~~~~~~~~~~~~~~

An argument may be a *name* instead of a path, declared in the schema's
top-level ``[import_keys]`` table — sphinx-needs spells the same thing
``needs_import_keys`` in its ``conf.py``:

.. code-block:: toml

   [import_keys]
   upstream_platform = "needs.json"

.. code-block:: rst

   .. needimport:: upstream_platform

It lives in ``entities.toml`` rather than in the build file because the alias is
written in a *document*: a reader needs the map to understand the line, and it
should travel with the documents. A value resolves like every other written
path here — relative to the schema file, or to the source root with a leading
``/`` — so a schema and the data files beside it move together.

The name is looked up *before* the argument is treated as a path, which is the
order sphinx-needs resolves in, so a key may be spelled with a ``.json`` suffix
and still win over a file of that name.

Build wiring
~~~~~~~~~~~~

The file is ordinary ``parse_data`` — a file the parser opens, like the ``.csv``
behind a ``.. csv-table::``'s ``:file:``, not a dependency on another library. A
file named through ``[import_keys]`` needs its entry too: a config file can say
what a name means, but only the build system can put the file in the parse
action's sandbox.

.. code-block:: python

   rinx_library(
       name = "docs",
       srcs = glob(["*.rst"]),
       entity_schema = "entities.toml",
       parse_data = ["needs.json"],
   )

There is no cache firewall, as there is none for ``.. include::``: editing the
``needs.json`` re-parses and re-renders every document that imports it, because
the needs really are part of those documents.

What it does not do
~~~~~~~~~~~~~~~~~~~

- **Nothing is fetched.** sphinx-needs accepts an ``http``/``https`` URL; a
  sandboxed build action may only read files declared before it runs, so a URL
  is refused by name.
- **A name matching no declared import key** is refused by name rather than
  opened as a file, so a forgotten ``[import_keys]`` entry warns where a
  forgotten ``parse_data`` entry fails the build.
- ``:hide:``, ``:collapse:``, ``:layout:`` and ``:style:`` are refused by name.
  Presentation comes from the schema's per-type ``template`` and the render-time
  site config.
- ``:setup:``, ``:pre_template:`` and ``:post_template:`` are refused by name.
  They run Python.
- **A ``.. noqa:`` cannot silence a diagnostic from imported prose**, which
  carries no position — JSON has no line to point at. The ``needimport.*``
  diagnostics themselves are reported against the directive's own line and
  suppress normally.

Diagnostics
-----------

Every one of these carries a source position and a stable code a ``.. noqa:`` can
name.

**While parsing**, with the position of the offending line:

.. list-table::
   :header-rows: 1

   * - code
     - when
   * - ``entity.unknown-attribute``
     - an option the type declares as neither attribute nor relation
   * - ``entity.invalid-attribute-value``
     - a value that does not fit its declared type
   * - ``entity.missing-required-attribute``
     - a ``required`` attribute left unset
   * - ``entity.malformed-argument``
     - an argument a type takes none of, or too many comma parts
   * - ``entity.invalid-id``
     - an illegal ``:id:``, or one that could not be derived
   * - ``entity.id-pattern-mismatch``
     - a legal id outside its type's ``id.pattern``; the entity keeps it
   * - ``entity.unknown-section``
     - a sub-directive this type does not declare
   * - ``entity.section-outside-entity``
     - a section directive written outside any entity
   * - ``entity.duplicate-section``
     - a section repeated without ``multiple``
   * - ``entity.missing-required-section``
     - a ``required`` section not written
   * - ``entity.missing-required-relation``
     - a ``required`` relation with no target
   * - ``entity.multiple-relation-targets``
     - several targets on a single-target relation

And, for a ``.. needimport::``, reported against the directive's own line, since
a ``needs.json`` has no line of its own to name:

.. list-table::
   :header-rows: 1

   * - code
     - when
   * - ``needimport.missing-path``
     - no file to import
   * - ``needimport.remote-source``
     - an ``http``/``https`` argument, which is never fetched
   * - ``needimport.unsupported-import-key``
     - an argument that is not a path to a ``.json`` file, such as a ``needs_import_keys`` alias
   * - ``needimport.file-unreadable``
     - the file could not be read; usually a missing ``parse_data`` entry
   * - ``needimport.malformed-json``
     - the file is not a readable ``needs.json``
   * - ``needimport.unknown-version``
     - no single version block could be chosen
   * - ``needimport.unknown-type``
     - a need whose ``type`` names no declared entity type
   * - ``needimport.unknown-field``
     - a field that is neither bookkeeping nor declared
   * - ``needimport.invalid-value``
     - a JSON value no option could have been written with
   * - ``needimport.invalid-id``
     - a need whose id is missing or illegal
   * - ``needimport.unknown-id``
     - an ``:ids:`` entry the file does not hold
   * - ``needimport.invalid-filter``
     - a ``:filter:`` the filter language cannot parse
   * - ``needimport.unknown-filter-field``
     - a ``:filter:`` naming an undeclared field
   * - ``needimport.unsupported-option``
     - one of sphinx-needs' options this build refuses
   * - ``needimport.unknown-option``
     - an option this directive does not accept
   * - ``needimport.no-tags-attribute``
     - ``:tags:`` on a type declaring no list attribute ``tags``
   * - ``needimport.empty-result``
     - an import that selected no need at all

And, for an ``.. entity-sequence::``, against the offending option line — or the
directive's own line for a missing option:

.. list-table::
   :header-rows: 1

   * - code
     - when
   * - ``entity-sequence.missing-start``
     - no ``:start:``, or one listing nothing; the directive becomes an error block
   * - ``entity-sequence.missing-relations``
     - no ``:relations:``/``:link_types:``; the message lists the declared ones
   * - ``entity-sequence.invalid-start``
     - a ``:start:`` entry that is not an entity id
   * - ``entity-sequence.unknown-relation``
     - a ``:relations:`` entry no type declares
   * - ``entity-sequence.invalid-filter``
     - a ``:filter:`` the filter language cannot parse
   * - ``entity-sequence.unknown-field``
     - a ``:filter:`` naming an undeclared field
   * - ``entity-sequence.invalid-max-items``
     - a ``:max-items:`` that is not a non-negative whole number
   * - ``entity-sequence.unsupported-option``
     - one of sphinx-needs' options this build refuses

A ``.. needservice::`` is refused outright, reported against its own line:

.. list-table::
   :header-rows: 1

   * - code
     - when
   * - ``needservice.unsupported``
     - any ``.. needservice::``; the message names the ``needs.json`` + ``.. needimport::`` route

**While building the index**, attributed to the document that wrote the source
entity:

.. list-table::
   :header-rows: 1

   * - code
     - when
   * - ``entity.duplicate-id``
     - two entities claiming one id
   * - ``entity.unknown-target``
     - a relation naming an entity no document declares
   * - ``entity.disallowed-relation``
     - a target whose type is outside the relation's ``to``
   * - ``entity.schema-mismatch``
     - a document parsed against a different schema

**While rendering:**

.. list-table::
   :header-rows: 1

   * - code
     - when
   * - ``entity.role-type-mismatch``
     - a role resolving to a type it does not accept
   * - ``entity-flow.empty-result``
     - a flowchart whose filter matched no entity
   * - ``entity-flow.unknown-config``
     - a ``:config:`` naming no declared preamble
   * - ``entity-sequence.unknown-start``
     - a ``:start:`` entry naming no entity; the others are still walked
   * - ``entity-sequence.empty-result``
     - a walk that found no message
   * - ``entity-sequence.truncated``
     - a walk ``:max-items:`` cut short; the picture is still drawn
   * - ``entity-sequence.unknown-config``
     - a ``:config:`` naming no declared preamble

A faulty schema is not a diagnostic but a hard error: it is the vocabulary the
parser works from, so continuing would report a cascade of unknown-directive
messages instead of the one real problem. Every fault in the file is listed at
once.

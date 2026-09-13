Importing from sphinx-needs
===========================

``.. needimport::`` reads a sphinx-needs ``needs.json`` and turns every need
in it into an entity of *this* document. It is a migration bridge and nothing
more: it keeps an existing project's import lines working while the rest of
its documents move over. This build's own name, ``entity-import``, is
deliberately left unclaimed for a richer construct later — see
``docs/decisions/016-needimport.md``.

The file it reads is ordinary ``parse_data``, like the source behind a
``.. csv-table::``'s ``:file:``. The name resolves relative to the document,
or to the source root with a leading ``/``. Nothing is ever fetched: an
``http``/``https`` argument is refused by name, since a sandboxed build action
may only read files declared before it runs.

Importing a whole file
----------------------

``needs.json`` beside this document is an export from an imaginary upstream
platform project. With no options, every need of its ``current_version``
becomes an entity here — typed, indexed, cross-referenceable and rendered
exactly like one written by hand.

.. needimport:: needs.json

The types the needs claim — ``req`` and ``spec`` — have to be declared in this
project's own ``entities.toml``; the schema is the authority over the file,
not the other way round. sphinx-needs' own bookkeeping keys (``docname``,
``lineno``, ``is_need``, ``type_color`` and their family) are ignored by
design, while a field that is neither bookkeeping nor a declared attribute or
relation is reported rather than dropped.

Imported entities are ordinary entities
---------------------------------------

Nothing downstream of the parser knows an import happened, which is the whole
point of reading the file while parsing. An imported entity is a ``:ref:``
target like any other — :ref:`PLAT_REQ_1` — and its relations join the project
graph, so the back-link on ``PLAT_SPEC_1`` below was derived from an edge that
arrived in a JSON file.

.. entity-table::
   :columns: id, title, status, linked_by
   :filter: id == "PLAT_REQ_1" or id == "PLAT_SPEC_1"

Importing by name
-----------------

An argument may also be a *name* rather than a path, which ``entities.toml``
maps to a file in its ``[import_keys]`` table — sphinx-needs spells the same
thing ``needs_import_keys`` in its ``conf.py``. The alias is written in the
document, so its meaning belongs with the project's vocabulary rather than in
the build file, and it travels with the documents.

.. needimport:: upstream_platform
   :ids: PLAT_SPEC_1
   :id_prefix: BYKEY_

The name is looked up *before* the argument is treated as a path, which is the
order sphinx-needs resolves in, so a key may be spelled with a ``.json``
suffix and still win over a file of that name. A name matching no declared key
is reported rather than opened as a file — a missing declaration should not
fail the build the way an undeclared ``parse_data`` entry does. The file the
key names still needs its ``parse_data`` entry; here ``needs.json`` already
has one.

Choosing a version
------------------

A ``needs.json`` holds one block per version and names a ``current_version``.
``:version:`` takes a different one. Version ``1.0`` of this file holds a
single, retired requirement under the same id as ``2.0``'s first — so it is
imported here under a prefix, which the next section explains.

.. needimport:: needs.json
   :version: 1.0
   :id_prefix: OLD_

Narrowing what is imported
--------------------------

``:ids:`` imports only the needs it names.

.. needimport:: needs.json
   :ids: PLAT_REQ_2
   :id_prefix: BYID_

``:filter:`` is the same typed filter language an ``.. entity-table::`` and an
``.. entity-flow::`` are written in, so a question asked of the graph means
the same thing wherever it is asked. It runs against the need as the schema
will store it, which is why ``"resilience" in tags`` matches whether the file
spelled the tags as a JSON array or as a comma-separated string.

.. needimport:: needs.json
   :filter: "resilience" in tags
   :id_prefix: BYFILTER_

Ids the project does not own
----------------------------

``:id_prefix:`` prepends a prefix to every imported id, which is what makes
the three imports above able to coexist with the first one. It also rewrites
the relation targets that point at another need *in the same import*, so a
prefixed copy links to its own siblings — and leaves a target naming anything
else alone, since that is a reference to an entity the project already holds
under its own id.

An id that collides with an entity the project already declares is reported as
``entity.duplicate-id``, the same diagnostic two hand-written entities would
get. There is no separate notion of an "external" entity.

Tagging an import
-----------------

``:tags:`` adds values to the imported entities' ``tags`` attribute. Nothing
in this model privileges that name: it works because ``req`` declares a
``list<string>`` attribute called ``tags``, and a type that declares no such
attribute is reported rather than having the values quietly dropped.

.. needimport:: needs.json
   :ids: PLAT_REQ_2
   :id_prefix: TAGGED_
   :tags: imported, upstream

What is not supported
---------------------

``:hide:``, ``:collapse:``, ``:layout:`` and ``:style:`` are presentation,
which this build takes from the schema's per-type ``template`` and the
render-time site config rather than from an import line. ``:setup:``,
``:pre_template:`` and ``:post_template:`` run Python, which this build does
not. Each is refused *by name*, with what to reach for instead, rather than
being ignored.

Named sections cannot be imported either: a ``needs.json`` has no way to
express one, so an imported entity's body is whatever its ``content`` field
holds, parsed as reStructuredText into the unnamed content section. A
diagnostic from inside that prose carries no position, because JSON has no
line for it to point at.

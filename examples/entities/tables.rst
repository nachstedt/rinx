Listing entities
================

A ``.. entity-table::`` asks the entity graph a question and renders the
answer. Its rows come from the project index rather than from this document,
so a table lists entities declared anywhere in the site — including the
requirements and audit events on the neighbouring pages.

``.. needtable::`` is accepted as a second spelling of the same directive, so
a project migrating from sphinx-needs keeps its documents unchanged.

Every entity in the project
---------------------------

With no ``:filter:``, a table lists everything. The default columns are the
three fields every entity has, whatever its type declares.

.. entity-table::

Choosing columns
----------------

``:columns:`` names the fields to show, in the order to show them. A column
may name a built-in field, an attribute, an outgoing relation, or a back-link
derived from one — ``verified_by`` is declared nowhere, and falls out of
``test.verifies`` pointing at a requirement.

.. entity-table::
   :columns: id, title, status, owner, verified_by
   :filter: type == "req"

Headings come from the schema's own labels, so a column is never a second copy
of a name that can drift from it.

Filtering
---------

A filter is a small typed expression language with Python's spelling. It
supports ``==``, ``!=``, ``in``, ``not in``, ``is None``, ``is not None``,
``and``, ``or``, ``not`` and parentheses, over field names, strings, whole
numbers and ``True``/``False``.

``in`` means what it means in Python: a substring test over text, and a
membership test over a list.

.. entity-table::
   :columns: id, title, tags
   :filter: "boot" in tags

Conditions combine, and parentheses group them:

.. entity-table::
   :columns: id, title, status
   :filter: type == "req" and (status == "open" or status == "in_progress")

A filter may also test whether a field was written at all:

.. entity-table::
   :columns: id, name, version
   :filter: type == "audit-event" and version is not None

Anything outside this language is reported by name — ``len(tags) > 0`` is
refused as "function calls are not supported here", pointing at ``len``,
rather than silently matching nothing.

Sorting and layout
------------------

``:sort:`` orders rows by one field; without it, rows are in id order, which
keeps a rendered page byte-identical between builds. The presentation options
every other table directive takes work here too, and sphinx-needs'
``:colwidths:`` is accepted as a spelling of ``:widths:``.

.. entity-table::
   :columns: id, title, status
   :filter: type == "req"
   :sort: title
   :colwidths: 20, 50, 30
   :name: requirements-by-title

That table registers an ordinary target, so :ref:`requirements-by-title`
links to it.

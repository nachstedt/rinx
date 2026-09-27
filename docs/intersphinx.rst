.. _intersphinx:

Linking between documentation sites
===================================

Every ``rinx_site`` writes an ``objects.inv``, Sphinx's inventory of the
targets it defines, and can link into other sites through theirs — Sphinx's
intersphinx, with one difference: an inventory is a **pinned build input**,
never downloaded while building. See ``docs/decisions/023-inventories.md`` for
why.

Linking into another site
-------------------------

Declare the inventory, then list it on the site:

.. code-block:: python

   load("@rinx//:defs.bzl", "rinx_inventory", "rinx_site")

   rinx_inventory(
       name = "python",
       src = "@python_objects_inv//file",
       base_url = "https://docs.python.org/3.13/",
   )

   rinx_site(
       name = "site",
       deps = [":docs"],
       inventories = [":python"],
   )

and fetch the file with a checksum in ``MODULE.bazel``:

.. code-block:: python

   http_file = use_repo_rule("@bazel_tools//tools/build_defs/repo:http.bzl", "http_file")

   http_file(
       name = "python_objects_inv",
       url = "https://docs.python.org/3.13/objects.inv",
       sha256 = "…",
   )

Pin a **versioned** URL. An unversioned one (``/3/objects.inv``) is republished
with every docs build, so its checksum stops matching; bumping the version is
then a deliberate change: update the URL, ``base_url`` and ``sha256`` together.
Vendoring the file in the repository works just as well, and is what
``examples/intersphinx/`` does.

The target's name is the inventory's name in documents (Sphinx's
``intersphinx_mapping`` key); set ``inventory_name`` to use another.

Writing references
------------------

Nothing has to change: a role searches this site first and then every declared
inventory, in the order ``inventories`` lists them. The one exception is
``:doc:``, which — as in Sphinx, whose ``intersphinx_disabled_reftypes`` leaves
``std:doc`` out by default — searches the inventories only when written with
the ``:external:`` prefix below.

.. code-block:: rst

   :py:class:`dict`, :func:`len`, :term:`bytecode`, :option:`-O`
   :ref:`tut-intro`                  — shows the title Python gives the label
   :ref:`the tutorial <tut-intro>`   — an explicit title still wins
   :any:`dict`                       — any entry type; the first inventory listing it wins

To pick an inventory, or to skip this site, as in Sphinx:

.. code-block:: rst

   :ref:`python:tut-intro`             — a `name:` prefix, tried after the target as written
   :external:py:class:`dict`           — any inventory, never this site
   :external+python:ref:`tut-intro`    — the `python` inventory only
   :external:doc:`tutorial/index`      — a page, named as the inventory lists it

``:external:`` works on ``:ref:``, ``:doc:``, ``:term:``, ``:option:``,
``:any:`` and every domain role. An external ``:doc:`` target is the other
site's document name exactly as written — never relative to the page it is
written on, and without a leading ``/``.

.. list-table::
   :header-rows: 1

   * - Code
     - Meaning
   * - ``link.broken-ref``, ``link.broken-object``, …
     - Found neither here nor in any inventory searched — the role's usual code.
   * - ``link.unknown-inventory``
     - ``:external+name:`` names an inventory the site does not declare.

Both are ordinary render warnings: ``.. noqa:`` silences them and
``strict_links = True`` makes them fatal. A file that is not an inventory, or two
inventories with one name, fails the build instead, since no document could
have caused it.

Letting other sites link into yours
-----------------------------------

Nothing to configure: ``objects.inv`` is written at the site's root. A Sphinx
project lists it in ``intersphinx_mapping`` like any other. Another
``rinx_site`` can depend on it without building the pages, through the
``inventory`` output group:

.. code-block:: python

   filegroup(
       name = "docs_objects_inv",
       srcs = ["//docs:site"],
       output_group = "inventory",
   )

   rinx_inventory(
       name = "docs",
       src = ":docs_objects_inv",
       base_url = "../docs/",   # relative to this site's root when deployed side by side
   )

``examples/intersphinx/BUILD.bazel``'s ``sibling_site`` is exactly this.

What the inventory lists
------------------------

Documents (``std:doc``), every internal label (``std:label``, with the title a bare
``:ref:`` would show), glossary terms, options, Python and C objects, and the
general index. Unlike Sphinx it also lists labels without a title, so an
entity or a named directive can be linked from another site; like Sphinx it
lists no equations.

Live preview
------------

The preview resolves against the last built index, which holds only the
external targets some document referenced when it was built. A reference you
have just written to an external target nothing else names stays unresolved in
the preview until the next build.

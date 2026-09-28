.. _rules:

Bazel rules
===========

The rules are loaded from ``@rinx//:defs.bzl``:

.. code-block:: python

   load("@rinx//:defs.bzl", "rinx_doctest_tests", "rinx_inventory", "rinx_library", "rinx_site")

They work like ``cc_library`` and ``cc_binary``. A ``rinx_library`` parses a
set of documents, and a ``rinx_site`` assembles libraries into one HTML site.
Each document is parsed and rendered by an action of its own, so Bazel rebuilds
only what an edit affects.

rinx_library
------------

Parses a set of reStructuredText documents. Its output is a parsed form of each
document that a ``rinx_site`` renders, not HTML.

.. list-table::
   :header-rows: 1

   * - Attribute
     - Description
   * - ``srcs``
     - The ``.rst`` documents this library owns. Each becomes a page of the
       site, at its path relative to the package.
   * - ``deps``
     - Other ``rinx_library`` targets whose documents this library's
       toctrees include. Only toctrees need this: cross-references and links
       between documents are resolved across the whole site without it. A
       toctree naming a document that no library in ``deps`` provides fails
       the build.
   * - ``parse_data``
     - Files the documents read while they are parsed: the file behind a
       ``.. csv-table:: :file:``, the sources ``.. include::`` and
       ``.. literalinclude::`` splice in, and the templates a Jinja
       ``{% include %}`` reads. A path resolves relative to the file the
       directive is written in, or to the source root with a leading ``/``.
       An ``.rst`` file included this way must not also be in ``srcs``, or it
       is published as a page of its own too.
   * - ``images``
     - The pictures ``.. image::`` and ``.. figure::`` show. They are copied
       into the site's ``_images/`` directory. An image a document shows but
       that is not declared here fails the site's build.
   * - ``downloads``
     - The files the ``:download:`` role links. They are copied into the
       site's ``_downloads/`` directory, keeping their path from the workspace
       root, and are read by nothing else, so editing one re-renders no page.
       A file a document links but that is not declared here fails the site's
       build as ``download.undeclared``.
   * - ``diagrams``
     - Whether the documents may hold PlantUML diagrams (``.. uml::``,
       ``.. plantuml::`` and the entity diagrams). Off by default, so a library
       without diagrams pays nothing for them. A diagram in a library without
       it fails the build, naming this attribute. Keep diagram documents in a
       library of their own to keep the cost on them.
   * - ``entity_schema``
     - The project's entity schema, which declares directives such as
       ``.. req::``; see :ref:`entities`. Every library of a site and the site
       itself must name the same file.
   * - ``jinja``
     - Whether the ``.rst`` sources are rendered as Jinja templates before they
       are parsed, the transform a Sphinx project gets from a ``source-read``
       hook. Off by default, since ``{{`` and ``{%`` are ordinary text in most
       documents. A document holding no Jinja delimiter is not rendered at all.
   * - ``jinja_context``
     - A dictionary of names a Jinja-rendered document may read, like Sphinx's
       ``html_context``. A name that nothing binds is reported rather than
       rendered as the empty string.
   * - ``default_domain``
     - The domain unprefixed directives and roles such as ``.. function::``
       belong to: ``"py"`` (the default) or ``"c"``.

rinx_site
---------

Assembles a site from ``rinx_library`` targets: it indexes every document the
libraries bring in (transitively), renders one page per document, compiles the
diagrams, and copies the images, the downloads and the stylesheet. The output is the directory
``<name>_site_out``. The site also writes an ``objects.inv``, the inventory
other Sphinx or rinx sites link into it through, available as the target's
``inventory`` output group.

.. list-table::
   :header-rows: 1

   * - Attribute
     - Description
   * - ``deps``
     - The ``rinx_library`` targets of the site.
   * - ``config``
     - A ``rinx.toml`` with the site's metadata; see :ref:`site-config`
       below. Defaults to one naming the project "Documentation".
   * - ``template``
     - The MiniJinja HTML template every page is rendered into. Defaults to the
       bundled one, whose sidebar starts collapsed except for the path to the
       current page; every other branch unfolds with its chevron.
   * - ``css``
     - The stylesheet copied into the site. Defaults to the bundled one.
   * - ``entity_schema``
     - The same schema file the libraries name, if they name one.
   * - ``entity_templates``
     - Per-type HTML templates a schema's ``template = "name.html"`` refers to
       by basename. Omit it for the built-in rendering.
   * - ``inventories``
     - ``rinx_inventory`` targets: other sites this one links into. See
       :ref:`intersphinx`.
   * - ``strict_links``
     - If ``True``, a broken cross-reference fails the build instead of only
       printing a warning. Off by default.

.. _site-config:

The configuration file
~~~~~~~~~~~~~~~~~~~~~~

The site's ``config`` is a TOML file holding metadata only. It never contains
paths: those are attributes of the rule, so that Bazel can move files around
freely.

.. code-block:: toml

   project = "My Project"
   version = "1.2"
   root_doc = "index"
   highlight_language = "python"

   numfig = true

   [uml_configs]
   monochrome = "skinparam monochrome true"

   [numfig_format]
   figure = "Figure %s"

.. list-table::
   :header-rows: 1

   * - Key
     - Meaning
   * - ``project``
     - The name shown in the sidebar and in each page's title.
   * - ``version``
     - The version shown in the sidebar.
   * - ``root_doc``
     - The document every other one hangs off, without ``.rst``. Defaults to
       ``index``.
   * - ``highlight_language``
     - The language of code blocks that name none, like Sphinx's setting of
       the same name. A name that matches no language fails the build.
   * - ``collapse_entities``
     - Whether an entity's details are folded behind a disclosure. On by
       default.
   * - ``show_entity_updates``
     - Whether ``.. entity-update::`` renders a visible box of the changes it
       makes. On by default.
   * - ``numfig``
     - Whether captioned figures, tables and code blocks are numbered, and
       ``:numref:`` can show those numbers. Off by default, as in Sphinx.
   * - ``numfig_secnum_depth``
     - How many levels of the section number a figure's number starts with,
       under a ``:numbered:`` toctree. Defaults to 1, so the third figure of
       chapter 2 is ``2.3``; ``0`` numbers straight through the site.
   * - ``numfig_format``
     - A table of the text each kind of number is shown in, with the keys
       ``figure``, ``table``, ``code-block`` and ``section``. Defaults to
       Sphinx's ``Fig. %s``, ``Table %s``, ``Listing %s`` and ``Section %s``.
       ``{number}`` works in place of ``%s``; ``{name}`` is refused here, since
       the same text is written in front of the caption it would name.
   * - ``uml_configs``
     - Named PlantUML preambles a diagram selects with ``:config:``.
   * - ``version_switcher``
     - A table whose ``json_url`` lists the site's published versions. See
       :ref:`version-switcher`.

.. _version-switcher:

Publishing several versions
~~~~~~~~~~~~~~~~~~~~~~~~~~~

A site published in several versions side by side, for example one directory
per release plus one for the development branch, can show a menu of those
versions in the sidebar. Every page other than the preferred version also gets
a banner that links to the same page in the preferred version:

.. code-block:: toml

   [version_switcher]
   json_url = "https://example.org/docs/versions.json"

``json_url`` is fetched by the browser. It must be an ``http(s)://`` URL or
start with ``/``. The file lists the versions in pydata-sphinx-theme's format,
so a tool that writes one for that theme writes one for this:

.. code-block:: json

   [
     {"version": "v1.2.0", "name": "v1.2.0 (latest)",
      "url": "https://example.org/docs/v1.2.0/", "preferred": true},
     {"version": "main", "name": "main (development)",
      "url": "https://example.org/docs/main/"},
     {"version": "v1.1.0", "url": "https://example.org/docs/v1.1.0/"}
   ]

A page finds its own version by its address: the entry whose ``url`` is the
longest prefix of it. So the build never needs to be told which version it is,
and the same commit built for two directories produces identical pages. A page
under no listed ``url``, such as a pull-request preview, is labelled a preview
build. Switching versions keeps the current page, and goes to the version's
root when that page does not exist there.

The banner depends on the version being read. On the entry marked
``preferred`` it is absent. On a version whose name starts with a number, such
as ``v1.1.0``, it says that is an older release. On any other version, such as
``main``, it says that is the development version.

``rinx_site`` writes ``version_switcher.js`` next to ``default.css`` for the
default template. A custom template can use the ``version_switcher`` variable
the same way: it holds ``json_url`` and ``script``, the script's path relative
to the page. When ``versions.json`` cannot be fetched, which is the case for a
build opened from disk, the menu stays hidden.

rinx_inventory
--------------

Declares another site's ``objects.inv`` that a ``rinx_site`` may link into,
Sphinx's intersphinx. Nothing is downloaded while building: the file is
vendored, pinned by an ``http_file`` with a ``sha256``, or produced by another
``rinx_site``. See :ref:`intersphinx`.

.. list-table::
   :header-rows: 1

   * - Attribute
     - Description
   * - ``src``
     - The ``objects.inv`` file. Required.
   * - ``base_url``
     - Where that site's pages are published: an absolute URL, or a path
       relative to the consuming site's root for a site deployed next to it.
       Required.
   * - ``inventory_name``
     - The name documents select this inventory by, as in
       ``:external+python:ref:`tut```. Defaults to the target's name.

rinx_doctest_tests
------------------

Runs the ``.. doctest::`` and ``.. testcode::`` blocks of one library's
documents as a Bazel test, on the Python toolchain rinx brings along. Building
a site never runs them; ``bazel test`` does. A test's result stays cached until
the code in the blocks changes. Prose edits do not re-run it.

.. code-block:: python

   rinx_doctest_tests(
       name = "doctests",
       lib = ":docs",
   )

.. list-table::
   :header-rows: 1

   * - Argument
     - Description
   * - ``name``
     - The name of the generated ``py_test``.
   * - ``lib``
     - The ``rinx_library`` whose documents are tested. Its ``deps`` are not
       tested; each library carries its own doctest target.
   * - ``py_deps``
     - ``py_library`` targets the documented code imports.
   * - ``global_setup``, ``global_cleanup``
     - A ``.py`` file run before or after every group, like Sphinx's
       ``doctest_global_setup`` and ``doctest_global_cleanup``.
   * - ``size``, ``tags``, …
     - Passed on to the generated ``py_test``.

Every document of a library runs in one interpreter, and a change to one
document's code re-runs the whole library's test. To narrow that, put the
documents with doctests in a library of their own.

.. _getting-started:

Getting started
===============

rinx runs as part of a Bazel build. You need Bazel 8.7 or later (preferably through
`Bazelisk <https://github.com/bazelbuild/bazelisk>`__) and a C compiler for
linking the Rust binary. Everything else, including the Rust, Java and Python
toolchains, is fetched by Bazel.

Adding rinx to a project
------------------------

Add rinx to your ``MODULE.bazel``:

.. code-block:: python

   bazel_dep(name = "rinx", version = "0.2.0")

To follow an unreleased commit instead, override it with the repository:

.. code-block:: python

   bazel_dep(name = "rinx", version = "0.2.0")
   git_override(
       module_name = "rinx",
       remote = "https://github.com/nachstedt/rinx.git",
       commit = "<commit sha>",
   )

A first site
------------

Write two documents. ``index.rst`` is the root of the site, and its toctree
pulls in the second one:

.. code-block:: rst

   Welcome
   =======

   An introduction, and a link to :ref:`the guide <guide>`.

   .. toctree::

      guide

.. code-block:: rst

   .. _guide:

   Guide
   =====

   Some prose, and a code block:

   .. code-block:: python

      print("Hello from rinx")

Then declare them in a ``BUILD.bazel`` next to them:

.. code-block:: python

   load("@rinx//:defs.bzl", "rinx_library", "rinx_site")

   rinx_library(
       name = "docs",
       srcs = ["index.rst", "guide.rst"],
   )

   rinx_site(
       name = "site",
       deps = [":docs"],
   )

``rinx_library`` parses its documents. ``rinx_site`` indexes everything its
libraries bring in and renders one HTML page per document.

Building and viewing it
-----------------------

.. code-block:: bash

   bazel build //:site

The site is written to ``bazel-bin/site_site_out/``, where ``site`` is the
target's name. Open its ``index.html``, or serve the directory:

.. code-block:: bash

   python3 -m http.server --directory bazel-bin/site_site_out

A broken cross-reference is a warning by default. Set ``strict_links = True``
on the site to make it fail the build instead.

.. _init-from-sphinx:

From a Sphinx project
---------------------

An existing Sphinx project needs no hand-written build. In the directory
holding its ``conf.py``, run:

.. code-block:: bash

   rinx init --from-sphinx .

It writes the project's Bazel build from ``conf.py`` and from what the
documents actually use:

- ``BUILD.bazel``, with one ``rinx_library`` holding every document Sphinx
  would build — the ``.rst`` files ``exclude_patterns`` leaves out are
  excluded from its ``srcs`` glob — and the ``rinx_site`` rendering it. The
  library declares exactly the files the documents read (``parse_data``),
  show (``images``) and link (``downloads``), and ``diagrams = True`` when one
  draws a diagram.
- ``rinx.toml``, with the project's name, version, root document and the
  other site settings ``conf.py`` sets.
- ``rst_prolog.txt`` and ``rst_epilog.txt``, when ``conf.py`` sets them.
- ``entities.toml``, when the project uses sphinx-needs: its configuration,
  converted into an entity schema.
- ``MODULE.bazel``: the dependency on rinx is added to the enclosing Bazel
  module, or, outside any, the directory becomes a module of its own, so
  every document keeps the name and the address it has under Sphinx.

It never overwrites a file, and prints as JSON what it wrote and what it
left out: a setting ``conf.py`` computes rather than spells, an
``intersphinx_mapping`` entry (declare it as a ``rinx_inventory``), or a
file a document reads from outside the directory. ``--python <exe>`` runs
``conf.py`` for the settings it computes, ``--jinja`` renders every document
as a Jinja template first (what a ``source-read`` hook does, which reading
``conf.py`` cannot see), and ``--dry-run`` only reports. In VS Code, the
*Rinx: Migrate Project to rinx* command does the same (see
:ref:`vscode-migrate`).

Where to go next
----------------

- :ref:`rules` lists every attribute of the four rules. Among them are
  ``parse_data`` for the files ``.. include::`` reads, ``images``,
  ``diagrams = True`` for PlantUML, and doctests.
- :ref:`syntax` lists which reStructuredText and Sphinx constructs rinx
  understands.
- To split documentation between teams, give each team a ``rinx_library`` and
  list it in the ``deps`` of the library whose toctree includes it. The
  `example site <../example-site/examples/index.html>`__ is built exactly that way.
- To give the site its own name and look, use the site's ``config``,
  ``template`` and ``css`` attributes.

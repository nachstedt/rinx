.. _getting-started:

Getting started
===============

rinx runs as part of a Bazel build. You need Bazel 8 or 9 (preferably through
`Bazelisk <https://github.com/bazelbuild/bazelisk>`__) and a C compiler for
linking the Rust binary. Everything else, including the Rust, Java and Python
toolchains, is fetched by Bazel.

Adding rinx to a project
------------------------

Add rinx to your ``MODULE.bazel``:

.. code-block:: python

   bazel_dep(name = "rinx", version = "0.1.0")

To follow an unreleased commit instead, override it with the repository:

.. code-block:: python

   bazel_dep(name = "rinx", version = "0.1.0")
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

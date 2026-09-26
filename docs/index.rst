.. _home:

####
rinx
####

**rinx** is a Rust re-implementation of (a subset of) the `Sphinx
<https://www.sphinx-doc.org/>`__ documentation generator, built to be a
first-class `Bazel <https://bazel.build/>`__ build step rather than an external
tool wrapped by one. It turns reStructuredText into an HTML site with
cross-references, toctree navigation, syntax highlighting, math and diagrams,
and it models requirements and other project-specific constructs the way
`sphinx-needs <https://sphinx-needs.com/>`__ does.

This site is itself built by rinx, from the ``docs/`` directory of its
repository.

.. note::

   rinx is early and pre-1.0. The rule and CLI interfaces may still change
   between minor versions. :ref:`syntax` lists what is supported today.

Why
===

A Sphinx build is a single process: it re-reads the whole project and decides
for itself what is out of date. rinx splits the work into small, deterministic
steps (parse, index, render), each of them a separate Bazel action. So:

- **Only what changed is rebuilt.** Editing one page re-parses that page only.
  Results are cached like any other Bazel output, including remote caches.
- **Documentation composes like code.** A ``rinx_library`` per team or
  component, a ``rinx_site`` that assembles them, and dependency checking that
  fails the build when a toctree points at a library that isn't declared.
- **Fast.** CPython's documentation (about 500 documents) builds as a Bazel
  target; see :ref:`benchmark`.
- **Resilient parsing.** A half-written document still produces a page, which
  is what makes the :ref:`VS Code live preview <vscode>` possible.

Contents
========

.. toctree::
   :maxdepth: 2

   getting_started
   rules
   syntax
   entities
   intersphinx
   vscode
   benchmark

More
====

- The `example site <../example-site/examples/index.html>`__ is a multi-team project using
  every feature rinx supports, built from ``examples/`` in the repository.
- The `architecture decision records
  <https://github.com/nachstedt/rinx/tree/main/docs/decisions>`__ explain why
  rinx is built the way it is.
- The source, issues and discussions are on `GitHub
  <https://github.com/nachstedt/rinx>`__.

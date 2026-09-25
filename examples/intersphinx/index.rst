.. _intersphinx-example:

Linking into other sites
========================

This site declares one other site's inventory, Python's, in its
``inventories`` (see ``examples/intersphinx/BUILD.bazel``). A reference no
document here defines is then looked up in it — Sphinx's intersphinx, with
the ``objects.inv`` a pinned build input rather than something fetched while
building. Every link below leaves this site.

Ordinary roles fall back to the inventory
-----------------------------------------

Nothing has to be spelled differently: a role first searches this site and
then every declared inventory, in the order the site lists them.

* Python objects: :py:class:`dict`, :func:`len`, :exc:`ValueError`,
  :meth:`dict.get`, :mod:`os.path` and :attr:`object.__dict__`. The role's type
  still matters — :exc:`ValueError` finds an exception, as it would locally.
* A C function: :c:func:`PyObject_GetAttr`.
* A glossary term: :term:`bytecode`.
* A command-line option: :option:`-O`.
* A label, which shows the title *that* site gives it: :ref:`tut-intro`. An
  explicit title still wins: :ref:`the chapter on functions <tut-functions>`.

Naming the inventory in the target
----------------------------------

A ``name:`` prefix picks one inventory out, as in Sphinx — but only when the
target is not found as written, so a label that merely contains a colon still
works: :ref:`python:typesmapping`.

The ``:external:`` roles
------------------------

``:external:`` skips this site entirely, and ``:external+name:`` searches one
inventory alone. This site defines a label of its own named ``typesmapping``,
just below; the two roles here reach it and Python's respectively:

* :ref:`typesmapping` — this site's label wins, as any local target does.
* :external:ref:`typesmapping` — Python's, since this site is not searched.

The other roles take the prefix too: :external:py:class:`list`,
:external+python:py:func:`open`, :external:term:`iterable` and
:external+python:option:`-X`.

.. _typesmapping:

A local label that shadows Python's
-----------------------------------

This section exists so the two references above have something local to
disagree about.

An inventory that was never declared
------------------------------------

Naming an inventory the site does not declare is reported as
``link.unknown-inventory`` — the target is not the problem, so it is not
reported as a broken link. It is suppressed here, since ``//examples:site`` is
built with ``strict_links = True``:

.. noqa: link.unknown-inventory

:external+numpy:py:class:`ndarray` names ``numpy``, which this site never
declared.

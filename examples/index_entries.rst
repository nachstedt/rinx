Index Entries
=============

This page demonstrates the ``.. index::`` directive, which adds entries to
the site-wide general index (``genindex.html``, linked from the sidebar).
Domain objects (see the Domains page) are automatically added to the same
index — nothing extra is needed for those.

Single Entries
---------------

.. index:: single: execution

A *single* entry adds one top-level term to the index.

Single Entries with a Subentry
-------------------------------

.. index:: single: execution; context

A subentry nests under its parent term in the rendered index.

Pair Entries
------------

.. index:: pair: loop; statement

A *pair* entry is shorthand for two reciprocal single entries: "loop;
statement" and "statement; loop".

Triple Entries
--------------

.. index:: triple: module; import; package

A *triple* entry expands into three entries, each pairing one value against
the other two.

Comma-Shorthand Entries
------------------------

.. index:: BNF, grammar, syntax

Multiple single entries can be declared on the directive's argument line,
comma-separated, without repeating ``single:`` for each.

Main Entries
------------

.. index:: ! single: Python

Prefixing a whole entry with ``!``, before the entry type, marks it as the
"main" definition, which is emphasized in the rendered index.

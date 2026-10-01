Index Entries
=============

This page demonstrates the ``.. index::`` directive and the ``:index:`` role,
which add entries to the site-wide general index (``genindex.html``, linked
from the sidebar).
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

See and See-Also Entries
------------------------

.. index::
   see: goto; loop
   seealso: iteration; loop

A *see* entry sends the reader from one term to another instead of linking to
this page: the index lists "see loop" under *goto*. A *seealso* entry does the
same alongside the term's own links, as "see also loop" under *iteration*.

The ``:index:`` Role
--------------------

The role indexes the place it is written and shows its text there, unlinked.
A bare target is one single entry, so :index:`interpreter` is listed as
*interpreter*, and :index:`interpreter; startup` nests *startup* under it —
the text still reads exactly as written. A comma does not split a bare
target: :index:`spam, eggs` is one term. A leading ``!`` marks the
:index:`!main entry` and is not shown.

With an explicit title the target is one ``.. index::`` line, so the role can
write any entry type: :index:`loops and statements <pair: loop; statement>`
indexes a pair, :index:`jumping around <see: jump; goto>` a redirect, and
:index:`breakfast <spam, !eggs>` two comma-separated terms, the second main.

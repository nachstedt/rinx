Simple Tables
=============

Simple tables trade the grid table's ``+---+`` scaffolding for whitespace:
runs of ``=`` on the top and bottom borders mark out the columns, and text
lines up underneath them. They are less capable than grid tables — a cell
cannot span rows — but far easier to type and to edit.

Because a simple table's border always has *two or more* ``=`` runs, it can
never be confused with a section underline, which is a single unbroken run.

Table with a Header
-------------------

An interior ``=`` rule separates the header rows from the body:

=====  =====================
Fruit  Notes
=====  =====================
Apple  Crisp, keeps well.
Pear   Best eaten the day it
       is picked.
=====  =====================

The "Pear" row shows a cell continued onto a second line: a line whose
first column is blank belongs to the row above it.

Table Without a Header Row
--------------------------

Leave out the interior rule and every row is a body row:

=======  =======
verbose  false
retries  3
timeout  30s
=======  =======

Cells as Miniature Documents
----------------------------

A cell holds block content, not just a phrase — lists, literal blocks and
inline markup all work, and a blank line inside a cell separates blocks the
same way it does anywhere else:

=====  ============================
Row    Contents
=====  ============================
1      A cell with *emphasis*, a
       ``literal``, and a link to
       :ref:`home-index`.
2      - A bullet list inside a
         cell.

       - Its second item.
=====  ============================

Cells Spanning Columns
----------------------

A line of ``-`` underneath a row merges the columns it covers, so the row
above the underline becomes a single wide cell:

=====  =====  ======
  A      B    A or B
=====  =====  ======
False  False  False
True   False  True
A span across all three columns
--------------------
False  True   True
=====  =====  ======

The Rightmost Column Is Unbounded
---------------------------------

Text in the last column may run past the border — only the columns to its
left have to stay inside their margins:

=====  ====
Short  Wide
=====  ====
1      This sentence is considerably longer than the border above it.
2      So is this one.
=====  ====

.. _grid-index:

################
Grid Directives
################

``.. grid::`` is sphinx-design's responsive row and ``.. grid-item::`` is one
cell of it. Like ``.. dropdown::`` neither is part of docutils or Sphinx, and
like it they earn their place by *containing* things: an unknown directive
never parses its body, so before these existed everything written inside a grid
— prose, targets, entities, diagrams — was invisible to every later phase.

A grid's argument says how many of twelve columns the row splits into. One
value applies at every breakpoint; four are read as *xs sm md lg*:

.. grid:: 1 1 2 2
   :gutter: 3

   .. grid-item::
      :outline:
      :padding: 3

      One column on a narrow screen, two once there is room. Resize the window
      and the two cells stack.

   .. grid-item::
      :outline:
      :padding: 3

      Each cell's body is ordinary block content, so it can hold anything a
      document can — including the directives on the pages below.

.. toctree::

   columns
   spacing
   alignment
   nesting

Columns
=======

"Columns" shows the row's own count and a cell's ``:columns:`` override. A
twelve-column row with one cell spanning four of them:

.. grid:: 12
   :gutter: 2

   .. grid-item::
      :columns: 4
      :outline:

      ``:columns: 4``

   .. grid-item::
      :columns: 8
      :outline:

      ``:columns: 8`` — the two together fill the row.

Spacing
=======

"Spacing" shows ``:gutter:``, ``:margin:`` and ``:padding:``. The gutter is the
space *between* cells and takes the same one-or-four breakpoint values the
column counts do:

.. grid:: 3
   :gutter: 5
   :outline:
   :padding: 2

   .. grid-item::
      :outline:

      ``:gutter: 5``

   .. grid-item::
      :outline:

      is the widest

   .. grid-item::
      :outline:

      step there is.

Alignment
=========

"Alignment" shows ``:child-direction:`` and ``:child-align:``, which govern how
a cell lays *its own* content out, and ``:reverse:``, which lays the row out
right to left:

.. grid:: 2
   :gutter: 3
   :reverse:

   .. grid-item::
      :outline:
      :child-align: center

      Written first, drawn second.

   .. grid-item::
      :outline:
      :child-align: center

      Written second, drawn first.

Nesting
=======

"Nesting" is the page this family was added for: it draws a PlantUML diagram
from inside a ``.. grid-item::``.

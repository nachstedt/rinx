.. _grid-columns:

Columns
=======

A ``.. grid::``'s argument is a column count, and a ``.. grid-item::``'s
``:columns:`` is how many of those columns one cell spans. Both accept one
value — applying at every breakpoint — or four, read as *xs sm md lg*, and both
accept ``auto``.

The argument
------------

Three cells to a row, at every width:

.. grid:: 3
   :gutter: 2

   .. grid-item::
      :outline:

      First

   .. grid-item::
      :outline:

      Second

   .. grid-item::
      :outline:

      Third

One cell to a row while narrow, four once there is room — the four-value form:

.. grid:: 1 2 3 4
   :gutter: 2

   .. grid-item::
      :outline:

      xs: 1

   .. grid-item::
      :outline:

      sm: 2

   .. grid-item::
      :outline:

      md: 3

   .. grid-item::
      :outline:

      lg: 4

A grid written with no argument at all is legal; the row then carries no column
classes and the cells share the width evenly:

.. grid::
   :gutter: 2

   .. grid-item::
      :outline:

      No argument

   .. grid-item::
      :outline:

      was written.

The ``:columns:`` option
------------------------

A cell may take a different share of the row than the argument gives it. In a
twelve-column row, a wide cell beside a narrow one:

.. grid:: 12
   :gutter: 2

   .. grid-item::
      :columns: 9
      :outline:

      ``:columns: 9``

   .. grid-item::
      :columns: 3
      :outline:

      ``:columns: 3``

``auto`` asks for only as much width as the content needs:

.. grid:: 12
   :gutter: 2

   .. grid-item::
      :columns: auto
      :outline:

      ``auto``

   .. grid-item::
      :columns: auto
      :outline:

      is as wide as its text.

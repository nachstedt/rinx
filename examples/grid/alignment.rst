.. _grid-alignment:

Alignment
=========

Two options govern how a cell lays out its own content, and one governs the
order the row draws its cells in.

``:child-direction:``
---------------------

A cell stacks its content in a column unless it is told otherwise. Written as
``row``, the content flows across instead:

.. grid:: 2
   :gutter: 3

   .. grid-item::
      :outline:
      :padding: 2

      Stacked

      in a column.

   .. grid-item::
      :child-direction: row
      :outline:
      :padding: 2

      Flowing

      across a row.

``:child-align:``
-----------------

Where the content sits along the cell's major axis — ``start``, ``end``,
``center``, ``justify`` or ``spaced``:

.. grid:: 3
   :gutter: 2

   .. grid-item::
      :child-align: start
      :outline:

      ``start``

   .. grid-item::
      :child-align: center
      :outline:

      ``center``

   .. grid-item::
      :child-align: end
      :outline:

      ``end``

``:reverse:``
-------------

The row lays its cells out right to left. The cell written first is drawn last:

.. grid:: 3
   :gutter: 2
   :reverse:

   .. grid-item::
      :outline:

      Written first

   .. grid-item::
      :outline:

      Written second

   .. grid-item::
      :outline:

      Written third

``:class-container:``, ``:class-row:`` and ``:class:``
------------------------------------------------------

The three escape hatches, for a project whose own stylesheet needs a hook. This
site defines nothing for them:

.. grid:: 2
   :gutter: 2
   :class-container: example-grid-container
   :class-row: example-grid-row

   .. grid-item::
      :class: example-grid-item
      :outline:

      The classes land on the outer ``<div>``, the row ``<div>`` and this
      cell's ``<div>`` respectively.

   .. grid-item::
      :outline:

      They are added to sphinx-design's own classes, never in place of them.

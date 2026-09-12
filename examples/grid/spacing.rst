.. _grid-spacing:

Spacing
=======

Three options control space around a grid: ``:gutter:`` between the cells,
``:margin:`` outside the whole row, and ``:padding:`` inside it. A cell takes
``:margin:`` and ``:padding:`` of its own.

``:gutter:``
------------

A step from 0 to 5, in the same one-or-four breakpoint form the column counts
use. Unlike a column count it starts at 0 and never accepts ``auto``.

No gutter at all — the cells touch:

.. grid:: 3
   :gutter: 0

   .. grid-item::
      :outline:

      0

   .. grid-item::
      :outline:

      is the

   .. grid-item::
      :outline:

      tightest step.

The widest:

.. grid:: 3
   :gutter: 5

   .. grid-item::
      :outline:

      5

   .. grid-item::
      :outline:

      is the

   .. grid-item::
      :outline:

      widest.

``:margin:`` and ``:padding:``
------------------------------

Both take one value, applying to all four sides, or four, read as *top bottom
left right* — an order that is neither CSS's clockwise one nor alphabetical.
``:margin:`` accepts ``auto``; ``:padding:`` does not.

.. grid:: 2
   :gutter: 2
   :margin: 4 4 auto auto
   :padding: 3
   :outline:

   .. grid-item::
      :padding: 3
      :outline:

      The row has a wide margin above and below and is centred horizontally;
      it carries a padding of its own, and so does this cell.

   .. grid-item::
      :margin: 0 0 3 3
      :outline:

      This cell keeps space to its left and right instead.

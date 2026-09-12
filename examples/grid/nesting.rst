.. _grid-nesting:

Nesting
=======

A cell's body is ordinary block content. That sounds unremarkable and is the
entire reason these two directives exist: an *unknown* directive never parses
its body, so every construct written inside a grid used to be invisible to the
index, the renderer and the diagram pipeline alike.

A diagram in a cell
-------------------

The shape that motivated the directive — the sphinx-needs demo corpus draws a
PlantUML diagram inside a ``.. grid-item::``, beside the source that produced
it. Neither the code block nor the diagram reached any later phase before the
grid parsed its body:

.. grid:: 2
   :gutter: 3

   .. grid-item::
      :outline:
      :padding: 2

      .. code-block:: rst

         .. uml::

            node A
            node B
            A --> B

   .. grid-item::
      :outline:
      :padding: 2

      .. uml::

         node A
         node B
         A --> B

Other constructs in a cell
--------------------------

A target written inside a cell registers like any other, so :ref:`this one
<grid-nested-target>` resolves:

.. grid:: 2
   :gutter: 3

   .. grid-item::
      :outline:
      :padding: 2

      .. _grid-nested-target:

      A cell holding a hyperlink target, an admonition and a list.

      .. note::

         Admonitions nest as they do anywhere else.

   .. grid-item::
      :outline:
      :padding: 2

      * A bullet list
      * inside a cell
      * beside a table:

      .. list-table::
         :header-rows: 1

         * - Option
           - Takes
         * - ``:gutter:``
           - 0 to 5
         * - ``:columns:``
           - 1 to 12, or ``auto``

A grid inside a grid
--------------------

A cell may hold a whole grid of its own:

.. grid:: 1
   :gutter: 3

   .. grid-item::
      :outline:
      :padding: 3

      The outer cell.

      .. grid:: 2
         :gutter: 2

         .. grid-item::
            :outline:

            An inner cell,

         .. grid-item::
            :outline:

            and its neighbour.

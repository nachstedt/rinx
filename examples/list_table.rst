List Tables
===========

``.. list-table::`` specifies a table as a nested bullet list instead of
character-art: the outer bullet list is the table's rows, and each row's own
bullet list is its cells. Cell content is parsed as full block-level RST —
"a miniature document" — exactly like a grid table cell.

Basic List Table
-----------------

.. list-table::

   * - Fruit
     - Colour
   * - Apple
     - Red
   * - Banana
     - Yellow

Table with a Title
-------------------

The directive's argument, if given, becomes the table's caption:

.. list-table:: Popular Fruits

   * - Fruit
     - Colour

``:header-rows:``
------------------

Marks the leading N rows as header rows, rendered inside ``<thead>``:

.. list-table::
   :header-rows: 1

   * - Fruit
     - Colour
   * - Apple
     - Red
   * - Banana
     - Yellow

``:stub-columns:``
-------------------

Marks the leading N columns as stub (row-header) columns in every row,
rendered as ``<th scope="row">``:

.. list-table::
   :header-rows: 1
   :stub-columns: 1

   * - Setting
     - Default
   * - verbose
     - false
   * - retries
     - 3

``:widths:``
------------

Sets relative column widths — ``auto`` and ``grid`` both let the renderer
decide, while an explicit list of integers is normalized into percentages:

.. list-table::
   :header-rows: 1
   :widths: 20 80

   * - Key
     - Description
   * - name
     - A short, human-readable identifier.

``:width:``
-----------

Sets the overall table width:

.. list-table::
   :width: 60%

   * - Fruit
     - Colour

``:align:``
-----------

Aligns the table on the page — ``left``, ``center``, or ``right``:

.. list-table::
   :width: 50%
   :align: center

   * - Fruit
     - Colour

``:class:``
-----------

Adds extra CSS classes to the rendered ``<table>``:

.. list-table::
   :class: compact

   * - Fruit
     - Colour

``:name:`` and Cross-Referencing
----------------------------------

Gives the table a hyperlink target, referenceable via ``:ref:``. As with any
other internal target in rusty-sphinx today, the link resolves to this
*document*, not precisely to the table itself.

.. list-table::
   :name: fruit-table

   * - Fruit
     - Colour

See :ref:`fruit-table` for the fruit table above.

Domain Objects Nested Inside a Cell
--------------------------------------

Cell content is fully reparsed as block-level RST, so a cell can nest a
domain-object definition — exactly like real CPython documentation nests
``.. attribute::`` directives inside a ``list-table`` cell (see
``reference/datamodel.rst``'s "Special read-only attributes" table):

.. list-table::
   :header-rows: 1

   * - Attribute
     - Meaning
   * - .. py:attribute:: method.__self__

     - The instance to which a bound method is bound.

:attr:`method.__self__` resolves correctly because the definition above was
fully parsed rather than swallowed as opaque directive body text.

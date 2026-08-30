The ``table`` Directive
========================

``.. table::`` wraps an existing grid or simple table (given as the
directive's own content) with a title/caption and the presentation options
neither ASCII-art syntax has notation of its own for.

Wrapping a Grid Table
----------------------

.. table::

   +-------+--------+
   | Fruit | Colour |
   +=======+========+
   | Apple | Red    |
   +-------+--------+
   | Lemon | Yellow |
   +-------+--------+

Wrapping a Simple Table
-------------------------

.. table::

   =====  =====
     A    not A
   =====  =====
   False  True
   True   False
   =====  =====

Table with a Title
--------------------

The directive's argument, if given, becomes the table's caption:

.. table:: Truth Table for "not"

   =====  =====
     A    not A
   =====  =====
   False  True
   True   False
   =====  =====

``:widths:``
------------

Sets relative column widths — ``auto`` and ``grid`` both let the renderer
decide, while an explicit list of integers (matching the wrapped table's own
column count) is normalized into percentages:

.. table::
   :widths: 20 80

   +------+-------------------------------------+
   | Key  | Description                         |
   +======+=====================================+
   | name | A short, human-readable identifier. |
   +------+-------------------------------------+

``:width:``
-----------

Sets the overall table width:

.. table::
   :width: 60%

   +-------+--------+
   | Fruit | Colour |
   +-------+--------+

``:align:``
-----------

Aligns the table on the page — ``left``, ``center``, or ``right``:

.. table::
   :width: 50%
   :align: center

   +-------+--------+
   | Fruit | Colour |
   +-------+--------+

``:class:``
-----------

Adds extra CSS classes to the rendered ``<table>``:

.. table::
   :class: compact

   +-------+--------+
   | Fruit | Colour |
   +-------+--------+

``:name:`` and Cross-Referencing
----------------------------------

Gives the table a hyperlink target, referenceable via ``:ref:``. As with any
other internal target in rusty-sphinx today, the link resolves to this
*document*, not precisely to the table itself.

.. table::
   :name: wrapped-fruit-table

   +-------+--------+
   | Fruit | Colour |
   +-------+--------+

See :ref:`wrapped-fruit-table` for the wrapped table above.

Domain Objects Nested Inside a Cell
--------------------------------------

A wrapped table's cells are ordinary grid/simple table cells, so they parse
as full block-level RST exactly like an unwrapped one — a cell can nest a
domain-object definition:

.. table::

   +----------------------------------+-----------------------------------------+
   | Attribute                        | Meaning                                 |
   +==================================+=========================================+
   | .. py:attribute:: Circle.radius  | The circle's radius, in pixels.         |
   +----------------------------------+-----------------------------------------+

:attr:`Circle.radius` resolves correctly because the definition above was
fully parsed rather than swallowed as opaque directive body text.

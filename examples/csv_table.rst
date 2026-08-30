CSV Tables
==========

``.. csv-table::`` specifies a table as CSV data rather than character-art or a
nested bullet list. It shares every presentation option with
``.. list-table::`` — the two parse into the same node and render through the
same code — and adds its own options for where the data comes from and how it
is tokenized.

Cell content is parsed as full block-level RST, exactly like a grid table cell.

Basic CSV Table
---------------

.. csv-table::

   Fruit, Colour
   Apple, Red
   Banana, Yellow

Table with a Title
------------------

The directive's argument, if given, becomes the table's caption:

.. csv-table:: Popular Fruits

   Apple, Red
   Banana, Yellow

``:header-rows:``
-----------------

Marks the leading N rows of the data as header rows, rendered inside
``<thead>``:

.. csv-table::
   :header-rows: 1

   Fruit, Colour
   Apple, Red
   Banana, Yellow

``:header:``
------------

Supplies header rows as CSV data in the option itself, so the body holds only
the table's real data. They are prepended to the rows and counted as header
rows automatically:

.. csv-table::
   :header: Fruit, Colour

   Apple, Red
   Banana, Yellow

``:header:`` and ``:header-rows:`` add up — the table below has two header
rows, one from each:

.. csv-table::
   :header: Fruit, Colour
   :header-rows: 1

   Citrus, Group
   Lemon, Yellow

``:stub-columns:``
------------------

Marks the leading N columns as stub (row-header) columns in every row,
rendered as ``<th scope="row">``:

.. csv-table::
   :header-rows: 1
   :stub-columns: 1

   Setting, Default
   verbose, false
   retries, 3

``:widths:``
------------

Sets relative column widths — ``auto`` and ``grid`` both let the renderer
decide, while an explicit list of integers is normalized into percentages:

.. csv-table::
   :header-rows: 1
   :widths: 20 80

   Key, Description
   name, A short human-readable identifier.

``:width:``
-----------

Sets the overall table width:

.. csv-table::
   :width: 60%

   Apple, Red

``:align:``
-----------

Aligns the table on the page — ``left``, ``center``, or ``right``:

.. csv-table::
   :width: 50%
   :align: center

   Apple, Red

``:class:``
-----------

Adds extra CSS classes to the rendered ``<table>``, after the ``csv-table``
class the directive always carries:

.. csv-table::
   :class: compact

   Apple, Red

``:name:`` and Cross-Referencing
--------------------------------

Gives the table a hyperlink target, referenceable via ``:ref:``. As with any
other internal target in rusty-sphinx today, the link resolves to this
*document*, not precisely to the table itself.

.. csv-table::
   :name: fruit-csv-table

   Apple, Red

See :ref:`fruit-csv-table` for the fruit table above.

Quoting
-------

A field wrapped in double quotes may contain the delimiter, a doubled quote
(which stands for one literal quote), and even line breaks:

.. csv-table::
   :header-rows: 1

   Name, Note
   "Apple, Braeburn", A cultivar whose name contains a comma.
   "He said ""hello""", A doubled quote is one literal quote.
   "First line
   second line", A quoted field may span lines.

``:delim:``
-----------

Changes the field delimiter. It accepts a literal character, the words
``space`` and ``tab``, or a character code such as ``0x3B``:

.. csv-table::
   :delim: ;

   Apple; Red
   Banana; Yellow

.. csv-table::
   :delim: tab

   Apple	Red
   Banana	Yellow

``:quote:``
-----------

Changes the quoting character:

.. csv-table::
   :quote: '

   'Apple, Braeburn', Red

``:escape:``
------------

Sets an escape character, an alternative to doubling the quote:

.. csv-table::
   :escape: \

   "He said \"hello\"", Red

``:keepspace:``
---------------

By default the whitespace following a delimiter is stripped. ``:keepspace:``
keeps it in the field text:

.. csv-table::
   :keepspace:

   Apple,   Red

``:file:``
----------

Reads the data from a file instead of the directive body. The path resolves
relative to this document, and the file must be declared in the library's
``csv_data`` attribute in ``BUILD.bazel`` so it reaches the parse action:

.. csv-table:: Fruits, from a file
   :header: Fruit, Colour
   :file: data/fruits.csv

Ragged Rows
-----------

A row with fewer fields than the widest row is padded with empty cells rather
than rejected:

.. csv-table::
   :header-rows: 1

   Fruit, Colour, Note
   Apple, Red
   Banana, Yellow, Best when speckled.

RST Inside a Cell
-----------------

Every field is reparsed as block-level RST, so inline markup, literals and
cross-references all work inside a cell:

.. csv-table::
   :header-rows: 1

   Construct, Example
   Emphasis, *emphasized text*
   Literal, ``literal text``
   Reference, :ref:`fruit-csv-table`

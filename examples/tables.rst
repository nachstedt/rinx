Grid Tables
===========

Grid tables use "ASCII art" made of ``+``, ``-``, ``=``, and ``|`` to lay out
rows and columns. Every ``+``/``|`` column boundary must line up in the same
character position across all rows — a table whose boundaries are misaligned
is rejected as malformed.

Simple Table with a Header
--------------------------

The ``=`` divider line marks the boundary between the header row and the body:

+-------------+---------------+----------+
| Fruit       | Colour        | Quantity |
+=============+===============+==========+
| Apple       | Red           | 12       |
+-------------+---------------+----------+
| Banana      | Yellow        | 7        |
+-------------+---------------+----------+

Table Without a Header Row
--------------------------

Header rows are optional; omit the ``=`` divider and every row is a body row:

+-------------+---------------+
| Setting     | Default       |
+-------------+---------------+
| verbose     | false         |
+-------------+---------------+
| retries     | 3             |
+-------------+---------------+

Cells Spanning Columns
----------------------

Dropping the ``|`` (and the ``+`` above/below it) merges a cell across
columns:

+-------------+---------------+----------+
| Region      | Q1            | Q2       |
+=============+===============+==========+
| North       | Sales grew steadily.     |
+-------------+---------------+----------+
| South       | 40            | 55       |
+-------------+---------------+----------+

Cells Spanning Rows
-------------------

Dropping the ``-`` divider between two rows in one column merges that cell
downward, so it spans multiple rows while its neighbours do not:

+-------------+---------------+
| Component   | Status        |
+=============+===============+
| Parser      | Stable and    |
+-------------+ well covered  |
| Renderer    | by tests.     |
+-------------+---------------+

Hierarchical (Nested) Headers
------------------------------

A header row can span multiple lines and introduce a new column split partway
through — the ``+`` on the second header line divides "Example values" into
two sub-columns, even though the top border only shows one wide column there:

+---------------+------+-----------------------+
|               | Bits | Example values        |
|               |      +------------+----------+
| Field         | (n)  | Little-end | Big-end  |
+===============+======+============+==========+
| Major version |   8  | 0x03       | 0x03     |
+---------------+------+------------+----------+
| Minor version |   8  | 0x0A       | 0x04     |
+---------------+------+------------+----------+

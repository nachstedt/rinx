.. _numref-example:

The ``:numref:`` Role
=====================

``:numref:`` links a figure, table, code block or section and shows its
number. This site sets ``numfig = true`` in its ``rinx.toml``, so every
captioned figure, table and code block carries a number in front of its
caption. The numbers count across the whole site in toctree order, and start
again under each chapter of a ``:numbered:`` toctree.

Numbered elements
-----------------

.. _numref-logo:

.. figure:: data/logo.svg
   :alt: The rinx logo
   :width: 100px

   The logo, numbered.

.. list-table:: The default formats
   :name: numref-formats
   :header-rows: 1

   * - Kind
     - Default ``numfig_format``
   * - ``figure``
     - ``Fig. %s``
   * - ``table``
     - ``Table %s``
   * - ``code-block``
     - ``Listing %s``
   * - ``section``
     - ``Section %s``

.. code-block:: python
   :caption: A numbered listing
   :name: numref-listing

   print("numbered")

.. _numref-uncaptioned:

.. code-block:: text

   A code block without a caption gets no number, even with a label.

Referring to them
-----------------

* A bare ``:numref:`` shows the site's format for the kind of thing it names:
  :numref:`numref-logo`, :numref:`numref-formats` and
  :numref:`numref-listing`.
* An explicit title is the format. ``{number}`` is the number and ``{name}``
  is the caption: :numref:`Figure {number}: {name} <numref-logo>`. The older
  ``%s`` spelling works too: :numref:`Table %s <numref-formats>`.
* Spelled with its domain, as Sphinx also accepts it:
  :std:numref:`numref-listing`.
* A section numbered by a ``:numbered:`` toctree shows its section number:
  :numref:`numbered-installing`. A label on a numbered page's title shows the
  page's number: :numref:`numbered-one`. Neither needs ``numfig``.
* That chapter's own listing is numbered under the chapter:
  :numref:`numbered-listing`.
* A leading ``!`` shows the text without looking it up:
  :numref:`!numref-logo`.

Problems
--------

A title with nowhere for the number to go is reported while parsing, as
``numref.invalid-format``. It is shown as text, as in Sphinx:

.. noqa: numref.invalid-format

:numref:`see the logo <numref-logo>`

A label on something that is never numbered, such as an uncaptioned code
block, is reported as ``link.broken-numref``. Both are suppressed here, since
this site is built with ``strict_links = True``:

.. noqa: link.broken-numref

:numref:`numref-uncaptioned`

Inline Roles
============

This page demonstrates the interpreted text roles supported by Rinx.

Supported Roles
---------------

Currently, we support the following interpreted text roles:

* **Program Name** (``:program:``):
  Used to mark the name of an executable program or command-line tool.
  For example, the :program:`rustc` compiler and the :program:`cargo` tool
  make up the core of Rust development.

Examples
--------

Here are some more examples of program names in different contexts:

* Program name as a single word: Use :program:`curl` to fetch webpage source.
* Program name with internal spaces: Run :program:`git status` to see unstaged changes.
* Program name adjacent to punctuation: Run (:program:`wget`), or search for :program:`tar`.
* Multiple program names in one sentence: You can use either :program:`gzip` or :program:`bzip2` to compress the output.

Term References (``:term:``)
-----------------------------

The ``:term:`` role creates a cross-reference to a term defined in a
``.. glossary::`` directive.

* Basic term reference: The :term:`environment` stores document metadata.
* Multi-word term: The :term:`configuration directory` holds project settings.
* Custom display text: :term:`the build tool <builder>` processes the RST files.

Reference Role (``:ref:``)
---------------------------

The ``:ref:`` role creates a cross-reference to a labeled location
elsewhere in the site.

* Basic reference: See :ref:`home-index` for the project overview. With no
  explicit title, the link shows the title of the section the label sits
  above, as in Sphinx.
* Custom display text: :ref:`the site home <home-index>` links back to
  the same page under different link text.

Subscript and Superscript (``:sub:``, ``:sup:``)
-------------------------------------------------

``:sub:`` and ``:sup:`` set their text below or above the line;
``:subscript:`` and ``:superscript:`` are the same roles under longer names.
Their text is plain: no markup inside it is interpreted.

* Escaped spaces join a script to its word: water is H\ :sub:`2`\ O, and
  Einstein wrote E = mc\ :sup:`2`.
* The long names: the 1\ :superscript:`st` of x\ :subscript:`i`.
* Text that would be markup elsewhere stays text: 2\ :sup:`*n*`.

CO\ :sub:`2` in a Heading
~~~~~~~~~~~~~~~~~~~~~~~~~

A script may be written in a section title too.

Derived Roles
~~~~~~~~~~~~~

``.. role::`` derives a role from either. Without a ``:class:``, the role's
own name is its class:

.. role:: chem(sub)

.. role:: power(sup)
   :class: exponent

* A ``chem`` role: C\ :chem:`6`\ H\ :chem:`12`\ O\ :chem:`6`.
* A ``power`` role carrying the ``exponent`` class: 10\ :power:`-3`.

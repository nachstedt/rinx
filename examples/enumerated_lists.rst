Enumerated Lists
================

A line opening with an enumerator — a number, letter or roman numeral followed
by ``.``, ``)`` or wrapped in ``()`` — starts an enumerated list, rendered as
``<ol class="sequence format">``.

Enumeration Sequences
---------------------

All five docutils sequences are supported. The sequence is decided by the
list's *first* enumerator and then held for the rest of the list.

Arabic numerals:

1. First item
2. Second item
3. Third item

Lowercase letters:

a. First item
b. Second item

Uppercase letters:

A. First item
B. Second item

Lowercase roman numerals:

i. First item
ii. Second item
iii. Third item

Uppercase roman numerals:

I. First item
II. Second item
III. Third item

Enumerator Formats
------------------

The same three formats docutils recognises. Unlike Sphinx — which drops the
punctuation and renders all three identically — rusty-sphinx keeps it, so the
notation you write is the notation the reader sees.

Suffixed with a period:

1. Period format
2. Second item

Suffixed with a right parenthesis:

1) Right-paren format
2) Second item

Surrounded by parentheses:

(1) Parenthesised format
(2) Second item

The format is part of the list's identity, so changing it mid-list does not
continue the list. These sequences and formats combine freely:

(iv) A parenthesised lower-roman list, starting at four
(v) Second item

Auto-Enumeration
----------------

``#.`` numbers an item automatically, so items can be reordered without
renumbering them by hand:

#. Automatically numbered
#. Second item
#. Third item

The first item may also be numbered explicitly, with the rest deferred to
``#``:

1. Explicitly numbered
#. Automatically numbered
#. Also automatic

Starting at Another Number
--------------------------

A list may begin anywhere in its sequence. The starting ordinal is preserved
as the ``<ol>``'s ``start`` attribute, and the build reports it as a
diagnostic in case the offset was accidental:

5. This list starts at five
6. Sixth item

Multi-Line and Multi-Paragraph Items
------------------------------------

An item's body continues for as long as it stays indented past the
enumerator, so items may wrap and may hold several paragraphs:

1. This item's text wraps onto a second source line, which is joined
   into one paragraph.

2. This item holds two paragraphs.

   The second paragraph stays part of the item because it is still indented
   relative to the enumerator.

3. A short third item.

An item's marker may also stand alone, with the body starting on the next
line:

1.
   The body indent is read from this line rather than assumed from the
   marker's width.

2. Second item

Nested Lists
------------

Item bodies are parsed as full block-level RST, so lists nest — in either
direction, and to any depth:

1. An outer arabic item

   a. A nested lower-alpha item
   b. Another nested item

      i. A third level, in roman numerals

2. Back to the outer list

Enumerated lists nest inside bullet lists and vice versa:

* A bullet item

  1. A nested enumerated item
  2. Another one

Other Block Content in an Item
------------------------------

Because item bodies go through the same block dispatch as the document body,
they may contain any block construct — directives, literal blocks, tables,
cross-references:

1. An item containing an admonition:

   .. note::

      Directives work inside list items.

2. An item containing a literal block::

      def numbered():
          return 1

3. An item cross-referencing the :py:mod:`greetings` module and linking to
   :ref:`home-index`.

Lists Inside Directives
-----------------------

The nesting works in the other direction too — an enumerated list is a plain
block construct, so it may appear in any directive body:

.. seealso::

   1. First thing to see
   2. Second thing to see

When a Line Is *Not* a List
---------------------------

An enumerator is ambiguous with ordinary prose, so docutils requires the
following line to agree: it must be blank, indented, or carry the next
enumerator in the sequence. That is what keeps this a paragraph rather than a
one-item list:

A. Einstein said this.
He was smart.

The same rule keeps a decimal number from opening a list, because the marker
must be followed by a space:

1.5 is a number, not a list item.

Note the rule only applies when a non-blank, unindented line follows. Standing
alone, ``A. Einstein said this.`` really would become a list — in docutils
too. The spec's advice there is to escape the period.

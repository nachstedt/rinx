.. _noqa-example:

Suppressing diagnostics
=======================

Every warning rinx reports carries a code, and a ``.. noqa:`` comment
silences the codes it names for the block that follows it.

The comment is ordinary reStructuredText — Sphinx itself sees a comment and
ignores it — so a document using this still builds with both tools.

Suppressing one code
--------------------

The paragraph below references a label that does not exist anywhere in this
site. Without the comment above it, ``//examples:site`` would fail, because it
is built with ``strict_links = True``.

.. noqa: link.broken-ref

See :ref:`a-label-that-does-not-exist` for details.

The very same reference is *not* suppressed a second time, so it has to be
written above each block that needs it — a suppression covers one block, not
the rest of the file.

Suppressing several codes at once
---------------------------------

Ids may be separated by commas, spaces, or both.

.. noqa: link.broken-ref, link.broken-term

Both :ref:`another-missing-label` and :term:`a missing glossary term` are
silenced by the one comment above, because both codes are named.

Suppressing everything in a block
---------------------------------

A bare ``.. noqa`` covers every code. Prefer naming the codes: a blanket
suppression also hides the *next* problem to appear in this block, which
nobody chose.

.. noqa

Here is :ref:`yet-another-missing-label` again.

Suppressing a parse-time diagnostic
-----------------------------------

Suppression is not limited to links. Any code works, including the ones the
parser reports while reading a directive:

.. noqa: directive.toctree-unknown-option

.. toctree::
   :hidden:
   :not-a-real-option: value

Nesting
-------

A ``.. noqa:`` applies to the next block *at its own level*, so one written
inside a directive body or a list item covers only that nested block:

.. note::

   .. noqa: link.broken-ref

   This nested reference is suppressed: :ref:`missing-inside-a-note`.

   .. noqa: link.broken-ref

   And this one needs its own comment: :ref:`also-missing-inside-a-note`.

Equally, a comment before a container covers everything nested inside it:

.. noqa: link.broken-ref

.. note::

   Both :ref:`missing-one` and :ref:`missing-two` are covered by the single
   comment above this note, because the note *is* the block that follows it.

Math warnings
-------------

The mechanism is not limited to links. LaTeX the math converter cannot read is
reported as ``math.invalid-latex``, and an ``:eq:`` naming no labeled equation
as ``link.broken-eq``; both suppress the same way. The equation below is
deliberately malformed, and its warning is silenced — the page still shows the
source the author wrote:

.. noqa: math.invalid-latex

.. math::

   \frac{1}{2

.. noqa: link.broken-eq

This reference to a nonexistent equation is suppressed too: :eq:`no-such-equation`.

Mistyped codes
--------------

An id that names no diagnostic is reported rather than ignored, so a typo
cannot quietly stop suppressing anything. Building this page prints a
``noqa.unknown-code`` warning for the comment below — deliberately, as the
demonstration:

.. noqa: link.broken-reff

The typo above suppresses nothing, so this paragraph deliberately references
nothing broken.

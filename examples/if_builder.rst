.. _if-builder-example:

Conditional content by builder
==============================

``.. if-builder::`` comes from `sphinx-simplepdf
<https://sphinx-simplepdf.readthedocs.io/en/latest/directives.html>`_. Its body
is contributed only when the argument names the builder that is running.
rinx has exactly one builder, ``html``, so a block naming any other
builder is left out.

The directive takes one argument and no options. The name is matched
case-insensitively, as it is upstream.

A block for this builder
------------------------

.. if-builder:: html

   This paragraph is inside ``.. if-builder:: html``, so it is part of the
   page.

The name is matched case-insensitively, so this block is included too:

.. if-builder:: HTML

   Written as ``HTML``, included all the same.

A block for another builder
---------------------------

The next block names sphinx-simplepdf's own PDF builder, so nothing between
here and the following section reaches the page — and nothing in it is
reported, because naming another builder is the ordinary use of the directive
rather than a mistake.

.. if-builder:: simplepdf

   This paragraph is only for the PDF build, and you are not reading it.

   .. some-pdf-only-directive::

      A non-matching body is never parsed at all, which is why this directive
      that rinx does not implement costs nothing here: no
      ``directive.unknown`` report, and no error block on the page.

   .. toctree::

      a_document_that_does_not_exist

   The toctree above names a document that is in no ``deps`` anywhere and does
   not exist. The site still builds, which is the point: content excluded by
   ``.. if-builder::`` is never read, so it demands no build dependency. Please
   leave it here — it is the standing proof of that property.

The body is spliced, not wrapped
--------------------------------

This is what ``.. if-builder::`` offers over Sphinx's own ``.. only::``, and
why sphinx-simplepdf's documentation recommends it: an included body is
contributed to the enclosing document rather than nested inside a container, so
headings, hyperlink targets and toctrees written inside it are real structure.

.. if-builder:: html

   .. _if-builder-spliced-target:

   A heading written inside the block
   ----------------------------------

   The heading above is a genuine section of this page — it appears in the
   navigation and takes its level from this document's own adornment sequence.
   The target above it is a document target, so :ref:`this reference
   <if-builder-spliced-target>` resolves to it from outside the block.

Nested blocks
-------------

The directive nests, and each level is decided on its own.

.. if-builder:: html

   The outer block is for this builder.

   .. if-builder:: html

      So is the inner one, so this line is on the page.

   .. if-builder:: latex

      This inner block is for LaTeX, so this line is not.

What is reported
----------------

An argument naming no builder anyone knows is reported as
``if-builder.unknown-builder`` and the block is left out. Upstream leaves it
out silently, which makes a typo indistinguishable from a deliberate exclusion.

.. noqa: if-builder.unknown-builder

.. if-builder:: htlm

   A misspelling of ``html``. Reported rather than silently deleted.

A directive with no argument names no builder at all. It is reported as
``if-builder.missing-builder`` and drawn as an error block quoting its source,
so the content is not lost without trace.

.. noqa: if-builder.missing-builder

.. if-builder::

   There is no builder name above, so this build cannot decide whether to keep
   this paragraph.

A selected block with no content is reported as ``if-builder.empty-body``: the
directive was written to contribute something and contributed nothing. The same
emptiness on a branch that was *not* selected is silent, since only the taken
branch can be wrong about it.

.. noqa: if-builder.empty-body

.. if-builder:: html

.. if-builder:: latex

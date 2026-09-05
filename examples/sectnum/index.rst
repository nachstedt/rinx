.. _sectnum-index:

###################
Sectnum Directives
###################

``.. sectnum::`` (docutils' other spelling: ``.. section-numbering::``) numbers
every section in the document it's written in, wherever it's written — unlike
``.. contents::``, its effect is document-wide rather than scoped to where the
directive sits, so unlike :ref:`contents-index` each option combination below
gets its own page instead of a single page with several ``:local:`` examples.

.. toctree::

   default
   depth
   start
   prefix_suffix

Default numbering
==================

"Default" numbers every section and subsection with no options at all.

Depth limit
===========

"Depth" stops one level short with ``:depth: 1``.

Start value
===========

"Start" begins its first chapter at ``:start: 5`` instead of ``1``.

Prefix and suffix
==================

"Prefix and Suffix" wraps every number in literal text with ``:prefix:`` and
``:suffix:``.

Precedence with ``:numbered:``
===============================

A document reached by a ``:numbered:`` toctree that also writes its own
``.. sectnum::`` ignores the directive entirely — the toctree's numbering
wins, exactly like a nested ``:numbered:`` toctree is ignored inside an
already-numbered subtree. See :ref:`toctree-index`'s own "Numbering" section
for that page.

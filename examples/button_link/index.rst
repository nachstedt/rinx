.. _button-link-index:

######################
Button Link Directives
######################

``.. button-link::`` is sphinx-design's button-shaped external link: a link
drawn as a button, whose argument is the URL and whose content is the label.
Like ``.. dropdown::`` and ``.. grid::`` it is not part of docutils or Sphinx,
and like them it is simply always available — this build has no
``extensions =`` setting to turn it on with.

It is the one sphinx-design directive here with no block body at all. Its
content is *inline* markup, so a literal or an emphasis in the label works and
a whole paragraph does not:

.. button-link:: https://www.sphinx-doc.org/
   :color: primary
   :shadow:

   Read the Sphinx documentation

.. toctree::

   basics
   colors
   layout
   diagnostics

Basics
======

"Basics" shows the label, and what a button with no content at all renders as
— the URL becomes its own label, so a bare one-line directive is still a
usable button:

.. button-link:: https://docutils.sourceforge.io/

Colours
=======

"Colours" paints the button with each of the eleven ``:color:`` values, filled
and as ``:outline:``. One of each:

.. button-link:: https://example.com/filled
   :color: success

   ``:color: success``

.. button-link:: https://example.com/outlined
   :color: success
   :outline:

   ``:color: success`` with ``:outline:``

Layout
======

"Layout" shows ``:align:``, ``:expand:``, ``:shadow:``, ``:tooltip:``,
``:click-parent:`` and ``:class:``. ``:align:`` lands on the paragraph holding
the button rather than on the button itself:

.. button-link:: https://example.com/centered
   :align: center
   :color: info
   :tooltip: The tooltip is the title attribute

   Centred, with a tooltip

Diagnostics
===========

"Diagnostics" writes every case this build refuses or narrows: a missing
argument, an unreadable ``:color:`` or ``:align:``, an ``:outline:`` with no
colour to outline, sphinx-needs' ``:ref-type:``, and a reference role inside a
label. Every one of them is reported and none of them costs the button.

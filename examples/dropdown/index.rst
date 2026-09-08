.. _dropdown-index:

####################
Dropdown Directives
####################

``.. dropdown::`` is sphinx-design's collapsible container: a ``<details>``
element with a clickable summary bar. Unlike every other directive in this
example site it is not part of docutils or Sphinx, and unlike a
``:collapsible:`` admonition its title is parsed as inline markup and its ten
options control the bar's colour, icon, marker, animation and spacing.

Both of its states, side by side — the first starts collapsed, the second
carries ``:open:`` so its body shows without being clicked:

.. dropdown:: Click me

   A dropdown's body is ordinary block content, so it can hold anything a
   document can.

.. dropdown:: Already open, with an icon and a coloured bar
   :open:
   :icon: light-bulb
   :color: success

   ``:open:``, ``:icon: light-bulb`` and ``:color: success`` together. The
   pages below take the ten options one group at a time.

.. toctree::

   basics
   colors
   icons
   markers
   spacing
   reference

Basics
======

"Basics" shows the title, the body, ``:open:``, and what a dropdown with no
title at all looks like. Written with no argument, the summary draws a
placeholder icon instead of an empty bar, so there is still something to click:

.. dropdown::

   No title was written for this one — the only thing in its summary is the
   placeholder.

Colours
=======

"Colours" paints the summary bar with each of the eleven ``:color:`` values.
One of them, with the marker taking the bar's own colour:

.. dropdown:: ``:color: danger``
   :color: danger

   Each value adds a background class and a matching text class, so the title
   stays readable on a light bar as well as a dark one.

Icons
=====

"Icons" puts an octicon before the title with ``:icon:``. The name is checked
while parsing, so a misspelled one is reported rather than silently missing:

.. dropdown:: ``:icon: beaker``
   :icon: beaker

   Any name from the bundled octicon set works; the icon sits before the title
   text, and the state marker stays on the right.

Markers and animation
=====================

"Markers" shows the two ``:chevron:`` directions and the two ``:animate:``
reveals. Both are only visible in motion, so this one is worth opening:

.. dropdown:: ``:chevron: down-up`` and ``:animate: fade-in-slide-down``
   :chevron: down-up
   :animate: fade-in-slide-down

   The marker pointed down while this was closed and points up now. The body
   faded in while sliding down as it opened.

Spacing and classes
===================

"Spacing" shows ``:margin:`` in both its forms, and the three
``:class-container:`` / ``:class-title:`` / ``:class-body:`` escape hatches:

.. dropdown:: ``:margin: 0 4 auto auto`` with a body class
   :margin: 0 4 auto auto
   :class-body: example-dropdown-body
   :color: info

   Four values are read as *top bottom left right* — no space above, a wide gap
   below, centred horizontally. ``:class-body:`` is the hook a project's own
   stylesheet would use; this site defines nothing for it.

Cross-references
================

"Reference" gives a dropdown a ``:name:`` and links to it. The name becomes the
``id`` of the ``<details>``, so a link lands on the summary bar:

.. dropdown:: ``:name: dropdown-overview-target``
   :name: dropdown-overview-target
   :icon: link

   Targets written *inside* a dropdown work too, since the body is ordinary
   content.

A link to the dropdown above: :ref:`dropdown-overview-target`.

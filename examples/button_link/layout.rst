.. _button-link-layout:

======
Layout
======

Alignment
=========

``:align:`` takes ``left``, ``right``, ``center`` or ``justify``. It is
written on the *paragraph* holding the button rather than on the button
itself, which is how sphinx-design builds it — so it aligns the button within
the line it occupies:

.. button-link:: https://example.com/left
   :align: left
   :color: primary

   left

.. button-link:: https://example.com/center
   :align: center
   :color: primary

   center

.. button-link:: https://example.com/right
   :align: right
   :color: primary

   right

Expanding
=========

``:expand:`` fills the width available. The button is wrapped in a grid
container, so the width belongs to the container and the button fills it:

.. button-link:: https://example.com/expanded
   :expand:
   :color: success

   This button fills the width of its container

Shadow and tooltip
==================

``:shadow:`` draws the button raised, and ``:tooltip:`` becomes the ``title``
attribute — a native tooltip, so hovering shows it:

.. button-link:: https://example.com/raised
   :color: info
   :shadow:
   :tooltip: Shown by the browser on hover

   Raised, with a tooltip

Clickable parent
================

``:click-parent:`` stretches the button's hit area over its whole positioned
ancestor, which is how a card becomes clickable through the button inside it.
Inside a ``.. grid-item::`` — the shape it is written in most often:

.. grid::

   .. grid-item::
      :outline:

      A cell whose whole area is clickable, because the button in it carries
      ``:click-parent:``.

      .. button-link:: https://example.com/stretched
         :color: primary
         :click-parent:

         Click anywhere in this cell

Extra classes
=============

``:class:`` adds classes to the button itself — the escape hatch a project's
own stylesheet hooks into. This site defines nothing for the name below, so
the button renders unchanged:

.. button-link:: https://example.com/classed
   :color: dark
   :class: example-button

   With an extra class

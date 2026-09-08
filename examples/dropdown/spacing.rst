.. _dropdown-spacing:

Dropdown Spacing and Classes
============================

``:margin:`` takes either one value, applying to every side, or exactly four,
read as *top bottom left right* — note the order, which is neither CSS's
clockwise one nor alphabetical. Each value is ``auto`` or a step from ``0`` to
``5``. Writing no ``:margin:`` is not the same as writing ``0``: the default
keeps a bottom margin so consecutive dropdowns do not touch.

.. dropdown:: One value, every side
   :margin: 4

   ``:margin: 4``

.. dropdown:: Four values, top bottom left right
   :margin: 0 4 auto auto
   :color: info

   ``:margin: 0 4 auto auto`` — no space above, a wide gap below, and centred
   horizontally.

The three class options are the escape hatch for anything the vocabulary above
does not cover. Each lands on its own element: the ``<details>``, the
``<summary>``, and the content ``<div>``.

.. dropdown:: Carrying custom classes
   :class-container: example-dropdown-container
   :class-title: example-dropdown-title
   :class-body: example-dropdown-body

   The stylesheet this site ships defines none of these three, so they change
   nothing here — they are what a project's own CSS would hook into.

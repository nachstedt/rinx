.. _dropdown-icons:

Dropdown Icons
==============

``:icon:`` draws a GitHub octicon before the title. The name is checked while
parsing, so a misspelled one is reported as ``dropdown.unknown-icon`` rather
than silently drawing nothing.

.. dropdown:: A hint
   :icon: light-bulb
   :color: success

   ``:icon: light-bulb``, the icon the sphinx-needs demo uses for its own
   hints.

.. dropdown:: An experiment
   :icon: beaker

   ``:icon: beaker``

.. dropdown:: A warning sign
   :icon: alert
   :color: warning

   ``:icon: alert``

An icon combines with everything else, including a title carrying its own
markup:

.. dropdown:: Reading ``conf.py``
   :icon: file-code
   :open:

   The icon sits before the title text; the state marker stays on the right.

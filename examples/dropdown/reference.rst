.. _dropdown-reference:

Referring to a Dropdown
=======================

``:name:`` makes a dropdown a cross-reference target, exactly like the
``:name:`` on an image, a table or a code block. The name becomes the ``id`` of
the ``<details>`` element, so a link lands on the summary bar.

.. dropdown:: The installation steps
   :name: dropdown-installation
   :icon: package
   :open:

   Anything can go here; what matters is that the dropdown above can be linked
   to.

A link to it: :ref:`dropdown-installation`.

Targets written *inside* a dropdown work too, since the body is ordinary
content:

.. dropdown:: Holding a target

   .. _dropdown-inner-target:

   This paragraph follows a target written inside the dropdown's body.

A link to that one: :ref:`dropdown-inner-target`.

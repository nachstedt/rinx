.. _dropdown-basics:

Dropdown Basics
===============

A dropdown's argument is its title, and everything indented under it is the
body. Both are ordinary reStructuredText.

.. dropdown:: A plain dropdown

   The body is parsed as normal block content, so it can hold anything —
   lists, code blocks, even other directives.

   * a bullet
   * another one

The title is parsed as inline markup, which no other directive caption in this
build does:

.. dropdown:: See ``rinx.toml`` for the *full* list

   A literal and an emphasis, both inside the summary bar.

``:open:`` starts the dropdown expanded rather than collapsed.

.. dropdown:: Open from the start
   :open:

   Visible without clicking anything.

Written with no argument at all, the summary shows a placeholder icon instead
of an empty bar, so there is still something to click.

.. dropdown::

   No title was written for this one.

A dropdown is a container, so a directive written inside it really is parsed —
this is the whole reason the build supports it rather than treating it as an
unknown name:

.. dropdown:: Containing another directive

   .. note::

      An admonition nested inside a dropdown.

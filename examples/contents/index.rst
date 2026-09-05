.. _contents-index:

####################
Contents Directives
####################

Every ``.. contents::`` option, so the rendered site exercises each of them.
The one below lists the whole page, which is the common case; every other
example further down is scoped with ``:local:`` to its own subsections, so
they each demonstrate one option in isolation instead of repeating the whole
page's table of contents over and over.

.. contents::

Explicit title
==============

An argument overrides the default title.

.. contents:: Table of Contents
   :local:

Explicit Title Target
----------------------

Text.

Depth limit
===========

``:depth: 1`` stops one level short of "Deeper Still" below.

.. contents::
   :local:
   :depth: 1

Depth Target
------------

Text.

Deeper Still
~~~~~~~~~~~~

Not listed above: ``:depth: 1`` stopped at "Depth Target".

Local
=====

Listing only "Local"'s own subsections rather than the whole page — and,
since no title is given, carrying no title of its own either.

.. contents::
   :local:

Nested One
----------

Text.

Nested Two
----------

Text.

Backlinks
=========

``:backlinks:`` controls whether a heading links back to the table of
contents that lists it. Each variant below is scoped with ``:local:`` so the
three tables of contents do not overlap.

Entry, the default
-------------------

Each heading below links back to its own entry above.

.. contents::
   :local:
   :backlinks: entry

Entry Target
~~~~~~~~~~~~

Text.

Top
---

Every heading below links back to this table of contents itself, rather than
to its own entry.

.. contents::
   :local:
   :backlinks: top

Top Target
~~~~~~~~~~

Text.

None
----

No backlinks at all.

.. contents::
   :local:
   :backlinks: none

None Target
~~~~~~~~~~~

Text.

Class and name
==============

``:class:`` adds a CSS class to the rendered wrapper, and ``:name:`` makes the
table of contents itself a reference target: :ref:`named-contents`.

.. contents::
   :local:
   :class: wide
   :name: named-contents

Named Target
------------

Text.

.. _sibling-index:

A sibling site
==============

This page belongs to a *second* site, ``//examples/intersphinx:sibling_site``,
built separately from ``//examples:site``. It links into that site through
the ``objects.inv`` every site writes — the other half of intersphinx:

* The example site's home page: :ref:`home-index`.
* Its page on these very links: :ref:`intersphinx-example`.
* A Python class it documents: :py:class:`Greeter`.
* A glossary term it defines: :term:`environment`.
* Any of those through ``:any:``, which searches every kind of entry the
  inventory lists: :any:`Greeter`.
* A whole page, through ``:external:doc:`` — a bare ``:doc:`` never leaves
  this site, as in Sphinx — named as the inventory lists it, from the other
  site's source root: :external:doc:`examples/doc_role`.

None of those targets is defined anywhere in this site. Its ``inventories``
name the example site's inventory with a site-relative base URL, so the links
work wherever the two sites are deployed side by side.

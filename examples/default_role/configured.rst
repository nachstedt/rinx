.. _default-role-configured:

########################
A Configured Default
########################

This document's library sets ``default_role = "any"``, so bare text is a
cross-reference, as if written with ``:any:``:

* A label: `home-index`.
* With an explicit title: `the interpreted text page <default-role-index>`.

``.. default-role::`` still overrides the library's choice for the rest of
the document, and without an argument restores ``title-reference`` rather
than the library's default:

.. default-role::

`Dune` is a title here.

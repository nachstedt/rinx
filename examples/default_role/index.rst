.. _default-role-index:

####################
Interpreted Text
####################

Text between single backquotes, with no role written, is *interpreted text*:
it is read as the *default role*. Unless a library or a document chooses
another, that is ``title-reference``, which marks the title of a work.

Titles
======

* Bare: `The Left Hand of Darkness` and `Dune` are novels.
* The same role written out, under all three of its names:
  :title-reference:`Solaris`, :title:`Neuromancer` and :t:`Foundation`.
* The text is plain, so markup inside it stays text: `*not* emphasis`.
* Other backquoted constructs are unaffected: a ``literal``, a
  `hyperlink <https://docutils.sourceforge.io/>`_ and a role such as
  :ref:`home-index`.

A Role After the Text
=====================

A role may be written after the text instead of before it, which reads the
same: water is H\ `2`:sub:\ O, and E = mc\ `2`:sup:.

Choosing the Default Role
=========================

``.. default-role::`` changes the default for the rest of the document.

.. default-role:: sup

Now bare text is raised: x\ `2` + y\ `2`.

Any role works, including one the document defines:

.. role:: chem(sub)

.. default-role:: chem

C\ `6`\ H\ `12`\ O\ `6`.

Without an argument, the directive restores ``title-reference``:

.. default-role::

`Dune` is a title again.

A Library's Default Role
========================

A library may set ``default_role`` in its ``BUILD.bazel``, as a Sphinx project
sets it in ``conf.py``:

.. toctree::

   configured

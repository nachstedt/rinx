See Also Directive
==================

The ``.. seealso::`` directive points readers to related resources.
It always renders with the fixed title **"See also"**.

Basic Usage
-----------

A simple paragraph body:

.. seealso::

   The :ref:`admonitions` page covers related block-level elements.

Bullet List Body
----------------

The body can also contain a bullet list:

.. seealso::

   * `Sphinx seealso documentation <https://www.sphinx-doc.org/en/master/usage/restructuredtext/directives.html#directive-seealso>`_
   * The :ref:`version-changes` page for version-related directives.

Multiple Paragraphs
-------------------

Multiple paragraphs are supported in the body:

.. seealso::

   First related resource: the admonitions showcase.

   Second related resource: the inline roles page.

Empty Body
----------

An empty body is tolerated without errors:

.. seealso::

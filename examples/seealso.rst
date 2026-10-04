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
   * Sphinx_ itself — here, its page on directives.

.. _Sphinx: https://www.sphinx-doc.org/en/master/usage/restructuredtext/directives.html

That last ``Sphinx_`` link goes to this page's own target, while the
:ref:`hyperlinks <hyperlinks-own-targets>` page defines ``Sphinx`` as the
project's home page: an external target belongs to the page it is written on.

Multiple Paragraphs
-------------------

Multiple paragraphs are supported in the body:

.. seealso::

   First related resource: the admonitions showcase.

   Second related resource: the inline roles page.

Content on the Directive Line
-----------------------------

As in Sphinx, text after the ``::`` is the first line of the body, and lines
indented below it continue the same paragraph:

.. seealso:: The :ref:`admonitions` page, in one line.

.. seealso:: The :ref:`version-changes` page, starting on the directive line
   and continued below it.

   A second paragraph follows after a blank line.

Empty Body
----------

An empty body is tolerated without errors:

.. seealso::

########################
Hyperlinks and Targets
########################

This page demonstrates the various hyperlink and target features now supported
by Rinx.

External Targets
================

You can define explicit external targets using the ``.. _name: URL`` syntax.
These can be on a single line or indented on the next line.

.. _Google: https://www.google.com
.. _Rust Language:
   https://www.rust-lang.org

Now we can link to Google_ or the `Rust Language`_ easily.

Embedded URIs
=============

You can also embed URIs directly within a phrased link using the 
``` `Text <URL>`_ ``` syntax.

Check out the `Sphinx Documentation <https://www.sphinx-doc.org>`_ for more 
details on reStructuredText.

Relative and Wrapped Embedded URIs
----------------------------------

An embedded URI may be relative, such as a fragment of this page:
`back to the top <#hyperlinks-and-targets>`_. The link text may also wrap
over a line break before its `embedded
URI <https://docutils.sourceforge.io/docs/ref/rst/restructuredtext.html#embedded-uris-and-aliases>`_,
and a URI that is all there is shows itself: `<https://www.python.org>`_.

The text of an embedded reference names a target of its own, so
`Sphinx Documentation`_ links to the URI embedded above.

Aliases and Indirect Targets
----------------------------

Text ending in an underscore inside ``<…>`` names another target instead of
a URI: `the language <Rust Language_>`_ links where ``Rust Language`` does.
A target can point at another target in the same way:

.. _Rust: `Rust Language`_

so Rust_ leads to the Rust site as well.

Phrased and Simple Links
========================

Phrased links use backticks and an underscore: `Python Website`_.
Simple links are just a word followed by an underscore: Sphinx_.

.. _Python Website: https://www.python.org
.. _Sphinx: https://www.sphinx-doc.org

Ordinary identifiers that merely contain underscores, such as
``my_variable_name`` or ``foo_bar_baz``, are left as plain text and are not
treated as references, since they don't end in a trailing underscore at a
word boundary.

.. _hyperlinks-own-targets:

Targets Belong to Their Page
============================

An external target is local to the page that writes it, as in docutils:
the :doc:`seealso` page defines its own ``Sphinx`` target with a different
URL, and each page's ``Sphinx_`` links use that page's definition. A named
reference never reaches another page — that is what ``:ref:`` is for, as in
:ref:`home-index`.

Section titles are targets of their own page too: `Embedded URIs`_ links to
the section above.

Case Insensitivity and Normalization
====================================

Target names are normalized. This means `GOOGLE`_, `google`_, and `Google`_ 
all point to the same target defined above. 

Internal References
===================

Of course, standard internal references still work: :ref:`home-index`.

Anonymous Hyperlinks
====================

Anonymous hyperlinks use a double underscore (``__``). They are resolved 
based on their order of appearance.

The first anonymous link points to the first anonymous target: `Example 1`__.
The second anonymous link points to the second anonymous target: link2__.

.. __: https://example.com/one
.. __: https://example.com/two

You can also use embedded URIs anonymously: `Google Search <https://google.com>`__.
These do not consume targets from the list above.

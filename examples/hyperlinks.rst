########################
Hyperlinks and Targets
########################

This page demonstrates the various hyperlink and target features now supported
by Rusty-Sphinx.

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

Phrased and Simple Links
========================

Phrased links use backticks and an underscore: `Python Website`_.
Simple links are just a word followed by an underscore: Sphinx_.

.. _Python Website: https://www.python.org
.. _Sphinx: https://www.sphinx-doc.org

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

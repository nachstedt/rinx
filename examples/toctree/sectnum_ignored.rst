Third Numbered Chapter
======================

Reached from the same ``:numbered:`` toctree as "First Numbered Chapter" and
"Second Numbered Chapter", and also writing its own ``.. sectnum::`` below —
which is entirely ignored, because the toctree's numbering already covers
this document. Compare with ``//examples/sectnum``, where the very same
directive, on a document no numbered toctree reaches, numbers its sections
instead.

.. sectnum::
   :prefix: Ignored
   :start: 100

Configuring
-----------

Still numbered by the toctree, continuing from "Second Numbered Chapter" —
not reset to "Ignored100." by the ``.. sectnum::`` above.

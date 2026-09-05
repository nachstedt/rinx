Prefix and Suffix
==================

.. sectnum::
   :prefix: (
   :suffix: )

``:prefix:`` and ``:suffix:`` wrap literal text around every number this
directive renders — here, "(1.)" and "(2.)" rather than plain "1." and "2.".
An option's value is always trimmed of leading/trailing whitespace, matching
every other directive option in this build, so a `:prefix:`/`:suffix:` meant
to carry a trailing/leading space (docutils' own examples often prepend
"Appendix " this way) cannot: choose surrounding punctuation instead, as
below, rather than relying on whitespace.

Installing
----------

Numbered "(1.)".

Configuring
-----------

Numbered "(2.)".

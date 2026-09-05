Substitutions
=============

A substitution definition, ``.. |name| directive::``, gives a name to
whatever an embedded directive produces; ``|name|`` anywhere in the document's
text is replaced with it. Three of docutils' substitution-only directives are
supported: ``replace``, ``unicode`` and ``image``.

Simple text substitution
-------------------------

``replace`` is a simple macro: whatever follows ``::`` becomes the
substitution's content, and it may contain inline markup of its own — the
one place this build allows a piece of text to gain markup it wasn't written
with directly.

This project is called |project name|, and the current release is |release|.

.. |project name| replace:: **rusty-sphinx**
.. |release| replace:: 3.13.0

A definition may be written anywhere in the document — including, as above,
*after* its only use. Real projects usually collect every definition in one
place, such as the bottom of the page or a shared included file.

Unicode characters
-------------------

``unicode`` decodes decimal numbers, hexadecimal numbers (prefixed by
``0x``, ``x``, ``\x``, ``U+``, ``u`` or ``\u``), or XML-style hexadecimal
character entities, into the literal characters they name. Anything from a
literal ``" .. "`` onward is a comment, not part of the codes.

Copyright |copy| 2024. This project is a trademark |tm|.

.. |copy| unicode:: 0xA9 .. copyright sign
.. |tm| unicode:: U+2122

``:ltrim:`` and ``:rtrim:`` (or ``:trim:`` for both) strip whitespace from the
text immediately surrounding a reference at its point of use — handy for a
substitution that should sit flush against its neighbours despite being
written with spaces around it for readability.

Left |nbsp| Right — written with an ordinary space on each side (needed for
the reference to be recognized at all), both stripped by ``:trim:`` so only
the non-breaking space itself separates the two words.

.. |nbsp| unicode:: 0xA0
   :trim:

Inline images
--------------

``image`` makes a picture part of running text rather than a block of its
own. Every option ``.. image::`` takes applies here too, with two narrowings
in the other direction: the three *vertical* alignments (``top``, ``middle``,
``bottom``) are additionally accepted, since an inline image has a text
baseline to align to, but ``:name:`` is refused — a substitution may be
referenced more than once, while a name must be unique.

|logo| — the rusty-sphinx logo, inline with this text.

.. |logo| image:: data/logo.svg
   :alt: The rusty-sphinx logo
   :align: middle
   :width: 24px

Substitutions and inline markup do not nest: a substitution reference written
inside emphasis or a literal is not recognized as one, matching every other
inline markup construct.

A ``|name|`` matching no definition anywhere in the document — or matching
one only case-insensitively, with more than one candidate — is reported as a
warning rather than silently vanishing, and rendered as the literal text it
was written as.

.. _headings-example:

Section headings
================

A section heading is a line of text with a run of one repeated punctuation
character under it, and optionally an identical run above it. The character
picks the level: the first adornment style a document uses is level 1, the
next new one level 2, and so on.

Overlined headings
------------------

The three-line form is a style of its own, distinct from an underline of the
same character, so the two get different levels even where the character
matches.

++++++++++++++++++
Overlined and deep
++++++++++++++++++

An overline and its underline must be the same length; the title is measured
against them the same way.

Titles wider than they look
---------------------------

The adornment is measured against the title's **display width**, not against
the bytes it takes to encode. That is what lets an author draw the underline
under what they actually see, whatever the title is written in:

🔍 An emoji-prefixed title
~~~~~~~~~~~~~~~~~~~~~~~~~~

漢字 in a title
~~~~~~~~~~~~~~~

Inline markup in a title
------------------------

Markup delimiters are part of the source line, so they count toward the width
the underline has to cover, even though the reader never sees them.

A **bold** word in a title
~~~~~~~~~~~~~~~~~~~~~~~~~~

Underlines that are too short
-----------------------------

An underline narrower than its title still makes a heading, and the build
reports ``heading.underline-too-short`` rather than silently turning the pair
into a paragraph. The comment below silences that warning for the heading
after it, so this page stays clean:

.. noqa: heading.underline-too-short

A deliberately under-drawn heading
~~~~~~~~~~~~

Below four characters, though, the run is too short to be read as an
adornment at all, and the two lines stay ordinary text:

Not a heading
~~~

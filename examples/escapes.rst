Backslash Escapes
=================

A backslash turns the character after it into ordinary text, stripping itself
from the output. It is how you write a character that would otherwise be
markup.

Escaping Markup Characters
--------------------------

Keep \*stars\* and \`\`backticks\`\` as literal characters, while **strong**
and *emphasis* next to them still work normally. A doubled backslash writes
one backslash: 2 \\ 3.

The Escaped Space
-----------------

A backslash before a space removes both, which is how markup gets attached to
neighbouring text without a visible gap:

- ``foo\ *bar*\ baz`` renders as foo\ *bar*\ baz — one word, with the middle
  in italics.
- Without the escapes, ``foo *bar* baz`` renders as foo *bar* baz.

The same escape marks a deliberately empty cell in the first column of a
simple table, where a blank cell would otherwise be ambiguous:

=======  =========================
Setting  Note
=======  =========================
verbose  Enables extra output.
\        A continuation of the row
         above, with no setting of
         its own.
=======  =========================

Escapes Are Not Processed in Literals
-------------------------------------

Inline literals take their content verbatim, so a backslash inside one stays
put: ``C:\path\to\file`` and ``re.compile(r"\d+")``.

The same is true of literal blocks::

    grep -oP '\d+\.\d+' file.txt

Escaping Typography
-------------------

Smart typography sees the escapes too, so an escaped \-\- pair stays two
hyphens while a real -- pair becomes an en dash, and escaped \.\.\. dots stay
three dots while real ... dots become an ellipsis.

Escapes in Roles
----------------

A role's content is un-escaped before the role interprets it, so
:func:`spawn\* <spawnl>` displays ``spawn*`` while still resolving the target
``spawnl``.

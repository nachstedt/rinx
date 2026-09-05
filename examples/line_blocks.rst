Line Blocks
=============

A line block is a sequence of lines each introduced by ``|``, rendered
without the paragraph reflow ordinary text gets — the construct RST uses for
poetry and addresses.

A Plain Line Block
---------------------

| Lend us a couple of bob till Thursday.
| I'm absolutely skint.
| But I am expecting a postal order and cheque, so I can pay you back.
| Love, Ewan.

A Deliberately Blank Line
-----------------------------

A bare ``|`` with nothing after it is a deliberately blank line in the
output — unlike a genuinely blank source line, which would end the block
instead.

| The first stanza.
|
| The second stanza, set apart by a blank line of its own.

Nesting
----------

Indenting the text after a ``|`` — not the ``|`` itself — nests that line one
level deeper than its siblings:

| Take it, all!
|   For nature could not choose your favour lose
| I am content to bear the loss of him.

Multiple Nesting Levels
---------------------------

Each further increase in indentation nests one level deeper still, and a
return to a shallower indentation closes those levels again:

| Level zero.
|   Level one.
|     Level two.
| Back to level zero.

A Wrapped Continuation Line
-------------------------------

A physical line with no ``|`` of its own, indented to align under the
previous line's text, continues that same logical line rather than starting
a new one:

| A signature line that runs on for quite a while and has to wrap
  onto a second physical line to fit.

A Line Block Inside a Block Quote
-------------------------------------

Indenting a line block without any enclosing directive nests it inside a
block quote, exactly as it does for a bullet list or a directive (see
``block_quotes.rst`` for the same behavior from that construct's side).

    | Indented once,
    | this line block sits inside a block quote.

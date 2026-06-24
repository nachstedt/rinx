Comments
========

RST comments are introduced with ``..`` and produce no output.

.. This is a single-line comment. It will not appear in the rendered HTML.

The paragraph above the comment and the paragraph below it are both rendered
normally; the comment itself is invisible.

..
   This is a multi-line comment.
   All indented lines belong to the comment body and are discarded.
   Nothing from this block appears in the output.

.. Another comment with inline text
   and a continuation body on the following indented lines.
   Still entirely invisible in the rendered output.

..

That last bare ``..`` with no body at all is also a valid comment.

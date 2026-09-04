Option Lists
=============

An option list documents a program's command-line options: one or more
comma-separated flags, at least two spaces, then a description (which may
also start on the next, more-indented line instead).

Synonyms and a Space-Delimited Argument
----------------------------------------

-h, --help            Show this help message and exit.
-o FILE, --output FILE
                       Write output to FILE.

An Equals-Delimited Long Option
---------------------------------

--output=FILE  Write output to FILE, using the ``=`` form.

An Adjacent Short Option
--------------------------

-oFILE  Write output to FILE, with no space or ``=`` between the flag and
        its argument.

DOS-Style and Old GNU-Style Markers
--------------------------------------

/Wall  Enable all compiler warnings (DOS/VMS-style marker).
+x     An old GNU-style option (rare, but part of the spec).

A Bracketed Placeholder Argument
-----------------------------------

-o <value1 value2>  A placeholder that itself contains spaces must be
                    wrapped in angle brackets.

Description Starting on the Next Line
-----------------------------------------

--long
    A long option with no text on its own line: the description starts
    entirely on the next, more-indented line instead.

A Multi-Paragraph Description
---------------------------------

--verbose  The first paragraph of the description.

           A second paragraph, still part of the same description.

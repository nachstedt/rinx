Smart Typography
=================

Rusty-Sphinx automatically converts a few plain-text punctuation patterns
into their typographic equivalents, mirroring Sphinx's default
``smartquotes`` behavior.

Em Dash
-------

Three consecutive hyphens become an em dash: wait---that's the point.

En Dash
-------

Two consecutive hyphens become an en dash, e.g. for ranges: see pages 10--20.

Ellipsis
--------

Three consecutive dots become an ellipsis: to be continued...

Inside Emphasis
----------------

The conversion also applies inside emphasis and strong emphasis: this is
**really---important**, or *wait... really?*

Not Inside Literals
--------------------

Literal text is left untouched: ``git log a---b`` and ``10--20`` stay
verbatim inside ``double backtick`` spans.

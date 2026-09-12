.. _unknown-directives-example:

Unrecognized and malformed directives
=====================================

rusty-sphinx implements a subset of reStructuredText and Sphinx, so a document
may well name a directive this build does not know. Such a directive is never
dropped silently: the page shows what could not be rendered, quoting the source
verbatim, and the build reports it with a code a ``.. noqa:`` can name.

That matters more than it looks like it should. An unrecognized directive's body
is never parsed, so everything written inside it — prose, targets, nested
entities — leaves the page with it. Without the block below, a reader would have
no way to tell that anything was missing at all.

An unrecognized name
--------------------

``.. mermaid::`` is not implemented here. The directive is reported as
``directive.unknown`` and rendered as an error block holding its source.

.. noqa: directive.unknown

.. mermaid:: A flowchart

   graph TD;
     A[Start] --> B{Works?};
     B -->|yes| C[Ship it];
     B -->|no| A;

A recognized name this build had to refuse
------------------------------------------

A directive whose *name* is known but whose content cannot be used is reported
against what is actually wrong with it, and the block says the same thing the
build log does — never "unknown directive", which would blame the name.

``.. figure::`` needs an image path as its argument:

.. noqa: image.missing-uri

.. figure::

   A caption for a picture that was never named.

``.. table::`` needs its content to be exactly one table:

.. noqa: table.directive.not-a-table

.. table:: Not a table at all

   Just a paragraph.

The suppression silences the warning, not the block
---------------------------------------------------

Both blocks above are written under a ``.. noqa:``, because this site is built
with ``strict_links = True`` and a clean build is worth keeping. The comment
takes the *warning* out of the build log — it does not take the author's text
off the page. Suppressing a diagnostic is saying "I know", not "hide this from
my readers".

See :ref:`noqa-example` for the suppression mechanism itself.

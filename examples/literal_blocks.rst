.. _literal-blocks:

##############
Literal Blocks
##############

Literal blocks render content verbatim inside a ``<pre>`` element. There are
two ways to introduce them in reStructuredText.

Like Sphinx, a ``::`` block is still syntax-highlighted: it names no language
of its own, so it takes whichever one ``.. highlight::`` last set.

Plain ``::`` Introduction
-------------------------

A paragraph ending with ``::`` introduces the following indented block as a
literal block. The ``::`` is either removed entirely (standalone) or reduced
to a single ``:`` (when it follows text)::

    This is verbatim text.
    It preserves all spacing and    gaps.

    Blank lines inside the block are also preserved.

A standalone ``::`` (with no preceding text) silently introduces the block
without rendering any paragraph at all:

::

    No paragraph is emitted for a standalone "::".
    Only this literal block appears.

``.. code-block::`` Directive
------------------------------

The ``.. code-block::`` directive takes a language and every presentation
option Sphinx defines. Highlighting happens during the build, into HTML
carrying one CSS class per token — see :ref:`code-blocks` for the full set of
options.

Python example:

.. code-block:: python

    def greet(name: str) -> str:
        return f"Hello, {name}!"

    print(greet("World"))

Bash example:

.. code-block:: bash

    #!/usr/bin/env bash
    set -euo pipefail

    for file in *.rst; do
        echo "Processing $file"
    done

No-language code block (``.. code-block::`` without an argument):

.. code-block::

    plain verbatim content
    no language class is emitted

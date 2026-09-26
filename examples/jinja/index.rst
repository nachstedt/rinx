{% set page="jinja/index.rst" %}
{% include "examples/jinja/_templates/page_header.rst" with context %}

.. _jinja-index:

🧩 Templated Sources
====================

Real Sphinx projects template their ``.rst`` files by connecting the
``source-read`` event in ``conf.py`` and running each file through Jinja2.
There is no Sphinx feature for it and rinx cannot run a ``conf.py``, so
the same transform is a declared step of the build instead: set ``jinja = True``
on the ``rinx_library``, and every source in it is rendered as a Jinja
template before it is parsed.

It is off by default. A document is free to contain ``{% raw %}{{{% endraw %}`` and
``{% raw %}{%{% endraw %}`` as text, and most projects mean nothing by them.

.. toctree::
   :maxdepth: 1

   loops
   literal

Including a shared fragment
---------------------------

The note at the top of this page is the whole reason the feature exists. This
document opens with

.. code-block:: jinja

   {% raw %}{% set page="jinja/index.rst" %}
   {% include "examples/jinja/_templates/page_header.rst" with context %}{% endraw %}

A template name resolves against the **source root**, not against the document
that includes it — which is why the path is spelled in full here, and why the
same spelling works from every page however deep. That is Jinja's own rule: the
``FileSystemLoader`` a ``conf.py`` builds is rooted at the Sphinx source
directory. A ``.. include::`` is the other way round, resolving against the
file it is written in, as docutils does.

The template is declared in the library's ``parse_data``, like every other file
the parser reads, and deliberately **not** in ``srcs`` — listing it there would
publish it as a page of its own as well as rendering it into this one.

Where a diagnostic points
-------------------------

An ``{% raw %}{% include %}{% endraw %}`` splices a whole file in, so everything below it moves.
The build tracks that: a warning raised on this line names *this* document and
this line, and one raised inside the header names the header. The same holds
for a ``.. noqa:`` comment, which only silences diagnostics from the file it
was written in.

What is refused
---------------

Three constructs are refused by name rather than rendered into something
subtly wrong:

* **Whitespace-control modifiers** (``{% raw %}{%-{% endraw %}``,
  ``{% raw %}-%}{% endraw %}``), because in reStructuredText a silently
  changed indent changes what a block contains.
* **A computed template name**, because the build declares every file an
  action reads before it runs.
* **An undefined value**, which Jinja2 would substitute with the empty string.

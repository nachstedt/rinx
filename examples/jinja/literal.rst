{% set page="jinja/literal.rst" %}
{% include "examples/jinja/_templates/page_header.rst" with context %}

.. _jinja-literal:

Jinja That Is Not Jinja
=======================

A page documenting Jinja has to show its syntax without it being rendered.
``{% raw %}{% raw %}{% endraw %}`` does that, exactly as it does in Jinja2:

.. code-block:: jinja

   {% raw %}{{ this_name_is_never_looked_up }}{% endraw %}

The text between the two markers is copied through untouched, so an undefined
name inside it is never looked up and never reported.

A library that does not set ``jinja = True`` needs none of this: its sources
are parsed exactly as written, and ``{% raw %}{{{% endraw %}`` is an ordinary pair of braces.

{% set page="jinja/loops.rst" %}
{% include "examples/jinja/_templates/page_header.rst" with context %}

.. _jinja-loops:

Generating Repetitive Content
=============================

The other thing projects template for: writing a table or a list once and
letting the loop repeat it.

.. code-block:: jinja

   {% raw %}{% for name, purpose in [("srcs", "the documents"),
                          ("parse_data", "the files the parser reads")] %}
   ``{{ name }}``
       {{ purpose }}
   {% endfor %}{% endraw %}

renders as the definition list below.

{% for name, purpose in [("srcs", "the documents this library publishes"), ("parse_data", "the files the parser reads while parsing them"), ("images", "the pictures its pages show")] %}
``{{ name }}``
    {{ purpose }}
{% endfor %}

Every generated line traces back to the line of the template that generated
it, so a mistake inside a loop body is reported once per iteration, on the
line that was actually written.

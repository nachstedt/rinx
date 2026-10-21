Drawn
=====

A diagram the preview draws itself, with no JVM.

.. plantuml::

   Alice -> Bob : Hello

A templated diagram whose second template line is no valid Jinja, underlined
on that line.

.. needuml::

   Alice -> Bob
   {{ unterminated

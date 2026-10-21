Diagrams
========

Each diagram asks the project something it cannot answer, which only
rendering against the whole of it reveals.

.. needuml::

   {{ need("R_GHOST").title }}

.. needuml::
   :caption: A template whose second line is no valid Jinja

   R_BRAKES -> S_PADS
   {{ unterminated

.. entity-flow::
   :filter: id == "R_NONE"

.. entity-sequence::
   :start: R_GHOST
   :relations: implements

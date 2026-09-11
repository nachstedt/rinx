.. _team-a-index:

Team A: Getting Started
=======================

Welcome to the Team A documentation.

This guide covers the public API owned by Team A.

Installation
------------

Download the latest package and follow the setup steps below.

.. plantuml::

    @startuml
    Alice -> Bob: Auth Request
    Bob --> Alice: Auth Response
    @enduml

.. note::

   Diagrams also work when nested inside another directive's body — the
   extraction and validation phases walk the whole document tree, not just
   its top level.

   .. plantuml::

       @startuml
       Client -> Gateway: Nested Request
       Gateway --> Client: Nested Response
       @enduml

.. uml::
   :caption: The same directive under sphinxcontrib-plantuml's shorter name
   :align: center
   :width: 400px
   :scale: 75
   :class: bordered
   :name: retry-flow

    @startuml
    Client -> Server: Request
    Server --> Client: 503
    Client -> Server: Retry
    @enduml

``.. uml::`` is the same construct as ``.. plantuml::`` above — one node, one
compiled picture — so a diagnostic about either names ``uml.*``. The options
here decide only how the finished picture sits on the page, which is why they
never reach the hash the SVG is named by. See :ref:`retry-flow`.

.. function:: int subtract(int a, int b)

   Subtracts ``b`` from ``a``. Written without an explicit domain prefix —
   Team A's library sets its ``default_domain`` Bazel attribute to ``c``,
   so this bare directive resolves to ``c:function`` rather than the global
   default of ``py``.

See :func:`subtract` for the implementation above.

See also :ref:`home-index`, :ref:`team-a-index`, and :ref:`team-b-index`.
 

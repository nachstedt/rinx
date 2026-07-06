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

.. function:: int subtract(int a, int b)

   Subtracts ``b`` from ``a``. Written without an explicit domain prefix —
   Team A's library sets its ``default_domain`` Bazel attribute to ``c``,
   so this bare directive resolves to ``c:function`` rather than the global
   default of ``py``.

See :func:`subtract` for the implementation above.

See also :ref:`home-index`, :ref:`team-a-index`, and :ref:`team-b-index`.
 

Requirements
============

An entity with every kind of declaration
----------------------------------------

.. req:: The system shall boot within two seconds
   :id: REQ_001
   :owner: platform
   :status: in_progress
   :tags: boot, kernel
   :links: SPEC_001, IMPL_001

   The unnamed leading prose is the *content section*. It is ordinary RST, so
   it may hold anything a document may hold.

   .. verification-criteria::

      Measured with the boot harness on the reference board. A section's body
      is fully parsed, which is what lets it carry a nested directive:

      .. note::

         The two-second budget excludes firmware hand-off.

   .. safety-comment::

      A boot timeout is treated as a hazard.

   .. safety-comment::

      Declared ``multiple``, so it may appear more than once.

Attribute defaults and a generated id
-------------------------------------

.. req:: Shut down cleanly on power loss
   :owner: platform

   No ``:id:`` was written, so one is generated — deterministically, from the
   document, the type and the source line. No ``:status:`` was written either,
   so it takes the schema's declared default.

   .. verification-criteria::

      Power is cut mid-write; the filesystem must mount cleanly afterwards.

Specifications and implementations
----------------------------------

.. spec:: Boot sequence
   :id: SPEC_001
   :status: approved
   :implemented_by: IMPL_001

   .. rationale::

      Ordering the probe phases lets the slowest one overlap with I/O.

An implementation's id and ``:module:`` follow the naming conventions its
type declares as patterns — ``IMPL_`` and three digits, and a dotted module
path. An ``:id: IMPL_1`` would be reported, and a ``:module: Boot/Loader``
refused.

.. impl:: Staged boot loader
   :id: IMPL_001
   :module: boot.loader

A one-directional relation and a required one
---------------------------------------------

.. test:: Boot timing regression test
   :id: TEST_001
   :verifies: REQ_001
   :automated: true

Referring to entities from prose
--------------------------------

A declared role carries a type check: :req:`REQ_001` links to the requirement
and shows its title. An explicit title overrides that, as in
:req:`the boot budget <REQ_001>`.

The sphinx-needs spelling :need:`SPEC_001` accepts any of the four types, and
the built-in :entity:`TEST_001` role accepts every type without being declared
at all. Because every entity also registers an ordinary target, plain
:ref:`REQ_001` reaches one too.

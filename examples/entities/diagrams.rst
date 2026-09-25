Diagrams over the entity graph
==============================

A diagram directive's body is PlantUML source. In the plain
``.. plantuml::``/``.. uml::`` spelling it reaches the compiler exactly as
written; in the three spellings below it is first expanded as a Jinja template
against the project's entity graph, so a picture can be *derived* from what the
documents declare rather than restated beside them.

Because the graph spans documents, expansion happens after the whole project
has been indexed — which is why diagram compilation belongs to the site rather
than to any one library.

Drawing what a filter selects
-----------------------------

``filter()`` takes the same expression language an ``.. entity-table::``'s
``:filter:`` is written in, and ``flow()`` turns an id into a clickable node.
Clicking one lands on exactly the anchor a ``:ref:`` to that entity would.

.. entity-diagram::
   :caption: Every requirement in the project
   :align: center

   @startuml
   {% for id in filter('type == "req"') %}
   {{ flow(id) }}
   {% endfor %}
   @enduml

``.. needuml::`` is accepted as a second spelling of the same directive, so a
migrating sphinx-needs project keeps its documents.

Reading one entity's fields
---------------------------

``needs`` holds every entity by id and ``need(id)`` fetches one. Attributes,
relations and derived back-links share the one namespace the directive's
options do, so ``.. req::``'s ``:owner:`` is read as ``need.owner``.

.. needuml::
   :caption: One requirement and what it links to
   :debug:

   @startuml
   {{ flow('REQ_001') }}
   note right of REQ_001
     owner: {{ need('REQ_001').owner }}
     status: {{ need('REQ_001').status }}
   end note
   {% for target in need('REQ_001').links %}
   {{ flow(target) }}
   REQ_001 --> {{ target }}
   {% endfor %}
   @enduml

``:debug:`` shows the expanded PlantUML below the picture — the text that was
actually compiled, which is what an author needs when a diagram comes out
wrong.

Extra values
------------

``:extra:`` binds names into the template's context, as comma-separated
``name: value`` pairs.

.. needuml::
   :extra: heading: Boot requirements, colour: LightBlue

   @startuml
   rectangle "{{ heading }}" #{{ colour }} {
     {{ flow('REQ_001') }}
   }
   @enduml

An architecture diagram inside an entity
----------------------------------------

``.. entity-arch::`` — sphinx-needs spells it ``.. needarch::`` — is written
*inside* an entity and binds that entity as ``need``. Writing one outside any
entity is reported as ``uml.arch-outside-entity`` rather than drawn against a
``need`` bound to nothing.

Its `:key:` stores the template on the entity, so another diagram can import
it. This is what makes architecture diagrams compositional: a component draws
itself once, and every diagram that reaches it pulls that picture in rather
than restating it.

.. req:: The bootloader shall verify the kernel signature
   :id: REQ_BOOT
   :owner: platform
   :links: SPEC_001

   .. entity-arch::
      :key: overview

      component "{{ need.title }}" as {{ need.id }}

   .. verification-criteria::

      A diagram written inside a *section* still knows the entity it sits in,
      even though a section's body deliberately forgets the entity's *type*:

      .. entity-arch::

         @startuml
         {{ flow(need.id) }}
         @enduml

Importing another entity's diagram
----------------------------------

``uml(id, key)`` expands the diagram another entity wrote, with ``need``
rebound to that entity. ``imports(id, relation)`` does the same for everything
an entity points at along a relation, skipping targets that drew nothing.

A diagram that reaches itself — directly, or around a cycle — is reported as
``uml.recursive-import`` with the route it took, rather than expanding until
the build dies.

.. entity-diagram::
   :caption: The bootloader's own picture, pulled in from where it was written

   @startuml
   {{ uml('REQ_BOOT', 'overview') }}
   @enduml

Named PlantUML preambles
------------------------

``:config:`` names a preamble declared under ``[uml_configs]`` in the site's
``rinx.toml`` — the preamble's *text*, never a path to a file holding
it, so it survives a sandboxed build relocating files. The preamble is inserted
directly after ``@startuml``, which is the only place PlantUML reads a
``skinparam`` from. Naming one that is not declared is
``uml.unknown-config`` rather than a diagram quietly missing its styling.

.. needuml::
   :config: monochrome
   :caption: The same graph, with the site's monochrome preamble applied

   @startuml
   {{ flow('REQ_001') }}
   @enduml

Flowcharts of the graph itself
------------------------------

``.. entity-flow::`` — sphinx-needs spells it ``.. needflow::`` — draws the
entities a filter selects and the relations between them, without a template.
The picture is *generated*: there is no body to write, and the directive
reports one rather than mistaking it for PlantUML source.

.. entity-flow::
   :caption: Every entity in the project, and every relation between them

Selecting a subgraph, and labelling its edges
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

``:filter:`` is the same expression language an ``.. entity-table::``'s is, and
``:relations:`` — sphinx-needs spells it ``:link_types:`` — says which relations
become edges. An edge whose target the filter left out is not drawn, so a
filtered picture stays a picture of what was asked for.

.. entity-flow::
   :filter: type == "req" or type == "spec"
   :relations: links
   :show-link-names:
   :direction: LR
   :caption: Requirements and the specifications they link to, laid out sideways
   :align: center
   :width: 600px
   :name: requirement-flow

The ``:name:`` above registers as an ordinary cross-reference target, so
:ref:`requirement-flow` links to the picture.

The generated source, and a preamble
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

``:debug:`` shows the PlantUML that was compiled — worth more here than on a
written diagram, since this text exists nowhere else — and ``:config:`` names a
preamble from the site's ``[uml_configs]``, exactly as it does above.

.. needflow::
   :filter: type == "test"
   :config: monochrome
   :debug:
   :caption: The test cases, in monochrome

A filter matching nothing is reported as ``entity-flow.empty-result`` rather
than compiled: PlantUML rejects an empty diagram, so the build would otherwise
fail with a syntax error naming a generated file nobody wrote.

.. noqa: entity-flow.empty-result

.. entity-flow::
   :filter: status == "withdrawn"
   :caption: Withdrawn requirements — there are none, so nothing is drawn

Sequence diagrams of the graph
------------------------------

``.. entity-sequence::`` — sphinx-needs spells it ``.. needsequence::`` — walks
the graph instead of filtering it. From each ``:start:`` entity, a relation
named in ``:relations:`` leads to a *message*, and the same relations lead on
from the message to its *receivers*; every hop is an arrow labelled with the
message's title, and every receiver not yet seen is walked in turn.

The components and messages below are ordinary entities, declared inside a
dropdown the way the sphinx-needs demo corpus declares its own.

.. dropdown:: The coffee machine's components and messages

   .. component:: User interface
      :id: COMP_UI
      :calls: MSG_INIT, MSG_ARM

   .. component:: Hardware abstraction layer
      :id: COMP_HAL
      :calls: MSG_INIT_OK, MSG_READING
      :stops: MSG_HALT

   .. component:: Safety monitor
      :id: COMP_SAFETY
      :calls: MSG_POLL, MSG_START_HEATER, MSG_READY

   .. component:: Heater
      :id: COMP_HEATER
      :calls: MSG_HEATER_UP

   .. message:: init()
      :id: MSG_INIT
      :calls: COMP_HAL

   .. message:: init_ok
      :id: MSG_INIT_OK
      :calls: COMP_UI

   .. message:: arm()
      :id: MSG_ARM
      :calls: COMP_SAFETY

   .. message:: poll_sensors()
      :id: MSG_POLL
      :calls: COMP_HAL

   .. message:: temp=22°C
      :id: MSG_READING
      :calls: COMP_SAFETY

   .. message:: start()
      :id: MSG_START_HEATER
      :calls: COMP_HEATER

   .. message:: heating
      :id: MSG_HEATER_UP
      :calls: COMP_SAFETY

   .. message:: ready
      :id: MSG_READY
      :calls: COMP_UI

   .. message:: emergency_stop
      :id: MSG_HALT
      :stops: COMP_HEATER, COMP_UI

.. entity-sequence:: Startup
   :start: COMP_UI
   :relations: calls
   :name: startup-sequence

The argument is the caption, as in sphinx-needs, and ``:name:`` registers the
picture as a cross-reference target: :ref:`startup-sequence`.

``:relations:`` is mandatory — sphinx-needs defaults it to ``links``, but
neither that nor "every relation" says which edges are messages. Several
starts share one walk, so a component the first start already reached is not
drawn twice:

.. needsequence:: Shutdown
   :start: COMP_HAL; COMP_UI
   :link_types: stops
   :align: center

Selecting receivers, and capping the picture
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

``:filter:`` keeps only the receivers it selects — a receiver it rejects gets
no arrow and is not walked on from. ``:max-items:`` draws only the first
messages and says so beneath the picture; the build warns as well, as
``entity-sequence.truncated``, which a deliberate cap silences.

.. entity-sequence::
   :start: COMP_UI
   :relations: calls
   :filter: id != "COMP_HEATER"
   :caption: Startup without the heater
   :width: 500px

.. noqa: entity-sequence.truncated

.. entity-sequence::
   :start: COMP_UI
   :relations: calls
   :max-items: 3
   :config: monochrome
   :debug:
   :caption: The first three messages of the startup, in monochrome

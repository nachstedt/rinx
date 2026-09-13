Charting the graph
==================

An ``.. entity-pie::`` asks the entity graph the question an
``.. entity-table::`` asks, and answers it as proportions instead of as rows.
Each line of its body is one filter, and the wedge it draws is how many
entities that filter selects.

``.. needpie::`` is accepted as a second spelling of the same directive, so a
project migrating from sphinx-needs keeps its documents unchanged.

Unlike the flowcharts and diagrams on the neighbouring page, a chart is drawn
while the page is rendered rather than compiled by a build action — so this
library sets no ``diagrams = True``, starts no JVM, and pays nothing for the
charts below beyond drawing them.

Counting by type
----------------

The argument is the chart's title, and ``:labels:`` names the wedges in the
order their filters were written.

.. entity-pie:: Entities by type
   :labels: Requirements, Specifications, Implementations, Tests

   type == "req"
   type == "spec"
   type == "impl"
   type == "test"

A legend and colours of your own
--------------------------------

``:legend:`` draws a key naming each wedge with its count, and ``:colors:``
replaces the built-in palette — as CSS hex or one of the basic colour
keywords. A ``:name:`` makes the chart a cross-reference target like any other
figure.

.. entity-pie:: Requirements by status
   :labels: Open, In progress, Closed
   :legend:
   :colors: #4c72b0, #dd852c, #55a868
   :caption: Where the requirements on this site stand
   :align: center
   :width: 520px
   :name: requirement-status-chart

   type == "req" and status == "open"
   type == "req" and status == "in_progress"
   type == "req" and status == "closed"

The chart above is :ref:`requirement-status-chart`.

Scoping a whole chart once
--------------------------

A ``:filter:`` narrows the entities before any wedge counts them, so a chart
about one part of the project need not repeat that condition on every line.
``startswith`` and ``endswith`` are the two string methods the filter language
has, which is how a chart selects by id prefix.

.. needpie:: Requirements by id prefix
   :filter: type == "req"
   :labels: REQ_, everything else
   :legend:

   id.startswith("REQ_")
   not id.startswith("REQ_")

Numbers written outright
------------------------

A body line that is a plain number is used as the wedge's size directly,
for a chart whose data does not come from the graph at all.

.. entity-pie:: Effort by phase (person-days)
   :labels: Analysis, Design, Implementation, Verification
   :legend:

   12
   18
   40
   25

When nothing matches
--------------------

A chart whose every wedge counts zero has nothing to draw, and that is
reported rather than left as a blank space — an empty chart is far more often
a filter that no longer matches than a deliberate statement. The suppression
below is what keeps this example page warning-free.

.. noqa: entity-pie.empty-result

.. entity-pie:: Entities of a type nothing declares
   :labels: Nothing

   type == "no-such-type"

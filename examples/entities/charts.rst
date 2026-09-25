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

Bar charts
----------

An ``.. entity-bar::`` (sphinx-needs' ``.. needbar::``) asks the same question
in two dimensions. Each body line is one *series* — a legend entry, drawn in one
colour — and each comma-separated cell of it is one *category* along the axis.
A cell is a filter whose count is the bar's height, or a number written
outright.

``:xlabels: FROM_DATA`` takes the category names from the first line and
``:ylabels: FROM_DATA`` the series names from the first cell of every line —
the shape sphinx-needs' own examples use. ``:stacked:`` piles a category's
series on top of each other, ``:show_sum:`` writes each bar's value inside it
and ``:show_top_sum:`` the stack's total above it.

.. needbar:: Work done and left to do
   :legend:
   :xlabels: FROM_DATA
   :ylabels: FROM_DATA
   :stacked:
   :show_sum:
   :show_top_sum:
   :name: status-bar-chart

   , Still to do, Done
   Requirements, type == "req" and status != "closed", type == "req" and status == "closed"
   Specifications, type == "spec" and status == "draft", type == "spec" and status == "approved"

The chart above is :ref:`status-bar-chart`.

Labels, axis titles and turned text
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

Labels may equally be written as options, pairing with the rows and columns by
position. Without ``:stacked:`` the series stand side by side. Axis titles
name the axes, and the three rotation options turn the category labels, the
value labels and the written values by any whole number of degrees.

.. entity-bar:: Entities by type
   :xlabels: Requirements, Specifications, Implementations, Tests
   :ylabels: Count
   :x_axis_title: Entity type
   :y_axis_title: Entities
   :xlabels_rotation: 30
   :show_top_sum:
   :caption: Every type this site declares, counted
   :align: center
   :width: 560px

   type == "req", type == "spec", type == "impl", type == "test"

Horizontal, transposed, and numbers written outright
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

``:horizontal:`` runs the bars rightwards, the first category at the top, and
``:transpose:`` swaps series and categories after the labels are read.
``:separator:`` changes what a body line is split on — useful when a filter
itself holds a comma; the label options are always comma-separated — and ``:colors:`` and ``:text_color:`` take the same spellings
a pie chart's do.

.. entity-bar:: Effort by phase (person-days)
   :xlabels: Analysis, Design, Build
   :ylabels: Planned, Spent
   :separator: ;
   :horizontal:
   :transpose:
   :legend:
   :show_sum:
   :sum_rotation: 0
   :colors: #8fb3de, #f2b880
   :text_color: #333333

   12; 18; 40
   10; 21; 35

Scoping a whole bar chart once
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

As on a pie, ``:filter:`` narrows the entities before any cell counts them.

.. entity-bar:: Requirements by owner and status
   :filter: type == "req"
   :xlabels: Open, In progress, Closed
   :ylabels: platform, everyone else
   :legend:
   :stacked:

   owner == "platform" and status == "open", owner == "platform" and status == "in_progress", owner == "platform" and status == "closed"
   owner != "platform" and status == "open", owner != "platform" and status == "in_progress", owner != "platform" and status == "closed"

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

The same holds for a bar chart whose every cell counts zero.

.. noqa: entity-bar.empty-result

.. entity-bar:: Entities of a type nothing declares

   type == "no-such-type", type == "nor-this-one"

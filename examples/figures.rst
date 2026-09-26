Figures
=======

A ``.. figure::`` is an image with a caption naming it and, optionally, a
legend explaining it. It takes every option ``.. image::`` takes, plus two of
its own.

A figure with a caption
-----------------------

The body's first paragraph becomes the caption.

.. figure:: data/logo.svg
   :alt: The rinx logo

   The rinx logo, drawn in **four** shapes.

A caption and a legend
----------------------

Everything after the caption is the legend, and it may hold any body content
at all — paragraphs, lists, even another directive.

.. figure:: data/logo.svg
   :alt: The rinx logo
   :width: 100px

   The logo at 100 pixels wide.

   The legend can explain the picture at length:

   - the circle is the sun
   - the triangle is a pyramid
   - the bar is an obelisk

A legend with no caption
------------------------

An empty comment as the first body element tells docutils that the paragraph
following it is legend rather than caption.

.. figure:: data/logo.svg
   :alt: The rinx logo

   ..

   This paragraph is legend, not a caption — note that it is not rendered as
   the figure's title.

Sizing the figure box
---------------------

``:width:`` sizes the picture; ``:figwidth:`` sizes the box around it, which is
what the caption wraps inside.

.. figure:: data/logo.svg
   :alt: A figure in a narrow box
   :width: 100%
   :figwidth: 200px

   A caption in a 200-pixel box, so it wraps rather than running the full
   width of the page.

``:figwidth: image`` makes the box exactly as wide as the picture it holds.

.. figure:: data/logo.svg
   :alt: A figure sized to its image
   :figwidth: image

   A caption no wider than the logo above it.

Alignment
---------

On a figure the alignment moves the whole box, caption included. As with
``.. image::``, left and right *float* the box so the text wraps beside it,
while centre leaves it in the normal flow.

.. figure:: data/logo.svg
   :alt: A left-aligned figure
   :align: left
   :figwidth: 200px

   Aligned left.

This paragraph wraps around the right-hand side of the figure above, caption
and all — the caption is part of the floated box, not a separate block left
behind in the flow.

Centre alignment
~~~~~~~~~~~~~~~~

.. figure:: data/logo.svg
   :alt: A centred figure
   :align: center
   :figwidth: 200px

   Aligned centre, and so is this caption.

A centred figure does not float, so this paragraph begins below it. The
caption is centred with the picture, which is why the alignment belongs on the
``<figure>`` rather than on the ``<img>`` inside it.

Right alignment
~~~~~~~~~~~~~~~

.. figure:: data/logo.svg
   :alt: A right-aligned figure
   :align: right
   :figwidth: 200px

   Aligned right.

This paragraph wraps around the left-hand side of the figure above.

Classes
-------

``:class:`` puts class names on the ``<img>``; ``:figclass:`` puts them on the
``<figure>`` around it.

.. figure:: data/logo.svg
   :alt: A figure with classes on both elements
   :class: bordered
   :figclass: framed

   The image is bordered and the figure is framed.

Referring to a figure
---------------------

``:name:`` makes the figure a target, so :ref:`the-logo-figure` reaches it.

.. figure:: data/logo.svg
   :alt: A named figure
   :name: the-logo-figure

   A figure that can be linked to by name.

A figure that is also a link
----------------------------

``:target:`` works here exactly as it does on an image: the picture becomes a
link, and the caption does not.

.. figure:: data/logo.svg
   :alt: A figure linking to a website
   :target: https://www.sphinx-doc.org/

   Clicking the picture opens the Sphinx website.

An embedded figure
------------------

``:loading: embed`` inlines the picture's bytes into the page, so the rendered
HTML needs no separate file to display it.

.. figure:: data/logo.svg
   :alt: An embedded figure
   :loading: embed
   :width: 80px

   This picture is carried by the page itself.

A figure with no body
---------------------

Both the caption and the legend are optional.

.. figure:: data/logo.svg
   :alt: A figure with neither caption nor legend
   :width: 60px

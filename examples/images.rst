Images
======

The ``.. image::`` directive puts a picture on the page with no caption. Its
argument is a path relative to this document, a path from the source root
(with a leading ``/``), or an external URL.

A bare image
------------

.. image:: data/logo.svg

Alternative text
----------------

``:alt:`` is what a screen reader announces. Writing ``:alt:`` with no value is
the deliberate spelling for a picture that carries no information of its own.

.. image:: data/logo.svg
   :alt: The rusty-sphinx logo

Sizing
------

``:width:`` takes a length or a percentage; ``:height:`` takes a length only.

.. image:: data/logo.svg
   :alt: The logo at a fixed width
   :width: 60px

.. image:: data/logo.svg
   :alt: The logo at half the available width
   :width: 50%

``:scale:`` multiplies whatever ``:width:`` and ``:height:`` were given.

.. image:: data/logo.svg
   :alt: The logo scaled to half of 120px
   :width: 120px
   :scale: 50

Alignment
---------

``:align: left`` and ``:align: right`` *float* the picture, which means the
text that follows flows around it rather than starting below it. That is what
alignment is for, so each example below is followed by enough prose to show
the wrapping.

.. image:: data/logo.svg
   :alt: A left-aligned logo
   :align: left

Floated to the left, so this paragraph wraps around its right-hand side. A
floated image is taken out of the normal flow of the page: the text does not
begin below it, it begins beside it, and only returns to the full width of the
page once it has passed the bottom of the picture. Notice that the heading
below still starts on a clean line — headings clear any float still standing,
so one section's picture can never intrude into the next.

Centre alignment
~~~~~~~~~~~~~~~~

``:align: center`` does not float. The image stays in the normal flow as a
block of its own, centred between the margins, and the text resumes below it.

.. image:: data/logo.svg
   :alt: A centred logo
   :align: center

This paragraph starts below the centred picture rather than beside it, which
is the whole difference between centring and floating.

Right alignment
~~~~~~~~~~~~~~~

.. image:: data/logo.svg
   :alt: A right-aligned logo
   :align: right

Floated to the right, so this paragraph wraps around its left-hand side. Two
images aligned to the same side stack one below the other rather than sitting
side by side, and a centred image between them starts below the float rather
than riding up alongside it — both of which depend on the ``clear`` rules in
the stylesheet.

Making the image a link
-----------------------

``:target:`` accepts a URL, or the name of a target elsewhere in the project
written with a trailing underscore.

.. image:: data/logo.svg
   :alt: The logo, linking to a website
   :target: https://www.sphinx-doc.org/

.. _the-image-target:

This paragraph is a link target, and the next image points at it.

.. image:: data/logo.svg
   :alt: The logo, linking to a target in this document
   :target: the-image-target_

Referring to an image
---------------------

``:name:`` makes the image itself a target, so :ref:`the-logo` reaches it.

.. image:: data/logo.svg
   :alt: A named logo
   :name: the-logo

Classes
-------

``:class:`` puts extra class names on the ``<img>`` element.

.. image:: data/logo.svg
   :alt: A logo with a custom class
   :class: bordered

How the bytes reach the page
----------------------------

``:loading:`` chooses between three answers. ``link`` is the default: the page
points at the bundled file.

.. image:: data/logo.svg
   :alt: A linked logo
   :loading: link

``lazy`` links the same way, and additionally sets HTML's own ``loading="lazy"``
attribute so the browser can defer fetching the picture until it is nearly on
screen.

.. image:: data/logo.svg
   :alt: A lazily loaded logo
   :loading: lazy

``embed`` puts the file's bytes into the page itself as a ``data:`` URI, so the
page needs no second request — and no second file — to show it.

.. image:: data/logo.svg
   :alt: An embedded logo
   :loading: embed

An external image
-----------------

An absolute URL is passed through untouched. Nothing is bundled for it and
nothing is validated, since the file is not part of this project.

.. image:: https://www.sphinx-doc.org/en/master/_static/sphinx-logo.svg
   :alt: The Sphinx logo, loaded from the Sphinx website
   :width: 80px

A source-root-relative path
---------------------------

A leading ``/`` resolves from the top of the documentation source tree rather
than from this document's own directory, which is what lets a deeply nested
page name a shared asset without counting ``../`` segments.

.. image:: /examples/data/logo.svg
   :alt: The logo, named from the source root
   :width: 60px

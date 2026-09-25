Math
====

Mathematics is written in LaTeX, in two places: the ``:math:`` role for a
formula sitting inside a sentence, and the ``.. math::`` directive for a
display equation on its own line.

Both are converted to `MathML <https://www.w3.org/TR/mathml-core/>`_ while the
site is built. Nothing is rendered in the reader's browser by a script, so a
page carries its own equations and needs no network to display them — and
LaTeX the converter cannot read becomes a build warning rather than a silent
failure on the page.

The ``:math:`` Role
-------------------

Inline math flows with the surrounding text: the identity
:math:`a^2 + b^2 = c^2` is Pythagoras' theorem, and Euler's
:math:`e^{i\pi} + 1 = 0` relates five constants at once. Greek letters
(:math:`\alpha`, :math:`\beta`, :math:`\Omega`), subscripts
(:math:`x_{n+1}`) and fractions (:math:`\tfrac{1}{2}`) all work.

Because the content is LaTeX, an angle bracket is an inequality and not the
explicit-title syntax other roles use — :math:`a < b` means what it says.

The ``.. math::`` Directive
---------------------------

The equation can be written as the directive's argument, for a short one:

.. math:: (a + b)^2 = a^2 + 2ab + b^2

Or as the directive's body, which is the usual form:

.. math::

   \int_{-\infty}^{\infty} e^{-x^2} \, dx = \sqrt{\pi}

Several Equations at Once
-------------------------

A blank line separates two equations inside one directive. They are aligned
with each other rather than rendered as separate blocks:

.. math::

   a = b + c

   d = e + f

Multi-Line Equations
--------------------

Within a single equation, ``\\`` starts a new aligned line and ``&`` marks the
alignment point:

.. math::

   (a + b)^2 &= (a + b)(a + b) \\
             &= a^2 + 2ab + b^2

``:label:`` and the ``:eq:`` Role
---------------------------------

A ``:label:`` gives an equation a number and makes it referenceable. Only
labeled equations are numbered, and the numbering restarts in every document:

.. math::
   :label: euler-identity

   e^{i\pi} + 1 = 0

.. math::
   :label: gauss-sum

   \sum_{k=1}^{n} k = \frac{n(n+1)}{2}

The ``:eq:`` role links to a labeled equation, showing its number as the link
text: the identity above is :eq:`euler-identity`, and the summation is
:eq:`gauss-sum`. A reference resolves across documents too, so another page
can point at :eq:`euler-identity` just as this one does.

``:name:``
----------

``:name:`` is accepted as a spelling of ``:label:``, and behaves identically:

.. math::
   :name: binomial-square

   (a + b)^2 = a^2 + 2ab + b^2

That equation is :eq:`binomial-square`.

``:class:``
-----------

Adds CSS class names to the rendered equation's wrapper, for a theme to style:

.. math::
   :class: highlighted

   E = mc^2

``:nowrap:``
------------

Normally the directive wraps its body in an alignment environment as needed.
``:nowrap:`` turns that off, so the author supplies the environment
themselves. An equation written this way is never numbered by rinx —
the numbering is the author's to write too, and an environment like ``align``
numbers its own rows. That count restarts in every ``:nowrap:`` directive,
because each one is converted on its own:

.. math::
   :nowrap:

   \begin{align}
   f(x) &= x^2 + 2x + 1 \\
        &= (x + 1)^2
   \end{align}

Environments
------------

Any environment the converter implements can be used, either through
``:nowrap:`` or directly in the body.

Cases:

.. math::

   \operatorname{sgn}(x) = \begin{cases}
     1  & x > 0 \\
     0  & x = 0 \\
     -1 & x < 0
   \end{cases}

Matrices:

.. math::

   A = \begin{pmatrix}
     1 & 2 \\
     3 & 4
   \end{pmatrix}

Stacked equations with ``gathered``:

.. math::

   \begin{gathered}
   x + y = 10 \\
   x - y = 2
   \end{gathered}

One deviation from Sphinx is worth knowing: Sphinx wraps a multi-line equation
in ``split``, which the MathML converter does not implement, so ``aligned`` is
used instead. It produces the same alignment.

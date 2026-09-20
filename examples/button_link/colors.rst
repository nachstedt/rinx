.. _button-link-colors:

=======
Colours
=======

``:color:`` takes any of sphinx-design's eleven semantic colours — the same
eleven a ``.. dropdown::`` paints its summary bar with. Each becomes one CSS
class this site's stylesheet defines, so a colour nobody styled would render
as an unpainted button rather than as an error; that is why the value is
checked while parsing.

Filled
======

.. button-link:: https://example.com/primary
   :color: primary

   primary

.. button-link:: https://example.com/secondary
   :color: secondary

   secondary

.. button-link:: https://example.com/success
   :color: success

   success

.. button-link:: https://example.com/info
   :color: info

   info

.. button-link:: https://example.com/warning
   :color: warning

   warning

.. button-link:: https://example.com/danger
   :color: danger

   danger

.. button-link:: https://example.com/light
   :color: light

   light

.. button-link:: https://example.com/muted
   :color: muted

   muted

.. button-link:: https://example.com/dark
   :color: dark

   dark

.. button-link:: https://example.com/white
   :color: white

   white

.. button-link:: https://example.com/black
   :color: black

   black

Outlined
========

``:outline:`` draws the same colour as a border and text on no fill. It needs
a ``:color:`` to outline — written alone it produces no class at all, which is
why this build reports that pair rather than leaving it silently invisible
(see ``diagnostics``).

.. button-link:: https://example.com/primary-outline
   :color: primary
   :outline:

   primary, outlined

.. button-link:: https://example.com/danger-outline
   :color: danger
   :outline:

   danger, outlined

.. button-link:: https://example.com/dark-outline
   :color: dark
   :outline:

   dark, outlined

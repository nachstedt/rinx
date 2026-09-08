.. _dropdown-markers:

Dropdown Markers and Animation
==============================

``:chevron:`` chooses which arrow marks the open/closed state. The value names
the pair of directions the marker moves between, and only the closed half is
drawn — the stylesheet rotates it on open.

.. dropdown:: right-down (the default)
   :chevron: right-down

   The marker points right when closed and down when open. Writing no
   ``:chevron:`` at all gives the same result.

.. dropdown:: down-up
   :chevron: down-up

   The marker points down when closed and up when open.

``:animate:`` reveals the body with a transition instead of an instant jump.

.. dropdown:: fade-in
   :animate: fade-in

   The body fades in.

.. dropdown:: fade-in-slide-down
   :animate: fade-in-slide-down

   The body fades in while sliding down.

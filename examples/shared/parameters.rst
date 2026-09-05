.. This fragment is included by ``includes.rst``. It is deliberately *not*
   listed in the library's ``srcs``: an included file is spliced into its
   includer, and listing it in ``srcs`` would additionally publish it as a
   page of its own.

The text between the markers below is what a selective include picks up.

.. BEGIN PARAMETERS

Every function in this example takes the same two parameters:

``name``
   The thing to greet, as a string.

``times``
   How often to repeat the greeting.

.. END PARAMETERS

This closing paragraph is outside the markers, so a
``:start-after:``/``:end-before:`` include leaves it behind.

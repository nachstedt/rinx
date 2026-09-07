Audit events
============

The same mechanism expresses CPython's ``.. audit-event::``, whose argument is
a comma-separated signature rather than a title, and whose id is derived from
its first field instead of generated.

.. audit-event:: os.system, command, 3.8

   Raised when a shell command is about to run.

.. audit-event:: open, path mode flags, 3.8

   Raised by every call that opens a file.

The reference :audit-event:`os.system` resolves against the derived id.

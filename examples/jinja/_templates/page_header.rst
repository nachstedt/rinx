.. note::

   Page source: ``{{ page }}`` — release {{ release }}.

   This box is written once, in ``_templates/page_header.rst``, and rendered
   into every page that includes it. ``page`` comes from the ``{% raw %}{% set %}{% endraw %}`` at
   the top of the including document; ``release`` comes from the library's
   ``jinja_context``.

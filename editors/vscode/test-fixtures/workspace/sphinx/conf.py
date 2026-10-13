# A Sphinx project inside the e2e workspace (src/test/e2e/sphinx.test.ts): its
# drafts are excluded, its root document is computed (reported, not run), it
# declares autodoc (shown as information), and its prolog defines |release|.
import os

exclude_patterns = ['drafts']
html_theme = 'alabaster'
root_doc = os.environ.get('ROOT_DOC', 'index')
extensions = ['sphinx.ext.autodoc']
rst_prolog = '.. |release| replace:: 1.0\n'

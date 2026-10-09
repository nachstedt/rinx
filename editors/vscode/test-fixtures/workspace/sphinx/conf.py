# A Sphinx project inside the e2e workspace (src/test/e2e/sphinx.test.ts):
# its drafts are excluded, its root document is computed (reported, not run),
# and it declares autodoc, whose directives are shown as information.
import os

exclude_patterns = ['drafts']
html_theme = 'alabaster'
root_doc = os.environ.get('ROOT_DOC', 'index')
extensions = ['sphinx.ext.autodoc']

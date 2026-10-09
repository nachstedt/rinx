# A Sphinx project inside the e2e workspace (src/test/e2e/sphinx.test.ts):
# its drafts are excluded, and its root document is computed, which the
# language server reports rather than runs.
import os

exclude_patterns = ['drafts']
html_theme = 'alabaster'
root_doc = os.environ.get('ROOT_DOC', 'index')

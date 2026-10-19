-- rinx's language server for reStructuredText, in Neovim 0.11 or later.
vim.lsp.config('rinx', {
  cmd = { 'rinx', 'lsp' },
  filetypes = { 'rst' },
  root_markers = { 'conf.py', 'MODULE.bazel', '.git' },
})
vim.lsp.enable('rinx')

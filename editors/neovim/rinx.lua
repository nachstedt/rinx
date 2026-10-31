-- rinx's language server for reStructuredText, in Neovim 0.11 or later.
vim.lsp.config('rinx', {
  cmd = { 'rinx', 'lsp' },
  filetypes = { 'rst' },
  root_markers = { 'conf.py', 'MODULE.bazel', '.git' },
})
vim.lsp.enable('rinx')

-- Colours the names rinx marks — directives, options, roles, references,
-- substitutions — like their nearest kin, unless the colour scheme says otherwise.
for kind, group in pairs({
  directive = '@keyword',
  option = '@property',
  role = '@function',
  reference = '@markup.link',
  substitution = '@variable',
}) do
  vim.api.nvim_set_hl(0, '@lsp.type.' .. kind, { link = group, default = true })
end

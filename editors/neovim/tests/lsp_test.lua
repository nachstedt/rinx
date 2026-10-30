-- Drives `rinx lsp` through Neovim's own client, configured by the snippet
-- docs/editors.rst shows, over a copy of editors/test-fixtures/workspace.
--
-- Run by editors/run-tests.sh: nvim --headless --clean -l lsp_test.lua <snippet> <workspace>

local snippet, workspace = arg[1], arg[2]

-- How long any one answer may take before the test fails rather than hangs.
local TIMEOUT = 20000

local failures = 0

--- Reports `name` as passed when `ok`, else as failed with `detail`.
local function check(name, ok, detail)
  if ok then
    io.write('ok: ' .. name .. '\n')
  else
    failures = failures + 1
    io.write('FAILED: ' .. name .. ': ' .. vim.inspect(detail) .. '\n')
  end
end

dofile(snippet)
vim.cmd.edit(workspace .. '/index.rst')
local buffer = vim.api.nvim_get_current_buf()
local uri = vim.uri_from_bufnr(buffer)

--- The first result the server answers `method` with, or nil.
local function request(method, params)
  local responses = vim.lsp.buf_request_sync(buffer, method, params, TIMEOUT) or {}
  for _, response in pairs(responses) do
    if response.result ~= nil then
      return response.result
    end
  end
end

--- `request`, asked again until it answers: the workspace scan may still run.
local function request_until_answered(method, params)
  local result
  vim.wait(TIMEOUT, function()
    result = request(method, params)
    return result ~= nil and not vim.tbl_isempty(result)
  end, 100)
  return result
end

--- The position of character `character` on line `line` of index.rst.
local function at(line, character)
  return { textDocument = { uri = uri }, position = { line = line, character = character } }
end

-- The snippet starts the server for the buffer.
local started = vim.wait(TIMEOUT, function()
  return #vim.lsp.get_clients({ bufnr = buffer, name = 'rinx' }) > 0
end, 50)
check('the snippet attaches rinx to an .rst buffer', started, vim.bo[buffer].filetype)

-- Diagnostics arrive on the line and column the build reports.
vim.wait(TIMEOUT, function()
  return #vim.diagnostic.get(buffer) > 0
end, 50)
local diagnostics = vim.diagnostic.get(buffer)
local first = diagnostics[1] or {}
check(
  'the unknown directive is underlined',
  #diagnostics == 1 and first.code == 'directive.unknown' and first.lnum == 9 and first.col == 0,
  diagnostics
)

-- Hover on `:ref:` names the section in target.rst, which only the scan knows.
local hover = request_until_answered('textDocument/hover', at(7, 4))
local markdown = hover and hover.contents and hover.contents.value or ''
check('hover shows the title the reference leads to', markdown:find('**Installing**', 1, true), hover)

-- Go to definition opens target.rst at the label, `.. _install:`.
local definition = request_until_answered('textDocument/definition', at(7, 4))
local location = definition and (definition[1] or definition) or {}
local target = location.targetUri or location.uri or ''
check('definition opens the target document', vim.endswith(target, '/target.rst'), definition)
local range = location.targetSelectionRange or location.range or { ['end'] = {} }
check('definition selects the label', range['end'].character == 12, definition)

-- Find references lists the use, and the label itself when asked to.
local params = at(7, 4)
params.context = { includeDeclaration = true }
local references = request_until_answered('textDocument/references', params) or {}
local referenced = vim.tbl_map(function(reference)
  return vim.fs.basename(reference.uri) .. ':' .. reference.range.start.line
end, references)
check('references list the use and the label', vim.deep_equal(referenced, { 'index.rst:7', 'target.rst:0' }), references)

-- Rename rewrites the use and the label, each its name alone; nothing is applied.
local rename_params = at(7, 4)
rename_params.newName = 'setup'
local rename = request_until_answered('textDocument/rename', rename_params) or {}
local renamed = {}
for uri, edits in pairs(rename.changes or {}) do
  for _, edit in ipairs(edits) do
    table.insert(renamed, vim.fs.basename(uri) .. ':' .. edit.range.start.line .. ':' .. edit.range.start.character .. '=' .. edit.newText)
  end
end
for _, change in ipairs(rename.documentChanges or {}) do
  for _, edit in ipairs(change.edits) do
    table.insert(renamed, vim.fs.basename(change.textDocument.uri) .. ':' .. edit.range.start.line .. ':' .. edit.range.start.character .. '=' .. edit.newText)
  end
end
table.sort(renamed)
check('rename rewrites the use and the label', vim.deep_equal(renamed, { 'index.rst:7:10=setup', 'target.rst:0:4=setup' }), rename)

-- Completion inside the role lists the label.
local completion = request_until_answered('textDocument/completion', at(7, 10))
local items = completion and (completion.items or completion) or {}
local labels = vim.tbl_map(function(item)
  return item.label
end, items)
check('completion lists the label', vim.tbl_contains(labels, 'install'), labels)

-- Completion of the unknown directive's name lists the directives rinx takes.
local directives = request_until_answered('textDocument/completion', at(9, 4))
local directive_items = directives and (directives.items or directives) or {}
local directive_labels = vim.tbl_map(function(item)
  return item.label
end, directive_items)
check('completion lists the directives', vim.tbl_contains(directive_labels, 'figure'), directive_labels)

-- Completion right after the reference's opening colon lists the roles.
local roles = request_until_answered('textDocument/completion', at(7, 5))
local role_items = roles and (roles.items or roles) or {}
local role_labels = vim.tbl_map(function(item)
  return item.label
end, role_items)
check('completion lists the roles', vim.tbl_contains(role_labels, 'ref'), role_labels)

-- Completion of the domain role's target lists the function the file defines.
local functions = request_until_answered('textDocument/completion', at(13, 14))
local function_items = functions and (functions.items or functions) or {}
local function_labels = vim.tbl_map(function(item)
  return item.label
end, function_items)
check('completion lists the functions', vim.tbl_contains(function_labels, 'run'), function_labels)

-- The outline lists the title, and under it the function the file defines:
-- nested when the client takes it so, else naming the title as its holder.
local outline = request_until_answered('textDocument/documentSymbol', { textDocument = { uri = uri } })
local symbols = outline or {}
local title = symbols[1] or {}
local under_title = vim.tbl_contains(
  vim.tbl_map(function(symbol)
    return symbol.name
  end, title.children or {}),
  'run'
) or vim.iter(symbols):any(function(symbol)
  return symbol.name == 'run' and symbol.containerName == 'Editor fixture'
end)
check('the outline lists the function under the title', title.name == 'Editor fixture' and under_title, outline)

-- A workspace search finds the label of a document no buffer holds.
local found = request_until_answered('workspace/symbol', { query = 'instal' }) or {}
local found_label = vim.iter(found):any(function(symbol)
  return symbol.name == 'install' and vim.endswith(symbol.location.uri, '/target.rst')
end)
check('a workspace search finds the label in target.rst', found_label, found)

if failures > 0 then
  -- `vim.lsp.log.get_filename` since 0.11; `vim.lsp.get_log_path` before.
  local path = (vim.lsp.log.get_filename or vim.lsp.get_log_path)()
  local log = io.open(path)
  if log then
    io.write('--- ' .. path .. '\n')
    io.write(log:read('*a'), '\n')
    log:close()
  end
  os.exit(1)
end
os.exit(0)

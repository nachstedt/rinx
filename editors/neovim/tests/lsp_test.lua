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

-- Go to definition opens target.rst.
local definition = request_until_answered('textDocument/definition', at(7, 4))
local location = definition and (definition[1] or definition) or {}
local target = location.targetUri or location.uri or ''
check('definition opens the target document', vim.endswith(target, '/target.rst'), definition)

-- Completion inside the role lists the label.
local completion = request_until_answered('textDocument/completion', at(7, 10))
local items = completion and (completion.items or completion) or {}
local labels = vim.tbl_map(function(item)
  return item.label
end, items)
check('completion lists the label', vim.tbl_contains(labels, 'install'), labels)

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

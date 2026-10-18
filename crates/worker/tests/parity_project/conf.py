# A sphinx-needs project for the parity test of what only the whole project
# reveals (`lsp_parity.rs`): the build converts `ubproject.toml` with
# `rinx needs_schema`, the server reads it through this file.
extensions = ["sphinx_needs"]
needs_from_toml = "ubproject.toml"

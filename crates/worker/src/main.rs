//! Rinx Application Entry Point
//!
//! Supports a subcommand-based CLI for Bazel phase integration as well as a
//! legacy single-file mode for quick manual testing.
//!
//! # Subcommands (Bazel phases)
//!
//! ```text
//! rinx parse  --input <file.rst>  --output <file.ast>  [--default-domain <py|c>]
//! rinx validate_toctree --input <file.ast>  [--allowed <path>...]
//! rinx index  [--domain-index <name>]... --inputs <a.ast> [<b.ast> ...]  --output <project.index>
//! rinx render --input <file.ast>  --index <project.index> --doc-path <rel_path> --output <file.html> [--strict-links] [--warnings-output <file.warnings.json>]
//! rinx genindex --index <project.index> --output <genindex.html> --config <config.toml> --template <template.html>
//! rinx modindex --index <project.index> --output <py-modindex.html> --config <config.toml> --template <template.html>
//! rinx inventory --index <project.index> --output <objects.inv> --config <config.toml>
//! rinx extract_doctests --input <file.ast> --output <file.doctests.json>
//! rinx embed_assets --input <file.ast> --output <file.embeds.json>
//! rinx lsp [--stdio | --check <folder>]
//! rinx validate_assets [--image-dir <dir>] [--download-dir <dir>] [--diagram-dirs <puml_dir>...] --inputs <a.ast> [<b.ast> ...]
//! ```
//!
//! # Legacy mode (quick preview)
//!
//! ```text
//! rinx <file.rst>    # prints HTML to stdout
//! ```

mod commands;

use anyhow::{Context, Result, anyhow};
use rinx::process_rst;
use std::env;
use std::fs;

use commands::{
    cmd_diagnostic_codes_rst, cmd_embed_assets, cmd_entity_json_schema, cmd_extract_doctests,
    cmd_genindex, cmd_index, cmd_inventory, cmd_lsp, cmd_modindex, cmd_parse, cmd_preview,
    cmd_render, cmd_validate_assets, cmd_validate_toctree,
};

fn cmd_legacy(path: &str) -> Result<()> {
    let rst = fs::read_to_string(path).with_context(|| format!("Error reading '{path}'"))?;
    let html = process_rst(path, &rst);
    print!("{html}");
    Ok(())
}

fn run(args: &[String]) -> Result<()> {
    match args.get(1).map(String::as_str) {
        Some("parse") => cmd_parse(&args[2..]),
        Some("validate_toctree") => cmd_validate_toctree(&args[2..]),
        Some("extract_doctests") => cmd_extract_doctests(&args[2..]),
        Some("embed_assets") => cmd_embed_assets(&args[2..]),
        Some("validate_assets") => cmd_validate_assets(&args[2..]),
        Some("index") => cmd_index(&args[2..]),
        Some("render") => cmd_render(&args[2..]),
        Some("genindex") => cmd_genindex(&args[2..]),
        Some("inventory") => cmd_inventory(&args[2..]),
        Some("modindex") => cmd_modindex(&args[2..]),
        Some("preview") => cmd_preview(&args[2..]),
        // The language server, for an editor rather than a build phase.
        Some("lsp") => cmd_lsp(&args[2..]),
        // Developer tooling, not a pipeline phase: regenerates the checked-in
        // JSON Schema that editors validate an `entities.toml` against.
        Some("entity_json_schema") => {
            cmd_entity_json_schema();
            Ok(())
        }
        // Developer tooling too: regenerates `docs/diagnostics.rst`, the page
        // the language server links each diagnostic code to.
        Some("diagnostic_codes_rst") => {
            cmd_diagnostic_codes_rst();
            Ok(())
        }
        Some(path) if !path.starts_with('-') => cmd_legacy(path),
        _ => {
            let program = args.first().map_or("rinx", String::as_str);
            let msg = format!(
                "Usage:\n\
                   {program} <file.rst>                                   (legacy preview)\n\
                   {program} parse  --input <file.rst> --output <file.ast> [--default-domain <py|c>] [--diagrams]\n\
                   {program} extract_doctests --input <file.ast> --output <file.doctests.json>\n\
                   {program} embed_assets --input <file.ast> --output <file.embeds.json>\n\
                   {program} validate_toctree --input <file.ast.raw> --output <file.ast> [--allowed <path>...]\n\
                   {program} index  [--domain-index <name>]... --inputs <a.ast> [<b.ast> ...] --output <project.index>\n\
                   {program} render --input <file.ast> --index <project.index> --doc-path <rel_path> --output <file.html> --config <config.toml> --template <template.html> [--embeds <file.embeds.json>] [--diagram-outdir <puml_dir>] [--strict-links] [--warnings-output <file.warnings.json>]\n\
                   {program} genindex --index <project.index> --output <genindex.html> --config <config.toml> --template <template.html>\n\
                   {program} modindex --index <project.index> --output <py-modindex.html> --config <config.toml> --template <template.html>\n\
                   {program} inventory --index <project.index> --output <objects.inv> --config <config.toml>\n\
                   {program} preview --doc-path <rel_path> --config <config.toml> --template <template.html> [--index <project.index>] [--default-domain <py|c>]\n\
                   {program} lsp [--stdio | --check <folder>]\n\
                   {program} validate_assets [--image-dir <dir>] [--download-dir <dir>] [--diagram-dirs <puml_dir>...] --inputs <a.ast> [<b.ast> ...]"
            );
            Err(anyhow!(msg))
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if let Err(err) = run(&args) {
        eprintln!("{err:?}");
        std::process::exit(1);
    }
}

//! The `validate_toctree` subcommand: checks toctree entries only reference
//! declared docs.

use anyhow::{Context, Result};
use rinx_ast as ast;
use rinx_worker::validator;
use std::fs;

use super::cli_args::{flag_value, flag_values_opt};

pub(crate) fn cmd_validate_toctree(args: &[String]) -> Result<()> {
    let input = flag_value(args, "--input")?;
    let output = flag_value(args, "--output")?;
    let allowed: std::collections::HashSet<String> =
        flag_values_opt(args, "--allowed").into_iter().collect();

    let ast_json =
        fs::read_to_string(&input).with_context(|| format!("Error reading '{input}'"))?;
    let doc: ast::Document =
        serde_json::from_str(&ast_json).context("Failed to deserialize AST")?;

    validator::validate_toctree(&doc, &allowed)?;
    fs::write(&output, &ast_json).with_context(|| format!("Error writing '{output}'"))?;
    Ok(())
}

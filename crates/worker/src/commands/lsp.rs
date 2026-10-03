//! The `lsp` subcommand: serves the language server over stdio.
//!
//! The server itself is the `rinx_lsp` crate, whose handlers are already
//! pure; what is left here is the one decision the command line makes —
//! which arguments it accepts.

use anyhow::{Result, bail};

/// Checks the arguments after `lsp`.
///
/// Accepts `--stdio`, which clients such as `vscode-languageclient` append
/// when told the transport, and which names the only one this server has.
/// Anything else is refused rather than ignored, so a flag that a later
/// version of the server would understand is not silently dropped by this one.
fn check_lsp_args(args: &[String]) -> Result<()> {
    match args.iter().find(|arg| arg.as_str() != "--stdio") {
        Some(unknown) => bail!("lsp: unknown argument '{unknown}'; the server speaks only stdio"),
        None => Ok(()),
    }
}

pub(crate) fn cmd_lsp(args: &[String]) -> Result<()> {
    check_lsp_args(args)?;
    rinx_lsp::run_stdio()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn test_check_lsp_args_accepts_no_arguments() {
        // Given / When / Then
        assert!(check_lsp_args(&[]).is_ok());
    }

    #[test]
    fn test_check_lsp_args_accepts_stdio() {
        // Given / When / Then
        assert!(check_lsp_args(&args(&["--stdio"])).is_ok());
    }

    #[test]
    fn test_check_lsp_args_refuses_another_transport() {
        // Given / When
        let error = check_lsp_args(&args(&["--socket=9000"])).unwrap_err();

        // Then
        assert!(error.to_string().contains("--socket=9000"), "{error}");
    }
}

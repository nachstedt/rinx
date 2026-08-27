//! Rusty Sphinx Application Entry Point
//!
//! Supports a subcommand-based CLI for Bazel phase integration as well as a
//! legacy single-file mode for quick manual testing.
//!
//! # Subcommands (Bazel phases)
//!
//! ```text
//! rusty-sphinx parse  --input <file.rst>  --output <file.ast>  [--default-domain <py|c>]
//! rusty-sphinx validate_toctree --input <file.ast>  [--allowed <path>...]
//! rusty-sphinx index  --inputs <a.ast> [<b.ast> ...]  --output <project.index>
//! rusty-sphinx render --input <file.ast>  --index <project.index> --doc-path <rel_path> --output <file.html> [--strict-links] [--warnings-output <file.warnings.json>]
//! rusty-sphinx genindex --index <project.index> --output <genindex.html> --config <config.toml> --template <template.html>
//! rusty-sphinx extract_doctests --input <file.ast> --output <file.doctests.json>
//! rusty-sphinx validate_images --inputs <a.ast> [<b.ast> ...] --image-dir <dir>
//! ```
//!
//! # Legacy mode (quick preview)
//!
//! ```text
//! rusty-sphinx <file.rst>    # prints HTML to stdout
//! ```

use anyhow::{Context, Result, anyhow};
use rusty_sphinx_analyzer as analyzer;
use rusty_sphinx_ast as ast;
use rusty_sphinx_parser as parser;
use rusty_sphinx_renderer::{self as renderer, config};
use rusty_sphinx_worker::{doctest_plan, domain_warnings, process_rst, validator};
use std::env;
use std::fs;
use std::io::{self, Read};

// ── Pure functions for arguments and logic ───────────────────────────────────

fn flag_value(args: &[String], flag: &str) -> Result<String> {
    if let Some(pos) = args.iter().position(|a| a == flag) {
        if pos + 1 < args.len() {
            Ok(args[pos + 1].clone())
        } else {
            Err(anyhow!("Missing value for flag {flag}"))
        }
    } else {
        Err(anyhow!("Missing required flag '{flag}'"))
    }
}

fn flag_value_opt(args: &[String], flag: &str) -> Option<String> {
    args.windows(2)
        .find(|w| w[0] == flag)
        .and_then(|w| w.get(1).cloned())
}

/// Returns all values that follow `flag` until the next flag (starting with `--`).
fn flag_values(args: &[String], flag: &str) -> Result<Vec<String>> {
    let start = args
        .iter()
        .position(|a| a == flag)
        .ok_or_else(|| anyhow!("Missing required flag '{flag}'"))?;

    let values: Vec<String> = args[start + 1..]
        .iter()
        .take_while(|a| !a.starts_with("--"))
        .cloned()
        .collect();

    Ok(values)
}

/// Returns all values that follow `flag` until the next flag, returning empty vector if flag is missing.
fn flag_values_opt(args: &[String], flag: &str) -> Vec<String> {
    args.iter()
        .position(|a| a == flag)
        .map(|start| {
            args[start + 1..]
                .iter()
                .take_while(|a| !a.starts_with("--"))
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

fn process_parse(path: &str, rst_content: &str, default_domain: ast::Domain) -> Result<String> {
    let doc = parser::parse_with_domain(path, rst_content, default_domain);
    serde_json::to_string(&doc).context("Serialization error")
}

/// Parses the optional `--default-domain` flag, defaulting to `py` — this is
/// how `rusty_sphinx_library`'s Bazel attribute reaches the `parse`/`preview`
/// subcommands (see `rules/library.bzl`, whose `default_domain` attribute
/// restricts to the same two values via `values = ["py", "c"]`).
///
/// Deliberately narrower than `ast::Domain::FromStr`, which also accepts
/// `"std"` (needed so `ObjectType`'s `"std:cmdoption:..."` keys round-trip):
/// `default_domain` is the domain a *bare* directive/role resolves to, and
/// several bare-role code paths (e.g. `handle_func_match`'s
/// `.expect("every domain defines a 'func' role")`) assume it is always `py`
/// or `c` — `std`-domain constructs (`.. option::`, `:option:`, ...) are
/// recognized unconditionally instead, never via `default_domain` (see
/// `resolve_domain_object_type`/`try_parse_scope_directive` in
/// `rusty_sphinx_parser`), so accepting `"std"` here would only invite a
/// runtime panic with no corresponding feature.
fn parse_default_domain_flag(args: &[String]) -> Result<ast::Domain> {
    match flag_value_opt(args, "--default-domain") {
        Some(s) => match s.parse::<ast::Domain>() {
            Ok(domain @ (ast::Domain::Py | ast::Domain::C)) => Ok(domain),
            _ => Err(anyhow!(
                "Invalid --default-domain '{s}', expected 'py' or 'c'"
            )),
        },
        None => Ok(ast::Domain::Py),
    }
}

fn process_index(ast_jsons: &[String]) -> Result<String> {
    let docs: Vec<ast::Document> = ast_jsons
        .iter()
        .map(|json| serde_json::from_str(json).context("Failed to deserialize AST"))
        .collect::<Result<_>>()?;

    let index = analyzer::build_project_index(&docs);
    serde_json::to_string(&index).context("Serialization error")
}

fn process_preview(
    rst: &str,
    index_json: Option<&str>,
    config: &config::SiteConfig,
    template_str: &str,
    doc_path: &str,
    default_domain: ast::Domain,
) -> Result<(
    String,
    Vec<renderer::BrokenLink>,
    Vec<renderer::ObjectTypeMismatch>,
)> {
    let doc = parser::parse_with_domain(doc_path, rst, default_domain);
    let mut index = if let Some(json) = index_json {
        serde_json::from_str(json).context("Failed to deserialize global index")?
    } else {
        rusty_sphinx_index::ProjectIndex::default()
    };

    let local_index = analyzer::analyze(&doc);
    let _ = index.merge(local_index);

    let render_output = renderer::render(&doc, &index, doc_path);

    // Extract page title from the first H1 heading, if any.
    let page_title = doc
        .nodes
        .iter()
        .find_map(|n| {
            if let ast::Node::Heading { level: 1, text } = n {
                Some(ast::inline_plain_text(text))
            } else {
                None
            }
        })
        .unwrap_or_else(|| doc_path.to_string());

    let depth = doc_path.matches('/').count();
    let css_path = if depth == 0 {
        "default.css".to_string()
    } else {
        format!("{}default.css", "../".repeat(depth))
    };

    let html = renderer::render_page(
        &render_output.html,
        template_str,
        config,
        &renderer::PageMeta {
            css_path: &css_path,
            page_title: &page_title,
            doc_path,
            nav_tree: &index.nav_tree,
            has_genindex: !index.genindex_entries.is_empty(),
        },
    )?;
    Ok((
        html,
        render_output.broken_links,
        render_output.object_type_mismatches,
    ))
}

fn process_render(
    ast_json: &str,
    index_json: &str,
    config: &config::SiteConfig,
    template_str: &str,
    doc_path: &str,
) -> Result<(
    String,
    Vec<renderer::BrokenLink>,
    Vec<renderer::ObjectTypeMismatch>,
)> {
    let doc: ast::Document =
        serde_json::from_str(ast_json).context("Failed to deserialize AST document")?;
    let index: rusty_sphinx_index::ProjectIndex =
        serde_json::from_str(index_json).context("Failed to deserialize Project Index")?;

    let render_output = renderer::render(&doc, &index, doc_path);

    // Extract page title from the first H1 heading, if any.
    let page_title = doc
        .nodes
        .iter()
        .find_map(|n| {
            if let ast::Node::Heading { level: 1, text } = n {
                Some(ast::inline_plain_text(text))
            } else {
                None
            }
        })
        .unwrap_or_default();

    let css_path = renderer::css_relative_path(doc_path, "default.css");
    let html = renderer::render_page(
        &render_output.html,
        template_str,
        config,
        &renderer::PageMeta {
            css_path: &css_path,
            page_title: &page_title,
            doc_path,
            nav_tree: &index.nav_tree,
            has_genindex: !index.genindex_entries.is_empty(),
        },
    )?;
    Ok((
        html,
        render_output.broken_links,
        render_output.object_type_mismatches,
    ))
}

fn process_genindex(
    index_json: &str,
    config: &config::SiteConfig,
    template_str: &str,
) -> Result<String> {
    let index: rusty_sphinx_index::ProjectIndex =
        serde_json::from_str(index_json).context("Failed to deserialize Project Index")?;
    renderer::render_genindex(&index, config, template_str)
}

/// Formats a single broken-link diagnostic as a human-readable warning line.
///
/// For a broken domain-object reference the role's requested object type (the
/// "missed type", e.g. `py:function`) is included — it's known at the point
/// resolution failed and pinpoints what kind of object couldn't be found. An
/// ambiguous reference additionally lists the qualified names it matched:
/// unlike a plain miss, the fix is to pick one of them, so they are the
/// actionable part of the message.
fn format_broken_link_warning(doc_path: &str, link: &renderer::BrokenLink) -> String {
    let requested = match &link.kind {
        renderer::BrokenLinkKind::DomainObjectReference(object_type) => {
            format!(" (referenced as {})", object_type.domain_qualified_str())
        }
        renderer::BrokenLinkKind::AmbiguousDomainObjectReference {
            object_type,
            candidates,
        } => format!(
            " (referenced as {}, matches {})",
            object_type.domain_qualified_str(),
            candidates.join(", ")
        ),
        _ => String::new(),
    };
    format!(
        "warning: broken {} '{}'{requested} in {doc_path}",
        link.kind.as_str(),
        link.target
    )
}

/// Formats a single object-type-mismatch diagnostic as a human-readable
/// warning line. Both the requested and resolved object types are shown
/// domain-qualified (e.g. `"py:class"`, not just `"class"`) via
/// [`rusty_sphinx_ast::ObjectType::domain_qualified_str`] — the alias
/// fallback is domain-scoped today (`py`'s `class`/`exception`, and `c`'s
/// `macro`/`member` and `function`/`macro`), so the two domains always match
/// in practice, but spelling both out avoids the reader having to assume
/// that rather than see it.
/// Unlike [`format_broken_link_warning`], this never feeds into
/// [`check_broken_links_strict`] — the reference did resolve, so `--strict-links`
/// never fails the build for it; the warning only flags that the reference's
/// role (e.g. `:exc:`) and the definition's actual object type (e.g. `class`)
/// are inconsistent.
fn format_object_type_mismatch_warning(
    doc_path: &str,
    mismatch: &renderer::ObjectTypeMismatch,
) -> String {
    format!(
        "warning: domain object '{}' referenced as '{}' but defined as '{}' in {doc_path}",
        mismatch.name,
        mismatch.requested_type.domain_qualified_str(),
        mismatch.resolved_type.domain_qualified_str(),
    )
}

/// Returns an error listing every broken link when `strict` is true and
/// `broken_links` is non-empty. Diagnostics are always reported to stderr by
/// the caller regardless of `strict` — this only controls whether they also
/// fail the render.
fn check_broken_links_strict(
    strict: bool,
    doc_path: &str,
    broken_links: &[renderer::BrokenLink],
) -> Result<()> {
    if !strict || broken_links.is_empty() {
        return Ok(());
    }
    let messages: Vec<String> = broken_links
        .iter()
        .map(|link| format_broken_link_warning(doc_path, link))
        .collect();
    Err(anyhow!(
        "Broken link validation failed:\n{}",
        messages.join("\n")
    ))
}

fn process_validate_images(ast_jsons: &[String], image_dir: &str) -> Result<()> {
    let mut missing_images = Vec::new();

    for json in ast_jsons {
        let doc: ast::Document = serde_json::from_str(json).context("Failed to deserialize AST")?;
        for content in collect_plantuml_contents(&doc) {
            let hash = content.hash();
            let svg_name = format!("{hash}.svg");
            let svg_path = std::path::Path::new(image_dir).join(&svg_name);
            if !svg_path.exists() {
                missing_images.push(format!(
                    "Image {svg_name} missing for document {}",
                    doc.path
                ));
            }
        }
    }

    if missing_images.is_empty() {
        Ok(())
    } else {
        Err(anyhow!(
            "Image validation failed:\n{}",
            missing_images.join("\n")
        ))
    }
}

/// Collects every `.. plantuml::` directive in `doc`, however deeply nested.
///
/// Shared by [`process_extract_diagrams`] and [`process_validate_images`] so the
/// set of diagrams that gets *compiled* and the set that gets *validated* can
/// never drift apart — when the two disagreed, a nested diagram silently
/// produced a broken `<img>` that validation did not catch.
fn collect_plantuml_contents(doc: &ast::Document) -> Vec<&ast::HashedContent> {
    let mut contents = Vec::new();
    ast::walk_nodes(&doc.nodes, &mut |node| {
        if let ast::Node::Directive(ast::Directive::PlantUml(content)) = node {
            contents.push(content);
        }
    });
    contents
}

/// Projects a document's doctest blocks into the runnable plan the Python
/// runner consumes.
///
/// Deliberately cheap and deliberately *lossy*: the plan drops everything
/// presentational, so a prose edit re-runs this step but leaves its output
/// bytes unchanged — which is what stops Bazel from re-running the tests. See
/// [`rusty_sphinx_worker::doctest_plan`] for the full reasoning.
fn process_extract_doctests(ast_json: &str) -> Result<String> {
    let doc: ast::Document = serde_json::from_str(ast_json).context("Failed to deserialize AST")?;
    let plan = doctest_plan::build_doctest_plan(&doc)
        .map_err(|problems| anyhow!("Doctest extraction failed:\n{problems}"))?;
    serde_json::to_string(&plan).context("Serialization error")
}

fn process_extract_diagrams(ast_json: &str, outdir_path: &str) -> Result<()> {
    let doc: ast::Document = serde_json::from_str(ast_json).context("Failed to deserialize AST")?;

    fs::create_dir_all(outdir_path).with_context(|| format!("Error creating '{outdir_path}'"))?;

    for content in collect_plantuml_contents(&doc) {
        let path = std::path::Path::new(outdir_path).join(format!("{}.puml", content.hash()));
        fs::write(&path, content.body())
            .with_context(|| format!("Error writing {}", path.display()))?;
    }

    Ok(())
}

// ── Subcommand Handlers (with IO) ────────────────────────────────────────────

fn cmd_parse(args: &[String]) -> Result<()> {
    let input = flag_value(args, "--input")?;
    let output = flag_value(args, "--output")?;
    let default_domain = parse_default_domain_flag(args)?;

    let rst = fs::read_to_string(&input).with_context(|| format!("Error reading '{input}'"))?;
    let json = process_parse(&input, &rst, default_domain)?;
    fs::write(&output, json).with_context(|| format!("Error writing '{output}'"))?;
    Ok(())
}

fn cmd_index(args: &[String]) -> Result<()> {
    let output = flag_value(args, "--output")?;
    let inputs = flag_values(args, "--inputs")?;

    let files: Vec<String> = inputs
        .iter()
        .map(|p| fs::read_to_string(p).with_context(|| format!("Error reading '{p}'")))
        .collect::<Result<_>>()?;

    let json = process_index(&files)?;
    fs::write(&output, json).with_context(|| format!("Error writing '{output}'"))?;
    Ok(())
}

fn cmd_render(args: &[String]) -> Result<()> {
    let input = flag_value(args, "--input")?;
    let index_path = flag_value(args, "--index")?;
    let output = flag_value(args, "--output")?;
    let config_path = flag_value(args, "--config")?;
    let template_path = flag_value(args, "--template")?;
    let doc_path = flag_value(args, "--doc-path")?;
    let strict_links = args.iter().any(|a| a == "--strict-links");
    let warnings_output = flag_value_opt(args, "--warnings-output");

    let config_str = fs::read_to_string(&config_path)
        .with_context(|| format!("Error reading config '{config_path}'"))?;
    let site_config: config::SiteConfig = toml::from_str(&config_str)
        .with_context(|| format!("Error parsing config '{config_path}'"))?;

    let template_str = fs::read_to_string(&template_path)
        .with_context(|| format!("Error reading template '{template_path}'"))?;

    let ast_json =
        fs::read_to_string(&input).with_context(|| format!("Error reading '{input}'"))?;
    let index_json =
        fs::read_to_string(&index_path).with_context(|| format!("Error reading '{index_path}'"))?;

    let (html, broken_links, object_type_mismatches) = process_render(
        &ast_json,
        &index_json,
        &site_config,
        &template_str,
        &doc_path,
    )?;

    for link in &broken_links {
        eprintln!("{}", format_broken_link_warning(&doc_path, link));
    }
    for mismatch in &object_type_mismatches {
        eprintln!(
            "{}",
            format_object_type_mismatch_warning(&doc_path, mismatch)
        );
    }

    // Emit the structured domain-object warning sidecar when requested. Written
    // unconditionally (even with zero warnings) and before the strict-links
    // check, so the file always exists as a Bazel-declared output and reflects
    // reality regardless of whether the strict check then fails the render.
    if let Some(warnings_path) = &warnings_output {
        let report = domain_warnings::build_domain_warning_report(
            &doc_path,
            &broken_links,
            &object_type_mismatches,
        );
        let report_json = serde_json::to_string_pretty(&report)
            .context("Failed to serialize domain warning report")?;
        fs::write(warnings_path, report_json)
            .with_context(|| format!("Error writing '{warnings_path}'"))?;
    }

    check_broken_links_strict(strict_links, &doc_path, &broken_links)?;

    fs::write(&output, html).with_context(|| format!("Error writing '{output}'"))?;
    Ok(())
}

fn cmd_genindex(args: &[String]) -> Result<()> {
    let index_path = flag_value(args, "--index")?;
    let output = flag_value(args, "--output")?;
    let config_path = flag_value(args, "--config")?;
    let template_path = flag_value(args, "--template")?;

    let index_json =
        fs::read_to_string(&index_path).with_context(|| format!("Error reading '{index_path}'"))?;
    let config_str = fs::read_to_string(&config_path)
        .with_context(|| format!("Error reading config '{config_path}'"))?;
    let site_config: config::SiteConfig = toml::from_str(&config_str)
        .with_context(|| format!("Error parsing config '{config_path}'"))?;
    let template_str = fs::read_to_string(&template_path)
        .with_context(|| format!("Error reading template '{template_path}'"))?;

    let html = process_genindex(&index_json, &site_config, &template_str)?;
    fs::write(&output, html).with_context(|| format!("Error writing '{output}'"))?;
    Ok(())
}

fn cmd_validate_toctree(args: &[String]) -> Result<()> {
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

fn cmd_extract_diagrams(args: &[String]) -> Result<()> {
    let input = flag_value(args, "--input")?;
    let outdir = flag_value(args, "--outdir")?;

    let ast_json =
        fs::read_to_string(&input).with_context(|| format!("Error reading '{input}'"))?;
    process_extract_diagrams(&ast_json, &outdir)?;
    Ok(())
}

fn cmd_extract_doctests(args: &[String]) -> Result<()> {
    let input = flag_value(args, "--input")?;
    let output = flag_value(args, "--output")?;

    let ast_json =
        fs::read_to_string(&input).with_context(|| format!("Error reading '{input}'"))?;
    let plan_json = process_extract_doctests(&ast_json)?;
    fs::write(&output, plan_json).with_context(|| format!("Error writing '{output}'"))?;
    Ok(())
}

fn cmd_validate_images(args: &[String]) -> Result<()> {
    let inputs = flag_values(args, "--inputs")?;
    let image_dir = flag_value(args, "--image-dir")?;
    let output = flag_value(args, "--output").ok();

    let ast_jsons: Vec<String> = inputs
        .iter()
        .map(|p| fs::read_to_string(p).with_context(|| format!("Error reading '{p}'")))
        .collect::<Result<_>>()?;

    process_validate_images(&ast_jsons, &image_dir)?;

    if let Some(out_path) = output {
        fs::write(&out_path, "OK").with_context(|| format!("Error writing '{out_path}'"))?;
    }

    Ok(())
}

fn cmd_preview(args: &[String]) -> Result<()> {
    let index_path = flag_value_opt(args, "--index");
    let doc_path = flag_value(args, "--doc-path")?;
    let config_path = flag_value(args, "--config")?;
    let template_path = flag_value(args, "--template")?;
    let default_domain = parse_default_domain_flag(args)?;

    // Read RST from stdin
    let mut rst = String::new();
    io::stdin()
        .read_to_string(&mut rst)
        .context("Failed to read from stdin")?;

    let index_json = if let Some(p) = index_path {
        Some(fs::read_to_string(&p).with_context(|| format!("Error reading index '{p}'"))?)
    } else {
        None
    };

    let config_str = fs::read_to_string(&config_path)
        .with_context(|| format!("Error reading config '{config_path}'"))?;
    let site_config: config::SiteConfig = toml::from_str(&config_str)
        .with_context(|| format!("Error parsing config '{config_path}'"))?;

    let template_str = fs::read_to_string(&template_path)
        .with_context(|| format!("Error reading template '{template_path}'"))?;

    let (html, broken_links, object_type_mismatches) = process_preview(
        &rst,
        index_json.as_deref(),
        &site_config,
        &template_str,
        &doc_path,
        default_domain,
    )?;

    // Preview is deliberately lenient (it renders over incomplete/WIP
    // documents), so broken links are always warnings, never a failure.
    for link in &broken_links {
        eprintln!("{}", format_broken_link_warning(&doc_path, link));
    }
    for mismatch in &object_type_mismatches {
        eprintln!(
            "{}",
            format_object_type_mismatch_warning(&doc_path, mismatch)
        );
    }

    println!("{html}");
    Ok(())
}

fn cmd_legacy(path: &str) -> Result<()> {
    let rst = fs::read_to_string(path).with_context(|| format!("Error reading '{path}'"))?;
    let html = process_rst(path, &rst);
    print!("{html}");
    Ok(())
}

// ── Entry point ──────────────────────────────────────────────────────────────

fn run(args: &[String]) -> Result<()> {
    match args.get(1).map(String::as_str) {
        Some("parse") => cmd_parse(&args[2..]),
        Some("validate_toctree") => cmd_validate_toctree(&args[2..]),
        Some("extract_diagrams") => cmd_extract_diagrams(&args[2..]),
        Some("extract_doctests") => cmd_extract_doctests(&args[2..]),
        Some("validate_images") => cmd_validate_images(&args[2..]),
        Some("index") => cmd_index(&args[2..]),
        Some("render") => cmd_render(&args[2..]),
        Some("genindex") => cmd_genindex(&args[2..]),
        Some("preview") => cmd_preview(&args[2..]),
        Some(path) if !path.starts_with('-') => cmd_legacy(path),
        _ => {
            let program = args.first().map_or("rusty-sphinx", String::as_str);
            let msg = format!(
                "Usage:\n\
                   {program} <file.rst>                                   (legacy preview)\n\
                   {program} parse  --input <file.rst> --output <file.ast> [--default-domain <py|c>]\n\
                   {program} extract_diagrams --input <file.ast> --outdir <puml_dir>\n\
                   {program} extract_doctests --input <file.ast> --output <file.doctests.json>\n\
                   {program} validate_toctree --input <file.ast.raw> --output <file.ast> [--allowed <path>...]\n\
                   {program} index  --inputs <a.ast> [<b.ast> ...] --output <project.index>\n\
                   {program} render --input <file.ast> --index <project.index> --doc-path <rel_path> --output <file.html> --config <config.toml> --template <template.html> [--strict-links] [--warnings-output <file.warnings.json>]\n\
                   {program} genindex --index <project.index> --output <genindex.html> --config <config.toml> --template <template.html>\n\
                   {program} preview --doc-path <rel_path> --config <config.toml> --template <template.html> [--index <project.index>] [--default-domain <py|c>]\n\
                   {program} validate_images --inputs <a.ast> [<b.ast> ...] --image-dir <dir>"
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

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flag_value_returns_value_when_flag_exists() {
        // Given
        let args = vec!["--input".to_string(), "a.rst".to_string()];

        // When
        let result = flag_value(&args, "--input");

        // Then
        assert_eq!(result.unwrap(), "a.rst");
    }

    #[test]
    fn test_flag_value_returns_error_when_flag_is_missing() {
        // Given
        let args = vec!["--input".to_string(), "a.rst".to_string()];

        // When
        let result = flag_value(&args, "--output");

        // Then
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Missing required flag '--output'"
        );
    }

    #[test]
    fn test_flag_value_returns_error_when_value_is_missing() {
        // Given
        let args = vec!["--input".to_string()];

        // When
        let result = flag_value(&args, "--input");

        // Then
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Missing value for flag --input"
        );
    }

    #[test]
    fn test_flag_value_opt_returns_some_when_flag_exists() {
        // Given
        let args = vec!["--input".to_string(), "a.rst".to_string()];

        // When
        let result = flag_value_opt(&args, "--input");

        // Then
        assert_eq!(result.unwrap(), "a.rst");
    }

    #[test]
    fn test_flag_value_opt_returns_none_when_flag_is_missing() {
        // Given
        let args = vec!["--input".to_string(), "a.rst".to_string()];

        // When
        let result = flag_value_opt(&args, "--output");

        // Then
        assert!(result.is_none());
    }

    #[test]
    fn test_flag_value_opt_returns_none_when_flag_at_end_without_value() {
        // Given
        let args = vec![
            "--input".to_string(),
            "a.rst".to_string(),
            "--output".to_string(),
        ];

        // When
        let result = flag_value_opt(&args, "--output");

        // Then
        assert!(result.is_none());
    }

    #[test]
    fn test_flag_value_opt_returns_none_when_args_is_empty() {
        // Given
        let args: Vec<String> = vec![];

        // When
        let result = flag_value_opt(&args, "--output");

        // Then
        assert!(result.is_none());
    }

    #[test]
    fn test_flag_value_opt_returns_first_value_when_flag_appears_multiple_times() {
        // Given
        let args = vec![
            "--input".to_string(),
            "a.rst".to_string(),
            "--input".to_string(),
            "b.rst".to_string(),
        ];

        // When
        let result = flag_value_opt(&args, "--input");

        // Then
        assert_eq!(result.unwrap(), "a.rst");
    }

    #[test]
    fn test_flag_values_returns_list_of_values() {
        // Given
        let args = vec![
            "--inputs".to_string(),
            "a.ast".to_string(),
            "b.ast".to_string(),
            "--output".to_string(),
            "c.idx".to_string(),
        ];

        // When
        let result = flag_values(&args, "--inputs");

        // Then
        assert_eq!(result.unwrap(), vec!["a.ast", "b.ast"]);
    }

    #[test]
    fn test_flag_values_returns_empty_list_when_no_values_follow_flag() {
        // Given
        let args = vec![
            "--inputs".to_string(),
            "--output".to_string(),
            "c.idx".to_string(),
        ];

        // When
        let result = flag_values(&args, "--inputs");

        // Then
        let expected: Vec<String> = vec![];
        assert_eq!(result.unwrap(), expected);
    }

    #[test]
    fn test_flag_values_returns_error_when_flag_is_missing() {
        // Given
        let args = vec!["--output".to_string(), "c.idx".to_string()];

        // When
        let result = flag_values(&args, "--inputs");

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn test_flag_values_opt_returns_list_of_values() {
        // Given
        let args = vec![
            "--inputs".to_string(),
            "a.ast".to_string(),
            "b.ast".to_string(),
            "--output".to_string(),
            "c.idx".to_string(),
        ];

        // When
        let result = flag_values_opt(&args, "--inputs");

        // Then
        assert_eq!(result, vec!["a.ast", "b.ast"]);
    }

    #[test]
    fn test_flag_values_opt_returns_empty_list_when_no_values_follow_flag() {
        // Given
        let args = vec![
            "--inputs".to_string(),
            "--output".to_string(),
            "c.idx".to_string(),
        ];

        // When
        let result = flag_values_opt(&args, "--inputs");

        // Then
        let expected: Vec<String> = vec![];
        assert_eq!(result, expected);
    }

    #[test]
    fn test_flag_values_opt_returns_empty_list_when_flag_is_missing() {
        // Given
        let args = vec!["--output".to_string(), "c.idx".to_string()];

        // When
        let result = flag_values_opt(&args, "--inputs");

        // Then
        let expected: Vec<String> = vec![];
        assert_eq!(result, expected);
    }

    #[test]
    fn test_flag_values_opt_returns_empty_list_when_args_is_empty() {
        // Given
        let args: Vec<String> = vec![];

        // When
        let result = flag_values_opt(&args, "--inputs");

        // Then
        let expected: Vec<String> = vec![];
        assert_eq!(result, expected);
    }

    #[test]
    fn test_process_parse_returns_serialized_ast() {
        // Given
        let rst = "Title\n=====";

        // When
        let json = process_parse("team_a/index.rst", rst, ast::Domain::Py).unwrap();

        // Then
        assert!(json.contains("Title"));
        assert!(json.contains(r#""path":"team_a/index.rst""#));
    }

    #[test]
    fn test_process_parse_resolves_bare_directive_via_default_domain() {
        // Given
        let rst = ".. function:: greet(name)\n\n   Greets the given name.";

        // When
        let json = process_parse("api.rst", rst, ast::Domain::C).unwrap();

        // Then
        assert!(json.contains(r#""CFunction""#));
    }

    #[test]
    fn test_parse_default_domain_flag_defaults_to_py_when_absent() {
        // Given
        let args: Vec<String> = vec![];

        // When
        let result = parse_default_domain_flag(&args).unwrap();

        // Then
        assert_eq!(result, ast::Domain::Py);
    }

    #[test]
    fn test_parse_default_domain_flag_parses_explicit_c() {
        // Given
        let args = vec!["--default-domain".to_string(), "c".to_string()];

        // When
        let result = parse_default_domain_flag(&args).unwrap();

        // Then
        assert_eq!(result, ast::Domain::C);
    }

    #[test]
    fn test_parse_default_domain_flag_rejects_invalid_value() {
        // Given
        let args = vec!["--default-domain".to_string(), "rust".to_string()];

        // When
        let result = parse_default_domain_flag(&args);

        // Then
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid --default-domain")
        );
    }

    #[test]
    fn test_parse_default_domain_flag_rejects_std() {
        // Given — `std` is a valid `ast::Domain` (needed for `ObjectType`
        // keys), but not a valid *default* domain: several bare-role code
        // paths assume `default_domain` is always `py` or `c`.
        let args = vec!["--default-domain".to_string(), "std".to_string()];

        // When
        let result = parse_default_domain_flag(&args);

        // Then
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid --default-domain")
        );
    }

    #[test]
    fn test_format_broken_link_warning_includes_kind_target_and_doc_path() {
        // Given
        let link = renderer::BrokenLink {
            kind: renderer::BrokenLinkKind::Reference,
            target: "missing-section".to_string(),
        };

        // When
        let message = format_broken_link_warning("guide/intro.rst", &link);

        // Then
        assert_eq!(
            message,
            "warning: broken ref 'missing-section' in guide/intro.rst"
        );
    }

    #[test]
    fn test_check_broken_links_strict_passes_when_not_strict() {
        // Given
        let broken_links = vec![renderer::BrokenLink {
            kind: renderer::BrokenLinkKind::Reference,
            target: "missing".to_string(),
        }];

        // When
        let result = check_broken_links_strict(false, "doc.rst", &broken_links);

        // Then
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_broken_links_strict_passes_when_no_broken_links() {
        // Given
        let broken_links: Vec<renderer::BrokenLink> = vec![];

        // When
        let result = check_broken_links_strict(true, "doc.rst", &broken_links);

        // Then
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_broken_links_strict_fails_when_strict_and_broken_links_present() {
        // Given
        let broken_links = vec![renderer::BrokenLink {
            kind: renderer::BrokenLinkKind::Reference,
            target: "missing".to_string(),
        }];

        // When
        let result = check_broken_links_strict(true, "doc.rst", &broken_links);

        // Then
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("Broken link validation failed"));
        assert!(msg.contains("missing"));
    }

    #[test]
    fn test_process_index_returns_serialized_project_index() {
        // Given
        let docs = vec![
            r#"{"path":"test.rst","nodes":[{"Heading":{"level":1,"text":[{"Text":"Title"}]}}]}"#
                .to_string(),
        ];

        // When
        let index = process_index(&docs).unwrap();

        // Then
        assert_eq!(
            index,
            r#"{"targets":{},"document_titles":{"test.rst":"Title"},"nav_tree":[{"title":"Title","path":"test.rst","children":[]}],"glossary_terms":{},"domain_objects":{},"genindex_entries":[]}"#
        );
    }

    #[test]
    fn test_process_render_returns_html_string() {
        // Given
        let doc =
            r#"{"path":"test.rst","nodes":[{"Heading":{"level":1,"text":[{"Text":"Title"}]}}]}"#;
        let index = r#"{"targets":{},"document_titles":{},"nav_tree":[]}"#;
        let config = config::SiteConfig::default();
        let template = "{{ body }}";

        // When
        let (html, broken_links, object_type_mismatches) =
            process_render(doc, index, &config, template, "test.rst").unwrap();

        // Then
        assert!(html.contains("<h1>Title</h1>"));
        assert!(broken_links.is_empty());
        assert!(object_type_mismatches.is_empty());
    }

    #[test]
    fn test_process_render_reports_broken_reference() {
        // Given
        let doc = r#"{"path":"test.rst","nodes":[{"Paragraph":[{"Reference":{"display":"missing","target":"missing"}}]}]}"#;
        let index = r#"{"targets":{},"document_titles":{},"nav_tree":[]}"#;
        let config = config::SiteConfig::default();
        let template = "{{ body }}";

        // When
        let (_, broken_links, _) =
            process_render(doc, index, &config, template, "test.rst").unwrap();

        // Then
        assert_eq!(broken_links.len(), 1);
        assert_eq!(broken_links[0].target, "missing");
    }

    #[test]
    fn test_process_render_reports_object_type_mismatch() {
        // Given — mirrors CPython's `xmlrpc.client.rst`: `Fault` is defined
        // via `.. class::` but referenced via `:exc:`.
        let doc = r#"{"path":"test.rst","nodes":[
            {"Directive":{"DomainObject":{"PyClass":{"signatures":["Fault"],"is_final":false,"body":[]}}}},
            {"Paragraph":[{"DomainObjectReference":{"object_type":"py:exception","name":"Fault","display":"Fault","link":true}}]}
        ]}"#;
        let index = r#"{"targets":{},"document_titles":{},"nav_tree":[],"glossary_terms":{},"domain_objects":{"fault":{"py:class":"test.rst"}},"genindex_entries":[]}"#;
        let config = config::SiteConfig::default();
        let template = "{{ body }}";

        // When
        let (_, broken_links, object_type_mismatches) =
            process_render(doc, index, &config, template, "test.rst").unwrap();

        // Then — resolved, not broken, but flagged as a mismatch
        assert!(broken_links.is_empty());
        assert_eq!(object_type_mismatches.len(), 1);
        assert_eq!(object_type_mismatches[0].name, "Fault");
        assert_eq!(
            object_type_mismatches[0].requested_type,
            ast::ObjectType::Py(ast::PyObjectType::Exception)
        );
        assert_eq!(
            object_type_mismatches[0].resolved_type,
            ast::ObjectType::Py(ast::PyObjectType::Class)
        );
    }

    #[test]
    fn test_format_object_type_mismatch_warning_includes_name_and_types() {
        // Given
        let mismatch = renderer::ObjectTypeMismatch {
            name: "fault".to_string(),
            requested_type: ast::ObjectType::Py(ast::PyObjectType::Exception),
            resolved_type: ast::ObjectType::Py(ast::PyObjectType::Class),
        };

        // When
        let message = format_object_type_mismatch_warning("xmlrpc.client.rst", &mismatch);

        // Then
        assert_eq!(
            message,
            "warning: domain object 'fault' referenced as 'py:exception' but defined as 'py:class' in xmlrpc.client.rst"
        );
    }

    #[test]
    fn test_process_genindex_renders_html_with_letter_section() {
        // Given
        let index = r#"{"targets":{},"document_titles":{"guide.rst":"Guide"},"nav_tree":[],"glossary_terms":{},"domain_objects":{},"genindex_entries":[{"primary":"execution","subentry":null,"main":false,"doc_path":"guide.rst","anchor":"index-0"}]}"#;
        let config = config::SiteConfig::default();
        let template = "{{ body }}";

        // When
        let html = process_genindex(index, &config, template).unwrap();

        // Then
        assert!(html.contains("<h2 id=\"E\">E</h2>"));
        assert!(html.contains("execution"));
        assert!(html.contains("guide.html#index-0"));
    }

    #[test]
    fn test_process_genindex_returns_error_for_invalid_index_json() {
        // Given
        let index = "not json";
        let config = config::SiteConfig::default();
        let template = "{{ body }}";

        // When
        let result = process_genindex(index, &config, template);

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn test_process_preview_renders_html_with_merged_index() {
        // Given
        let rst = "Section A\n=========\n\nSee :ref:`section-b`";
        // Global index only knows about section-b in another file
        let global_index = r#"{"targets":{"section-b":{"Internal":"other.rst"}},"document_titles":{"other.rst":"Other"},"nav_tree":[]}"#;
        let config = config::SiteConfig::default();
        let template = "<html>{{ body }}</html>";

        // When
        let (html, broken_links, _) = process_preview(
            rst,
            Some(global_index),
            &config,
            template,
            "test.rst",
            ast::Domain::Py,
        )
        .unwrap();

        // Then
        assert!(html.contains("<h1>Section A</h1>"));
        // Cross-reference to other file should be resolved
        assert!(html.contains("href=\"other.html#section-b\""));
        assert!(broken_links.is_empty());
    }

    #[test]
    fn test_process_preview_works_without_global_index() {
        // Given
        let rst = "Section A\n=========";
        let config = config::SiteConfig::default();
        let template = "<html>{{ body }}</html>";

        // When
        let (html, broken_links, _) =
            process_preview(rst, None, &config, template, "test.rst", ast::Domain::Py).unwrap();

        // Then
        assert!(html.contains("<h1>Section A</h1>"));
        assert!(broken_links.is_empty());
    }

    #[test]
    fn test_process_preview_reports_broken_reference_without_failing() {
        // Given — preview is lenient: an unresolved :ref: is a diagnostic, not an error
        let rst = "Section A\n=========\n\nSee :ref:`missing`";
        let config = config::SiteConfig::default();
        let template = "<html>{{ body }}</html>";

        // When
        let (html, broken_links, _) =
            process_preview(rst, None, &config, template, "test.rst", ast::Domain::Py).unwrap();

        // Then
        assert!(html.contains("class=\"broken-link\""));
        assert_eq!(broken_links.len(), 1);
        assert_eq!(broken_links[0].target, "missing");
    }

    #[test]
    fn test_process_extract_doctests_emits_a_plan() {
        // Given
        let doc = parser::parse(
            "test.rst",
            ".. testcode::\n\n   print(1)\n\n.. testoutput::\n\n   1\n",
        );
        let ast_json = serde_json::to_string(&doc).unwrap();

        // When
        let plan_json = process_extract_doctests(&ast_json).expect("should extract");

        // Then
        let plan: doctest_plan::DocTestPlan = serde_json::from_str(&plan_json).unwrap();
        assert_eq!(plan.doc_path, "test.rst");
        assert_eq!(plan.groups.len(), 1);
    }

    #[test]
    fn test_process_extract_doctests_emits_an_empty_plan_without_doctests() {
        // Given — every document gets a plan, so the Bazel action can be
        // declared unconditionally like the diagram extraction is.
        let doc = parser::parse("test.rst", "Title\n=====\n\nProse.");
        let ast_json = serde_json::to_string(&doc).unwrap();

        // When
        let plan_json = process_extract_doctests(&ast_json).expect("should extract");

        // Then
        let plan: doctest_plan::DocTestPlan = serde_json::from_str(&plan_json).unwrap();
        assert!(plan.groups.is_empty());
    }

    #[test]
    fn test_process_extract_doctests_fails_on_an_orphan_testoutput() {
        // Given
        let doc = parser::parse("test.rst", ".. testoutput::\n\n   42\n");
        let ast_json = serde_json::to_string(&doc).unwrap();

        // When
        let result = process_extract_doctests(&ast_json);

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn test_process_extract_doctests_returns_error_for_invalid_ast_json() {
        // Given
        let ast_json = "{not json";

        // When
        let result = process_extract_doctests(ast_json);

        // Then
        assert!(result.is_err());
    }

    #[test]
    fn test_process_extract_doctests_is_unchanged_by_a_prose_edit() {
        // Given — the same tests, different prose around them. This is the
        // cache firewall as the subcommand actually emits it.
        let before = parser::parse(
            "test.rst",
            "Title\n=====\n\nOriginal prose.\n\n.. testcode::\n\n   print(1)\n",
        );
        let after = parser::parse(
            "test.rst",
            "Title\n=====\n\nRewritten, much longer prose.\n\n.. testcode::\n\n   print(1)\n",
        );

        // When
        let left = process_extract_doctests(&serde_json::to_string(&before).unwrap()).unwrap();
        let right = process_extract_doctests(&serde_json::to_string(&after).unwrap()).unwrap();

        // Then — byte-identical, so Bazel does not re-run the tests.
        assert_eq!(left, right);
    }

    #[test]
    fn test_process_extract_diagrams_creates_files() {
        // Given
        let content = ast::HashedContent::new("A -> B".to_string());
        let expected_hash = content.hash().to_string();
        let doc = ast::Document::new(
            "test.rst".to_string(),
            vec![ast::Node::Directive(ast::Directive::PlantUml(content))],
        );
        let ast_json = serde_json::to_string(&doc).unwrap();
        let outdir = std::env::temp_dir().join(format!(
            "puml_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        // When
        process_extract_diagrams(&ast_json, outdir.to_str().unwrap()).unwrap();

        // Then
        let path = outdir.join(format!("{expected_hash}.puml"));
        assert!(path.exists());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "A -> B");

        let _ = std::fs::remove_dir_all(outdir);
    }

    /// Wraps `inner` in a `.. note::` so a directive sits one level below the
    /// document root — the shape the old top-level-only scan used to miss.
    fn note_containing(inner: ast::Node) -> ast::Node {
        ast::Node::Directive(ast::Directive::Admonition {
            kind: ast::AdmonitionKind::Note,
            title: None,
            collapsible: None,
            body: vec![inner],
        })
    }

    #[test]
    fn test_process_extract_diagrams_extracts_a_diagram_nested_in_an_admonition() {
        // Given — a `.. plantuml::` inside a `.. note::`.
        let content = ast::HashedContent::new("A -> B".to_string());
        let expected_hash = content.hash().to_string();
        let doc = ast::Document::new(
            "test.rst".to_string(),
            vec![note_containing(ast::Node::Directive(
                ast::Directive::PlantUml(content),
            ))],
        );
        let ast_json = serde_json::to_string(&doc).unwrap();
        let outdir = std::env::temp_dir().join(format!(
            "puml_nested_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        // When
        process_extract_diagrams(&ast_json, outdir.to_str().unwrap()).unwrap();

        // Then — the nested diagram is extracted, not silently skipped.
        let path = outdir.join(format!("{expected_hash}.puml"));
        assert!(path.exists());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "A -> B");

        let _ = std::fs::remove_dir_all(outdir);
    }

    #[test]
    fn test_process_validate_images_fails_when_a_nested_diagrams_svg_is_missing() {
        // Given — a nested diagram whose SVG was never produced.
        let content = ast::HashedContent::new("A -> B".to_string());
        let doc = ast::Document::new(
            "test.rst".to_string(),
            vec![note_containing(ast::Node::Directive(
                ast::Directive::PlantUml(content),
            ))],
        );
        let ast_json = serde_json::to_string(&doc).unwrap();
        let empty_image_dir = std::env::temp_dir().join(format!(
            "puml_validate_nested_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&empty_image_dir).unwrap();

        // When
        let result = process_validate_images(&[ast_json], empty_image_dir.to_str().unwrap());

        // Then — validation catches it instead of passing a broken <img> through.
        assert!(result.is_err());

        let _ = std::fs::remove_dir_all(empty_image_dir);
    }

    #[test]
    fn test_collect_plantuml_contents_returns_diagrams_in_document_order() {
        // Given — one top-level diagram and one nested inside an admonition.
        let doc = ast::Document::new(
            "test.rst".to_string(),
            vec![
                ast::Node::Directive(ast::Directive::PlantUml(ast::HashedContent::new(
                    "first".to_string(),
                ))),
                note_containing(ast::Node::Directive(ast::Directive::PlantUml(
                    ast::HashedContent::new("second".to_string()),
                ))),
            ],
        );

        // When
        let contents = collect_plantuml_contents(&doc);

        // Then
        let bodies: Vec<&str> = contents.iter().map(|c| c.body()).collect();
        assert_eq!(bodies, vec!["first", "second"]);
    }

    #[test]
    fn test_collect_plantuml_contents_returns_empty_for_a_document_without_diagrams() {
        // Given
        let doc = ast::Document::new("test.rst".to_string(), vec![ast::Node::Transition]);

        // When
        let contents = collect_plantuml_contents(&doc);

        // Then
        assert!(contents.is_empty());
    }

    #[test]
    fn test_process_extract_diagrams_leaves_empty_dir_when_no_diagrams() {
        // Given
        let ast_json = r#"{"path":"test.rst","nodes":[]}"#;
        let outdir = std::env::temp_dir().join(format!(
            "puml_test_dummy_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        // When
        process_extract_diagrams(ast_json, outdir.to_str().unwrap()).unwrap();

        // Then
        assert!(outdir.exists());
        let path = outdir.join(".dummy.puml");
        assert!(!path.exists());

        let _ = std::fs::remove_dir_all(outdir);
    }

    #[test]
    fn test_process_extract_diagrams_returns_error_for_invalid_ast_json() {
        // Given
        let bad_json = "not valid json";
        let outdir = std::env::temp_dir().join(format!(
            "puml_test_bad_json_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        // When
        let result = process_extract_diagrams(bad_json, outdir.to_str().unwrap());

        // Then
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("Failed to deserialize AST"));
    }

    // ── process_validate_images ───────────────────────────────────────────────

    fn make_plantuml_ast(doc_path: &str, body: &str) -> String {
        let content = ast::HashedContent::new(body.to_string());
        let doc = ast::Document::new(
            doc_path.to_string(),
            vec![ast::Node::Directive(ast::Directive::PlantUml(content))],
        );
        serde_json::to_string(&doc).unwrap()
    }

    fn make_empty_ast(doc_path: &str) -> String {
        let doc = ast::Document::new(doc_path.to_string(), vec![]);
        serde_json::to_string(&doc).unwrap()
    }

    fn temp_image_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "validate_images_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_process_validate_images_succeeds_when_no_diagrams_present() {
        // Given — a document with no PlantUML directives
        let ast_json = make_empty_ast("test.rst");
        let image_dir = temp_image_dir();

        // When
        let result = process_validate_images(&[ast_json], image_dir.to_str().unwrap());

        // Then — no diagrams means nothing to validate, should pass
        assert!(result.is_ok());

        let _ = std::fs::remove_dir_all(image_dir);
    }

    #[test]
    fn test_process_validate_images_succeeds_when_all_svgs_present() {
        // Given — a document with a PlantUML diagram whose SVG exists in image_dir
        let body = "A -> B";
        let content = ast::HashedContent::new(body.to_string());
        let expected_hash = content.hash().to_string();
        let ast_json = make_plantuml_ast("team_a/index.rst", body);

        let image_dir = temp_image_dir();
        std::fs::write(image_dir.join(format!("{expected_hash}.svg")), "<svg/>").unwrap();

        // When
        let result = process_validate_images(&[ast_json], image_dir.to_str().unwrap());

        // Then
        assert!(result.is_ok());

        let _ = std::fs::remove_dir_all(image_dir);
    }

    #[test]
    fn test_process_validate_images_fails_when_svg_is_missing() {
        // Given — a PlantUML diagram but the image_dir is empty
        let body = "A -> B";
        let content = ast::HashedContent::new(body.to_string());
        let expected_hash = content.hash().to_string();
        let ast_json = make_plantuml_ast("team_a/index.rst", body);

        let image_dir = temp_image_dir();
        // Do NOT write the .svg file

        // When
        let result = process_validate_images(&[ast_json], image_dir.to_str().unwrap());

        // Then
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains(&expected_hash),
            "error should mention the missing hash"
        );
        assert!(
            msg.contains("team_a/index.rst"),
            "error should mention the source document"
        );

        let _ = std::fs::remove_dir_all(image_dir);
    }

    #[test]
    fn test_process_validate_images_reports_all_missing_svgs() {
        // Given — two documents each with a distinct diagram, neither SVG present
        let body_a = "A -> B";
        let body_b = "C -> D";
        let hash_a = ast::HashedContent::new(body_a.to_string())
            .hash()
            .to_string();
        let hash_b = ast::HashedContent::new(body_b.to_string())
            .hash()
            .to_string();
        let ast_a = make_plantuml_ast("team_a/index.rst", body_a);
        let ast_b = make_plantuml_ast("team_b/index.rst", body_b);

        let image_dir = temp_image_dir();

        // When
        let result = process_validate_images(&[ast_a, ast_b], image_dir.to_str().unwrap());

        // Then — both missing images are reported in a single error
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains(&hash_a), "hash_a should be mentioned");
        assert!(msg.contains(&hash_b), "hash_b should be mentioned");
        assert!(
            msg.contains("team_a/index.rst"),
            "doc a should be mentioned"
        );
        assert!(
            msg.contains("team_b/index.rst"),
            "doc b should be mentioned"
        );

        let _ = std::fs::remove_dir_all(image_dir);
    }

    #[test]
    fn test_process_validate_images_succeeds_with_empty_input_list() {
        // Given — no AST files at all
        let image_dir = temp_image_dir();

        // When
        let result = process_validate_images(&[], image_dir.to_str().unwrap());

        // Then — nothing to validate
        assert!(result.is_ok());

        let _ = std::fs::remove_dir_all(image_dir);
    }

    #[test]
    fn test_process_validate_images_returns_error_for_invalid_ast_json() {
        // Given — invalid JSON
        let bad_json = "not valid json".to_string();

        let image_dir = temp_image_dir();

        // When
        let result = process_validate_images(&[bad_json], image_dir.to_str().unwrap());

        // Then — deserialisation failure is surfaced as an error
        assert!(result.is_err());

        let _ = std::fs::remove_dir_all(image_dir);
    }

    #[test]
    fn test_process_validate_images_ignores_non_plantuml_nodes() {
        // Given — a document with only headings and paragraphs, no diagrams
        let doc = ast::Document::new(
            "test.rst".to_string(),
            vec![
                ast::Node::Heading {
                    level: 1,
                    text: vec![ast::InlineNode::Text("Title".to_string())],
                },
                ast::Node::Paragraph(vec![ast::InlineNode::Text("text".to_string())]),
            ],
        );
        let ast_json = serde_json::to_string(&doc).unwrap();
        let image_dir = temp_image_dir();

        // When
        let result = process_validate_images(&[ast_json], image_dir.to_str().unwrap());

        // Then — non-diagram nodes are ignored, validation passes
        assert!(result.is_ok());

        let _ = std::fs::remove_dir_all(image_dir);
    }
}

//! The Bazel module a migrated project builds in.
//!
//! A project inside a Bazel workspace already gets rinx's rules there, once
//! its `MODULE.bazel` depends on rinx: the dependency is appended when it is
//! missing, and nothing else of the file is touched. A project in no Bazel
//! workspace becomes one of its own, rooted at `conf.py`'s directory — so the
//! build names each document, and resolves each `/`-rooted path, from where
//! Sphinx does.

/// The file declaring a Bazel module.
pub(crate) const MODULE_FILE: &str = "MODULE.bazel";

/// The module name a project whose own name makes none gets.
const FALLBACK_MODULE_NAME: &str = "docs";

/// rinx's version, which its Bazel module shares with its crates.
const RINX_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The `bazel_dep` on rinx.
fn rinx_dependency() -> String {
    format!("bazel_dep(name = \"rinx\", version = \"{RINX_VERSION}\")\n")
}

/// Whether `module`, a `MODULE.bazel`'s text, already provides rinx's
/// rules: it depends on rinx, or is rinx itself.
#[must_use]
pub(crate) fn depends_on_rinx(module: &str) -> bool {
    let code: String = module
        .lines()
        .map(|line| line.split('#').next().unwrap_or_default())
        .collect::<String>()
        .split_whitespace()
        .collect();
    code.contains("bazel_dep(name=\"rinx\"") || code.contains("module(name=\"rinx\"")
}

/// What is appended to a `MODULE.bazel` that does not depend on rinx yet.
#[must_use]
pub(crate) fn appended_dependency(module: &str) -> String {
    let separator = if module.is_empty() || module.ends_with("\n\n") {
        ""
    } else if module.ends_with('\n') {
        "\n"
    } else {
        "\n\n"
    };
    format!(
        "{separator}# Added by `rinx init --from-sphinx`.\n{}",
        rinx_dependency()
    )
}

/// A new `MODULE.bazel` for the project named `project`.
#[must_use]
pub(crate) fn new_module(project: Option<&str>) -> String {
    format!(
        "\"\"\"Written by `rinx init --from-sphinx`: this documentation's own Bazel module.\"\"\"\n\n\
         module(name = \"{}\")\n\n{}",
        module_name(project),
        rinx_dependency()
    )
}

/// A Bazel module name made of `project`: lowercase, starting with a
/// letter, with every run of other characters an underscore.
fn module_name(project: Option<&str>) -> String {
    let mut name = String::new();
    for character in project.unwrap_or_default().chars() {
        if character.is_ascii_alphanumeric() {
            name.push(character.to_ascii_lowercase());
        } else if !name.is_empty() && !name.ends_with('_') {
            name.push('_');
        }
    }
    let name = name.trim_end_matches('_');
    if name.starts_with(|first: char| first.is_ascii_lowercase()) {
        format!("{name}_docs")
    } else {
        FALLBACK_MODULE_NAME.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_depends_on_rinx_reads_the_dependency_however_it_is_spaced() {
        // When / Then
        assert!(depends_on_rinx(
            "bazel_dep(name = \"rinx\", version = \"0.1.0\")\n"
        ));
        assert!(depends_on_rinx("bazel_dep(\n    name=\"rinx\",\n)\n"));
        assert!(!depends_on_rinx("bazel_dep(name = \"rinx_extra\")\n"));
        assert!(!depends_on_rinx("# bazel_dep(name = \"rinx\")\n"));
        assert!(depends_on_rinx("module(\n    name = \"rinx\",\n)\n"));
        assert!(!depends_on_rinx("module(name = \"rinx_smoke\")\n"));
    }

    #[test]
    fn test_appended_dependency_starts_a_paragraph_of_its_own() {
        // When / Then
        assert!(appended_dependency("module(name = \"a\")").starts_with("\n\n# Added"));
        assert!(appended_dependency("module(name = \"a\")\n").starts_with("\n# Added"));
        assert!(appended_dependency("module(name = \"a\")\n\n").starts_with("# Added"));
        assert!(appended_dependency("").starts_with("# Added"));
        assert!(depends_on_rinx(&appended_dependency("")));
    }

    #[test]
    fn test_new_module_names_the_module_after_the_project_and_depends_on_rinx() {
        // When
        let text = new_module(Some("Sphinx-Needs Demo"));

        // Then
        assert!(
            text.contains("module(name = \"sphinx_needs_demo_docs\")\n"),
            "{text}"
        );
        assert!(depends_on_rinx(&text));
    }

    #[test]
    fn test_module_name_falls_back_when_the_project_names_no_module() {
        // When / Then
        assert_eq!(module_name(None), "docs");
        assert_eq!(module_name(Some("3.14 docs")), "docs");
        assert_eq!(module_name(Some("  ")), "docs");
        assert_eq!(module_name(Some("Python")), "python_docs");
    }
}

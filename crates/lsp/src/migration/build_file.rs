//! The `BUILD.bazel` a migration writes beside `conf.py`: one library holding
//! every document, and the site rendering it.
//!
//! Every list is sorted and every attribute left at its rule's default is
//! left out, so the file is the same whenever the project is, and reads as
//! one written by hand.

use std::fmt::Write as _;

use super::srcs::SRCS_PATTERN;

/// The library's target name.
pub(crate) const LIBRARY_NAME: &str = "docs";
/// The site's target name.
pub(crate) const SITE_NAME: &str = "site";

/// What the written `BUILD.bazel` declares, each path relative to the
/// package and `/`-separated.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct BuildFile {
    /// What `srcs`' glob excludes.
    pub srcs_excludes: Vec<String>,
    pub parse_data: Vec<String>,
    pub images: Vec<String>,
    pub downloads: Vec<String>,
    /// `default_domain`, when it is not the rule's `py`.
    pub default_domain: Option<String>,
    /// `default_role`, when one is set.
    pub default_role: Option<String>,
    pub rst_prolog: Option<String>,
    pub rst_epilog: Option<String>,
    pub diagrams: bool,
    pub jinja: bool,
    /// The entity schema both rules name.
    pub entity_schema: Option<String>,
    pub parse_config: Option<String>,
    /// The site's `rinx.toml`.
    pub config: String,
    /// Whether the site writes the Python module index.
    pub py_modindex: bool,
}

impl BuildFile {
    /// The file's text.
    #[must_use]
    pub(crate) fn render(&self) -> String {
        let mut library = Call::new("rinx_library", LIBRARY_NAME);
        library.raw("srcs", &srcs_glob(&self.srcs_excludes));
        library.list("parse_data", &self.parse_data);
        library.list("images", &self.images);
        library.list("downloads", &self.downloads);
        library.string("default_domain", self.default_domain.as_deref());
        library.string("default_role", self.default_role.as_deref());
        library.string("rst_prolog", self.rst_prolog.as_deref());
        library.string("rst_epilog", self.rst_epilog.as_deref());
        library.flag("diagrams", self.diagrams);
        library.flag("jinja", self.jinja);
        library.string("entity_schema", self.entity_schema.as_deref());
        library.string("parse_config", self.parse_config.as_deref());

        let mut site = Call::new("rinx_site", SITE_NAME);
        site.string("config", Some(&self.config));
        site.string("entity_schema", self.entity_schema.as_deref());
        if self.py_modindex {
            site.list("domain_indices", &["py-modindex".to_string()]);
        }
        site.list("deps", &[format!(":{LIBRARY_NAME}")]);

        format!(
            "\"\"\"Written by `rinx init --from-sphinx` from this directory's conf.py.\"\"\"\n\n\
             load(\"@rinx//:defs.bzl\", \"rinx_library\", \"rinx_site\")\n\n{}\n{}",
            library.finish(),
            site.finish(),
        )
    }
}

/// `srcs`' value: every `.rst` file of the package but `excludes`.
fn srcs_glob(excludes: &[String]) -> String {
    let pattern = starlark_string(SRCS_PATTERN);
    if excludes.is_empty() {
        return format!("glob([{pattern}])");
    }
    format!(
        "glob(\n        [{pattern}],\n        exclude = {},\n    )",
        list_value(excludes, 2)
    )
}

/// One rule call, its attributes in the order they are added.
struct Call {
    text: String,
}

impl Call {
    fn new(rule: &str, name: &str) -> Self {
        Self {
            text: format!("{rule}(\n    name = {},\n", starlark_string(name)),
        }
    }

    fn raw(&mut self, attribute: &str, value: &str) {
        let _ = writeln!(self.text, "    {attribute} = {value},");
    }

    fn list(&mut self, attribute: &str, items: &[String]) {
        if !items.is_empty() {
            self.raw(attribute, &list_value(items, 1));
        }
    }

    fn string(&mut self, attribute: &str, value: Option<&str>) {
        if let Some(value) = value {
            self.raw(attribute, &starlark_string(value));
        }
    }

    fn flag(&mut self, attribute: &str, set: bool) {
        if set {
            self.raw(attribute, "True");
        }
    }

    fn finish(mut self) -> String {
        self.text.push_str(")\n");
        self.text
    }
}

/// A list literal: one item inline, more one per line, indented `depth`
/// levels in.
fn list_value(items: &[String], depth: usize) -> String {
    if let [item] = items {
        return format!("[{}]", starlark_string(item));
    }
    let indent = "    ".repeat(depth + 1);
    let closing = "    ".repeat(depth);
    let mut lines = String::new();
    for item in items {
        let _ = writeln!(lines, "{indent}{},", starlark_string(item));
    }
    format!("[\n{lines}{closing}]")
}

/// `text` as a Starlark string literal.
fn starlark_string(text: &str) -> String {
    let mut literal = String::with_capacity(text.len() + 2);
    literal.push('"');
    for character in text.chars() {
        match character {
            '"' => literal.push_str("\\\""),
            '\\' => literal.push_str("\\\\"),
            '\n' => literal.push_str("\\n"),
            other => literal.push(other),
        }
    }
    literal.push('"');
    literal
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| (*item).to_string()).collect()
    }

    #[test]
    fn test_render_writes_a_plain_project_with_the_rules_defaults_left_out() {
        // Given
        let build = BuildFile {
            config: "rinx.toml".to_string(),
            ..BuildFile::default()
        };

        // When
        let text = build.render();

        // Then
        assert_eq!(
            text,
            "\"\"\"Written by `rinx init --from-sphinx` from this directory's conf.py.\"\"\"\n\n\
             load(\"@rinx//:defs.bzl\", \"rinx_library\", \"rinx_site\")\n\n\
             rinx_library(\n    name = \"docs\",\n    srcs = glob([\"**/*.rst\"]),\n)\n\n\
             rinx_site(\n    name = \"site\",\n    config = \"rinx.toml\",\n    deps = [\":docs\"],\n)\n"
        );
    }

    #[test]
    fn test_render_writes_every_attribute_a_project_needs() {
        // Given
        let build = BuildFile {
            srcs_excludes: strings(&["README.rst", "includes/**"]),
            parse_data: strings(&["includes/a.rst"]),
            images: strings(&["_static/logo.png"]),
            downloads: strings(&["data/a.csv"]),
            default_domain: Some("c".to_string()),
            default_role: Some("any".to_string()),
            rst_prolog: Some("rst_prolog.txt".to_string()),
            rst_epilog: Some("rst_epilog.txt".to_string()),
            diagrams: true,
            jinja: true,
            entity_schema: Some("entities.toml".to_string()),
            parse_config: Some("rinx_parser.toml".to_string()),
            config: "rinx.toml".to_string(),
            py_modindex: true,
        };

        // When
        let text = build.render();

        // Then
        for line in [
            "    srcs = glob(\n        [\"**/*.rst\"],\n        exclude = [\n            \"README.rst\",\n            \"includes/**\",\n        ],\n    ),\n",
            "    parse_data = [\"includes/a.rst\"],\n",
            "    images = [\"_static/logo.png\"],\n",
            "    downloads = [\"data/a.csv\"],\n",
            "    default_domain = \"c\",\n",
            "    default_role = \"any\",\n",
            "    rst_prolog = \"rst_prolog.txt\",\n",
            "    rst_epilog = \"rst_epilog.txt\",\n",
            "    diagrams = True,\n",
            "    jinja = True,\n",
            "    parse_config = \"rinx_parser.toml\",\n",
            "    domain_indices = [\"py-modindex\"],\n",
        ] {
            assert!(text.contains(line), "{line:?} missing from:\n{text}");
        }
        assert_eq!(text.matches("entity_schema = \"entities.toml\"").count(), 2);
    }

    #[test]
    fn test_list_value_writes_one_item_inline_and_more_one_per_line() {
        // When / Then
        assert_eq!(list_value(&strings(&["a"]), 1), "[\"a\"]");
        assert_eq!(
            list_value(&strings(&["a", "b"]), 1),
            "[\n        \"a\",\n        \"b\",\n    ]"
        );
    }

    #[test]
    fn test_starlark_string_escapes_what_ends_or_breaks_a_literal() {
        // When / Then
        assert_eq!(starlark_string("a\"b\\c\nd"), "\"a\\\"b\\\\c\\nd\"");
    }
}

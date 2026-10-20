//! Which `rinx_library` of a package lists a file.
//!
//! A site collects its libraries through `deps`, so a library nobody in the
//! site depends on yet — the usual reason a toctree entry is outside `deps` —
//! is in no manifest. Bazel would answer with a query; the server runs no
//! Bazel, so it reads the build file of the file's package instead: the
//! nearest directory holding one, as Bazel's own packages are. A `srcs` is
//! read when it is a list of strings, a `glob(…)` of them with an `exclude`,
//! or a `+` of those; any other `srcs`, and a library a macro declares, lists
//! nothing as far as this module can tell.

use std::path::Path;

use rinx_pyconf::{Token, tokenize};

use super::calls::{StringList, is_close, is_open, read_call, top_level_calls};

/// The label of the `rinx_library` listing the document `doc_path` — a path
/// from `source_root` — in the build file of its package, read with `read`;
/// the first one, should two list it.
#[must_use]
pub fn library_listing(
    source_root: &Path,
    doc_path: &str,
    read: &dyn Fn(&Path) -> Option<String>,
) -> Option<String> {
    let components: Vec<&str> = doc_path.split('/').collect();
    (0..components.len()).rev().find_map(|depth| {
        let package = components[..depth].join("/");
        let directory = components[..depth]
            .iter()
            .fold(source_root.to_path_buf(), |path, component| {
                path.join(component)
            });
        let text = ["BUILD.bazel", "BUILD"]
            .into_iter()
            .find_map(|name| read(&directory.join(name)))?;
        let file = components[depth..].join("/");
        // The nearest build file is the package, whether or not it lists the
        // file: an outer one never does, by Bazel's rules.
        Some(library_in(&text, &file).map(|name| format!("//{package}:{name}")))
    })?
}

/// The name of the first `rinx_library` in the build file `text` whose
/// `srcs` holds `file`, a path from the package.
fn library_in(text: &str, file: &str) -> Option<String> {
    let tokenized = tokenize(text);
    top_level_calls(&tokenized.tokens)
        .into_iter()
        .filter(|call| call.function.kind.is_name("rinx_library"))
        .find(|call| {
            call.argument("srcs")
                .and_then(|srcs| srcs_hold(srcs.value, file))
                .unwrap_or(false)
        })
        .and_then(|call| call.name().map(str::to_string))
}

/// Whether the `srcs` written as `value` holds `file` — `None` when `value`
/// is no expression this module evaluates.
fn srcs_hold(value: &[Token], file: &str) -> Option<bool> {
    let mut held = false;
    for term in terms(value) {
        held |= if let Some(list) = StringList::read(term) {
            list.items
                .iter()
                .any(|(item, ..)| item.strip_prefix(':').unwrap_or(item) == file)
        } else {
            glob_holds(term, file)?
        };
    }
    Some(held)
}

/// `value`'s terms: the operands of the `+`s outside brackets.
fn terms(value: &[Token]) -> Vec<&[Token]> {
    let mut terms = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    for (index, token) in value.iter().enumerate() {
        if is_open(&token.kind) {
            depth += 1;
        } else if is_close(&token.kind) {
            depth = depth.saturating_sub(1);
        } else if depth == 0 && token.kind.is_op("+") {
            terms.push(&value[start..index]);
            start = index + 1;
        }
    }
    terms.push(&value[start..]);
    terms
}

/// Whether the `glob(…)` written as `term` matches `file` — `None` when
/// `term` is no such call, or its patterns are not written as lists.
fn glob_holds(term: &[Token], file: &str) -> Option<bool> {
    let [function, open, ..] = term else {
        return None;
    };
    if !function.kind.is_name("glob") || !open.kind.is_op("(") {
        return None;
    }
    let (call, end) = read_call(term, 1)?;
    if end != term.len() - 1 {
        return None;
    }
    let include = call
        .arguments
        .iter()
        .find(|argument| argument.keyword.is_none())
        .or_else(|| call.argument("include"))?;
    let patterns = |value| -> Option<Vec<String>> {
        Some(
            StringList::read(value)?
                .items
                .into_iter()
                .map(|(pattern, ..)| pattern)
                .collect(),
        )
    };
    let included = patterns(include.value)?;
    let excluded = match call.argument("exclude") {
        Some(exclude) => patterns(exclude.value)?,
        None => Vec::new(),
    };
    Some(
        included.iter().any(|pattern| glob_matches(pattern, file))
            && !excluded.iter().any(|pattern| glob_matches(pattern, file)),
    )
}

/// Whether Bazel's glob `pattern` matches `path`: `**` any number of whole
/// components, `*` any characters within one, `?` one character.
fn glob_matches(pattern: &str, path: &str) -> bool {
    let pattern: Vec<&str> = pattern.split('/').collect();
    let path: Vec<&str> = path.split('/').collect();
    components_match(&pattern, &path)
}

fn components_match(pattern: &[&str], path: &[&str]) -> bool {
    match pattern.split_first() {
        None => path.is_empty(),
        Some((&"**", rest)) => (0..=path.len()).any(|skip| components_match(rest, &path[skip..])),
        Some((component, rest)) => path.split_first().is_some_and(|(first, others)| {
            component_matches(component, first) && components_match(rest, others)
        }),
    }
}

/// Whether one component of a pattern matches one of a path.
fn component_matches(pattern: &str, name: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let name: Vec<char> = name.chars().collect();
    // `matched[j]`: whether the pattern so far matches the first `j`
    // characters of the name.
    let mut matched = vec![false; name.len() + 1];
    matched[0] = true;
    for wildcard in &pattern {
        let mut next = vec![false; name.len() + 1];
        for j in 0..=name.len() {
            next[j] = match wildcard {
                '*' => matched[j] || (j > 0 && next[j - 1]),
                '?' => j > 0 && matched[j - 1],
                literal => j > 0 && matched[j - 1] && name[j - 1] == *literal,
            };
        }
        matched = next;
    }
    matched[name.len()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    /// A reader of the files `files`, by their path under `/work`.
    fn files(files: &[(&str, &str)]) -> impl Fn(&Path) -> Option<String> + use<> {
        let files: BTreeMap<PathBuf, String> = files
            .iter()
            .map(|(path, text)| (Path::new("/work").join(path), (*text).to_string()))
            .collect();
        move |path| files.get(path).cloned()
    }

    fn listing(doc_path: &str, build_files: &[(&str, &str)]) -> Option<String> {
        library_listing(Path::new("/work"), doc_path, &files(build_files))
    }

    #[test]
    fn test_a_library_listing_a_file_by_name_is_found() {
        // Given / When
        let found = listing(
            "docs/team/index.rst",
            &[(
                "docs/team/BUILD.bazel",
                "rinx_library(\n    name = \"docs\",\n    srcs = [\"index.rst\"],\n)\n",
            )],
        );

        // Then
        assert_eq!(found.as_deref(), Some("//docs/team:docs"));
    }

    #[test]
    fn test_the_nearest_build_file_is_the_package() {
        // Given — the outer package's glob would match, but a file belongs to
        // the nearest package
        let build_files = [
            (
                "docs/BUILD",
                "rinx_library(name = \"outer\", srcs = glob([\"**/*.rst\"]))\n",
            ),
            (
                "docs/team/api/BUILD.bazel",
                "rinx_library(name = \"api\", srcs = glob([\"**/*.rst\"]))\n",
            ),
        ];

        // When
        let deep = listing("docs/team/api/ref/index.rst", &build_files);
        let shallow = listing("docs/team/index.rst", &build_files);

        // Then
        assert_eq!(deep.as_deref(), Some("//docs/team/api:api"));
        assert_eq!(shallow.as_deref(), Some("//docs:outer"));
    }

    #[test]
    fn test_a_glob_with_an_exclude_and_a_list_beside_it_is_evaluated() {
        // Given
        let build = "rinx_library(\n    name = \"docs\",\n    srcs = glob(\n        [\"*.rst\"],\n        exclude = [\"draft_*.rst\"],\n    ) + [\"extra/notes.rst\"],\n)\n";

        // When / Then
        for (file, expected) in [
            ("index.rst", Some("//:docs")),
            ("draft_one.rst", None),
            ("extra/notes.rst", Some("//:docs")),
            ("extra/other.rst", None),
        ] {
            assert_eq!(
                listing(file, &[("BUILD.bazel", build)]).as_deref(),
                expected,
                "{file}"
            );
        }
    }

    #[test]
    fn test_only_a_rinx_library_lists_a_document() {
        // Given — a filegroup listing the same file comes first
        let build = "filegroup(name = \"all\", srcs = [\"index.rst\"])\n\nrinx_library(name = \"docs\", srcs = [\"index.rst\"])\n";

        // When
        let found = listing("index.rst", &[("BUILD.bazel", build)]);

        // Then
        assert_eq!(found.as_deref(), Some("//:docs"));
    }

    #[test]
    fn test_a_srcs_that_cannot_be_evaluated_lists_nothing() {
        for build in [
            "rinx_library(name = \"docs\", srcs = SOURCES)\n",
            "rinx_library(name = \"docs\", srcs = glob(PATTERNS))\n",
            "rinx_library(name = \"docs\", srcs = select({\"//c\": [\"index.rst\"]}))\n",
            "rinx_library(name = NAME, srcs = [\"index.rst\"])\n",
            "my_docs(name = \"docs\", srcs = [\"index.rst\"])\n",
        ] {
            // When / Then
            assert_eq!(
                listing("index.rst", &[("BUILD.bazel", build)]),
                None,
                "{build}"
            );
        }
    }

    #[test]
    fn test_a_file_in_no_package_has_no_library() {
        assert_eq!(listing("docs/index.rst", &[]), None);
    }

    #[test]
    fn test_a_label_shaped_entry_names_its_file() {
        // Given / When
        let found = listing(
            "index.rst",
            &[(
                "BUILD.bazel",
                "rinx_library(name = \"docs\", srcs = [\":index.rst\"])\n",
            )],
        );

        // Then
        assert_eq!(found.as_deref(), Some("//:docs"));
    }

    #[test]
    fn test_glob_matches_as_bazel_does() {
        for (pattern, path, expected) in [
            ("*.rst", "index.rst", true),
            ("*.rst", "sub/index.rst", false),
            ("**/*.rst", "index.rst", true),
            ("**/*.rst", "a/b/index.rst", true),
            ("a/**", "a/b/c.rst", true),
            ("a/**", "b/c.rst", false),
            ("ch?.rst", "ch1.rst", true),
            ("ch?.rst", "ch10.rst", false),
            ("*_test.rst", "a_test.rst", true),
            ("index.rst", "index.rst", true),
            ("index.rst", "index.txt", false),
        ] {
            assert_eq!(glob_matches(pattern, path), expected, "{pattern} {path}");
        }
    }
}

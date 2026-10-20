//! The edit adding labels to a library's `deps`.

use std::cmp::Ordering;

use rinx_ast::{Position, Span};
use rinx_pyconf::tokenize;

use super::calls::{Call, StringList, line_number, point, span_of, top_level_calls};
use super::labels::{compare_labels, spell_label, split_label};

/// One replacement in a build file: `text` in place of `span`, empty for an
/// insertion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildEdit {
    pub span: Span,
    pub text: String,
}

/// Why no edit of the build file adds the labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoEdit {
    /// The file stops being Starlark before the call could be read.
    Unreadable,
    /// No call has the library's name as a written `name = "…"`; a macro
    /// computing it, for one.
    NoTarget,
    /// Two calls have the name, which Bazel refuses.
    Ambiguous,
    /// `deps` is something other than a list of strings.
    ComputedDeps,
    /// Every label is in `deps` already.
    AlreadyDeclared,
}

/// The edits adding `labels` to the `deps` of the library `label`, declared
/// in the build file whose text is `text`.
///
/// Labels in the library's own package are written `:name`, as buildifier
/// does; the others as given. They go where buildifier's order puts them when
/// the list is in that order already, and at its end otherwise, and a list
/// that grows past one item is written one item per line.
///
/// # Errors
///
/// [`NoEdit`] when the call, or its `deps`, is not written so that this
/// edit can be made safely.
pub fn add_deps(text: &str, label: &str, labels: &[String]) -> Result<Vec<BuildEdit>, NoEdit> {
    let (package, name) = split_label(label).ok_or(NoEdit::NoTarget)?;
    let tokenized = tokenize(text);
    if tokenized.error.is_some() {
        return Err(NoEdit::Unreadable);
    }
    let mut calls = top_level_calls(&tokenized.tokens)
        .into_iter()
        .filter(|call| call.name() == Some(name));
    let call = calls.next().ok_or(NoEdit::NoTarget)?;
    if calls.next().is_some() {
        return Err(NoEdit::Ambiguous);
    }
    let lines: Vec<&str> = text.lines().collect();
    let spelled: Vec<String> = labels.iter().map(|dep| spell_label(dep, package)).collect();
    match call.argument("deps") {
        Some(deps) => {
            let list = StringList::read(deps.value).ok_or(NoEdit::ComputedDeps)?;
            let new: Vec<String> = spelled
                .into_iter()
                .filter(|dep| !list.items.iter().any(|(item, ..)| item == dep))
                .collect();
            if new.is_empty() {
                return Err(NoEdit::AlreadyDeclared);
            }
            Ok(list.add(&new, deps.keyword_span(), &lines))
        }
        None => Ok(vec![call.add_argument(&spelled, &lines)]),
    }
}

impl Call<'_> {
    /// The edit adding `deps = [labels]` after its last argument.
    fn add_argument(&self, labels: &[String], lines: &[&str]) -> BuildEdit {
        let multi_line = self.open.span.start.line != self.close.span.start.line;
        let Some(last) = self.arguments.last() else {
            return BuildEdit {
                span: point(self.close.span.start),
                text: format!("deps = {}", inline_list(labels)),
            };
        };
        let after = last
            .comma
            .map_or_else(
                || last.value.last().map(|token| token.span),
                |comma| Some(comma.span),
            )
            .unwrap_or(self.open.span)
            .end;
        let comma = if last.comma.is_some() { "" } else { "," };
        if !multi_line {
            return BuildEdit {
                span: point(after),
                text: format!("{comma} deps = {}", inline_list(labels)),
            };
        }
        let first = self.arguments[0]
            .keyword
            .map_or(self.arguments[0].value[0].span, |token| token.span);
        let indent = indentation_of(lines, first.start.line);
        BuildEdit {
            span: point(after),
            text: format!("{comma}\n{indent}deps = {},", list_text(labels, &indent)),
        }
    }
}

impl StringList<'_> {
    /// The edits adding `new`, none of them in the list yet, to the list of
    /// the argument at `argument`.
    fn add(&self, new: &[String], argument: Span, lines: &[&str]) -> Vec<BuildEdit> {
        let sorted = self
            .items
            .windows(2)
            .all(|pair| compare_labels(&pair[0].0, &pair[1].0) != Ordering::Greater);
        let multi_line = self.open.span.start.line != self.close.span.start.line;
        if !multi_line {
            let mut all: Vec<String> = self.items.iter().map(|(item, ..)| item.clone()).collect();
            insert_all(&mut all, new, sorted);
            let indent = indentation_of(lines, argument.start.line as usize);
            return vec![BuildEdit {
                span: span_of(self.open.span.start, self.close.span.end),
                text: list_text(&all, &indent),
            }];
        }
        let item_indent = self.items.first().map_or_else(
            || format!("{}    ", indentation_of(lines, self.close.span.start.line)),
            |(_, token, _)| indentation_of(lines, token.span.start.line),
        );
        let mut new = new.to_vec();
        if sorted {
            new.sort_by(|left, right| compare_labels(left, right));
        }
        let mut edits = Vec::new();
        let mut comma_added = false;
        for label in &new {
            // After the last item the new one sorts after, so a comment
            // above the next item stays with it.
            let before = if sorted {
                self.items
                    .iter()
                    .rposition(|(item, ..)| compare_labels(item, label) != Ordering::Greater)
            } else {
                self.items.len().checked_sub(1)
            };
            let line = before.map_or(self.open.span.start.line, |index| {
                let (_, token, comma) = &self.items[index];
                if comma.is_none() && !comma_added {
                    comma_added = true;
                    edits.push(BuildEdit {
                        span: point(token.span.end),
                        text: ",".to_string(),
                    });
                }
                comma.map_or(token.span.end.line, |comma| comma.span.end.line)
            });
            let at = Position::new(line_number(line + 1), 1);
            edits.push(BuildEdit {
                span: Span::new(at, at),
                text: format!("{item_indent}{},\n", quoted(label)),
            });
        }
        edits
    }
}

/// `new` added to `items`, each at its place in buildifier's order when
/// `sorted`, at the end otherwise.
fn insert_all(items: &mut Vec<String>, new: &[String], sorted: bool) {
    for label in new {
        let at = if sorted {
            items.partition_point(|item| compare_labels(item, label) != Ordering::Greater)
        } else {
            items.len()
        };
        items.insert(at, label.clone());
    }
}

/// `labels` as a list: inline when it holds one, and otherwise one per
/// line, closed at `indent`.
fn list_text(labels: &[String], indent: &str) -> String {
    if labels.len() < 2 {
        return inline_list(labels);
    }
    let mut text = String::from("[\n");
    for label in labels {
        text.push_str(indent);
        text.push_str("    ");
        text.push_str(&quoted(label));
        text.push_str(",\n");
    }
    text.push_str(indent);
    text.push(']');
    text
}

fn inline_list(labels: &[String]) -> String {
    let items: Vec<String> = labels.iter().map(|label| quoted(label)).collect();
    format!("[{}]", items.join(", "))
}

/// `label` as a Starlark string literal; a label holds no quote or
/// backslash to escape.
fn quoted(label: &str) -> String {
    format!("\"{label}\"")
}

/// The leading whitespace of the 1-based `line`.
fn indentation_of(lines: &[&str], line: usize) -> String {
    let text = lines
        .get(line.saturating_sub(1))
        .copied()
        .unwrap_or_default();
    text.chars().take_while(|c| c.is_whitespace()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `text` with `edits` applied, as a client applies them: each against
    /// the original text, an earlier one first at the same place.
    fn apply(text: &str, edits: &[BuildEdit]) -> String {
        let offset = |position: Position| {
            let line_start: usize = text
                .split_inclusive('\n')
                .take(position.line as usize - 1)
                .map(str::len)
                .sum();
            let line = text[line_start..].split('\n').next().unwrap_or_default();
            line_start
                + line
                    .char_indices()
                    .nth(position.column as usize - 1)
                    .map_or(line.len(), |(index, _)| index)
        };
        let mut ordered: Vec<(usize, &BuildEdit)> = edits.iter().enumerate().collect();
        ordered.sort_by_key(|(order, edit)| {
            (
                std::cmp::Reverse(offset(edit.span.start)),
                std::cmp::Reverse(*order),
            )
        });
        let mut result = text.to_string();
        for (_, edit) in ordered {
            result.replace_range(offset(edit.span.start)..offset(edit.span.end), &edit.text);
        }
        result
    }

    /// `text` once `labels` are added to the deps of `//docs:root`.
    fn adding(text: &str, labels: &[&str]) -> Result<String, NoEdit> {
        let labels: Vec<String> = labels.iter().map(ToString::to_string).collect();
        add_deps(text, "//docs:root", &labels).map(|edits| apply(text, &edits))
    }

    #[test]
    fn test_a_label_joins_a_sorted_list_at_its_place_keeping_comments() {
        // Given
        let text = r#"load("//:defs.bzl", "rinx_library")

rinx_library(
    name = "root",
    srcs = ["index.rst"],
    deps = [
        # The guide's toctree needs it.
        ":guide",
        "//team_b:docs",
    ],
)
"#;

        // When
        let edited = adding(text, &["//team_a:docs"]);

        // Then
        assert_eq!(
            edited.as_deref(),
            Ok(r#"load("//:defs.bzl", "rinx_library")

rinx_library(
    name = "root",
    srcs = ["index.rst"],
    deps = [
        # The guide's toctree needs it.
        ":guide",
        "//team_a:docs",
        "//team_b:docs",
    ],
)
"#)
        );
    }

    #[test]
    fn test_a_label_sorting_first_goes_above_the_first_items_comment() {
        // Given
        let text = "rinx_library(\n    name = \"root\",\n    deps = [\n        # b's\n        \"//b:docs\",\n    ],\n)\n";

        // When
        let edited = adding(text, &[":a"]);

        // Then
        assert_eq!(
            edited.as_deref(),
            Ok(
                "rinx_library(\n    name = \"root\",\n    deps = [\n        \":a\",\n        # b's\n        \"//b:docs\",\n    ],\n)\n"
            )
        );
    }

    #[test]
    fn test_labels_are_appended_to_an_unsorted_list_with_its_missing_comma() {
        // Given
        let text = "rinx_library(\n    name = \"root\",\n    deps = [\n        \"//z:docs\",\n        \"//a:docs\"\n    ],\n)\n";

        // When
        let edited = adding(text, &["//m:docs", "//b:docs"]);

        // Then
        assert_eq!(
            edited.as_deref(),
            Ok(
                "rinx_library(\n    name = \"root\",\n    deps = [\n        \"//z:docs\",\n        \"//a:docs\",\n        \"//m:docs\",\n        \"//b:docs\",\n    ],\n)\n"
            )
        );
    }

    #[test]
    fn test_a_single_line_list_growing_is_written_one_item_per_line() {
        // Given
        let text = "rinx_library(\n    name = \"root\",\n    deps = [\":guide\"],\n)\n";

        // When
        let edited = adding(text, &["//api:docs"]);

        // Then
        assert_eq!(
            edited.as_deref(),
            Ok(
                "rinx_library(\n    name = \"root\",\n    deps = [\n        \":guide\",\n        \"//api:docs\",\n    ],\n)\n"
            )
        );
    }

    #[test]
    fn test_an_empty_list_takes_one_label_inline() {
        // Given
        let text = "rinx_library(\n    name = \"root\",\n    deps = [],\n)\n";

        // When
        let edited = adding(text, &["//api:docs"]);

        // Then
        assert_eq!(
            edited.as_deref(),
            Ok("rinx_library(\n    name = \"root\",\n    deps = [\"//api:docs\"],\n)\n")
        );
    }

    #[test]
    fn test_a_missing_deps_is_added_after_the_last_argument() {
        // Given — with and without a trailing comma
        let with_comma = "rinx_library(\n    name = \"root\",\n    srcs = [\"index.rst\"],\n)\n";
        let without = "rinx_library(\n    name = \"root\",\n    srcs = [\"index.rst\"]\n)\n";

        // When / Then
        let expected = "rinx_library(\n    name = \"root\",\n    srcs = [\"index.rst\"],\n    deps = [\"//api:docs\"],\n)\n";
        assert_eq!(adding(with_comma, &["//api:docs"]).as_deref(), Ok(expected));
        assert_eq!(adding(without, &["//api:docs"]).as_deref(), Ok(expected));
    }

    #[test]
    fn test_a_missing_deps_of_several_labels_is_one_per_line() {
        // Given
        let text = "rinx_library(\n    name = \"root\",\n)\n";

        // When
        let edited = adding(text, &["//a:docs", "//b:docs"]);

        // Then
        assert_eq!(
            edited.as_deref(),
            Ok(
                "rinx_library(\n    name = \"root\",\n    deps = [\n        \"//a:docs\",\n        \"//b:docs\",\n    ],\n)\n"
            )
        );
    }

    #[test]
    fn test_a_single_line_call_gains_deps_inline() {
        // Given
        let text = "docs(name = \"root\", srcs = [\"index.rst\"])\n";

        // When — whatever the rule is called once loaded
        let edited = adding(text, &["//api:docs"]);

        // Then
        assert_eq!(
            edited.as_deref(),
            Ok("docs(name = \"root\", srcs = [\"index.rst\"], deps = [\"//api:docs\"])\n")
        );
    }

    #[test]
    fn test_a_label_of_the_same_package_is_written_relative() {
        // Given
        let text = "rinx_library(\n    name = \"root\",\n    deps = [],\n)\n";

        // When
        let edited = adding(text, &["//docs:guide"]);

        // Then
        assert_eq!(
            edited.as_deref(),
            Ok("rinx_library(\n    name = \"root\",\n    deps = [\":guide\"],\n)\n")
        );
    }

    #[test]
    fn test_only_the_named_call_is_edited() {
        // Given
        let text = "rinx_library(\n    name = \"other\",\n    deps = [],\n)\n\nrinx_library(\n    name = \"root\",\n    deps = [],\n)\n";

        // When
        let edited = adding(text, &["//api:docs"]);

        // Then
        assert_eq!(
            edited.as_deref(),
            Ok(
                "rinx_library(\n    name = \"other\",\n    deps = [],\n)\n\nrinx_library(\n    name = \"root\",\n    deps = [\"//api:docs\"],\n)\n"
            )
        );
    }

    #[test]
    fn test_what_cannot_be_edited_safely_is_refused() {
        let cases = [
            (
                "rinx_library(name = \"root\", deps = COMMON + [\":a\"])\n",
                NoEdit::ComputedDeps,
            ),
            (
                "rinx_library(name = \"root\", deps = select({\"//c\": []}))\n",
                NoEdit::ComputedDeps,
            ),
            ("rinx_library(name = NAME, deps = [])\n", NoEdit::NoTarget),
            (
                "[rinx_library(name = \"root\") for _ in [1]]\n",
                NoEdit::NoTarget,
            ),
            (
                "rinx_library(name = \"root\")\nrinx_library(name = \"root\")\n",
                NoEdit::Ambiguous,
            ),
            (
                "rinx_library(name = \"root\", deps = [\"//api:docs\"])\n",
                NoEdit::AlreadyDeclared,
            ),
            (
                "rinx_library(name = \"root\", deps = [\n",
                NoEdit::Unreadable,
            ),
        ];
        for (text, refusal) in cases {
            // When / Then
            assert_eq!(adding(text, &["//api:docs"]), Err(refusal), "{text}");
        }
    }
}

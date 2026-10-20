//! How a label is spelled in a build file and where buildifier puts it in a
//! list.

use std::cmp::Ordering;

/// `//pkg:name` as its package and name, `//pkg` naming `pkg`'s last
/// component; `None` for a label of another repository.
pub(super) fn split_label(label: &str) -> Option<(&str, &str)> {
    let path = label.strip_prefix("//")?;
    Some(match path.split_once(':') {
        Some((package, name)) => (package, name),
        None => (path, path.rsplit('/').next().unwrap_or(path)),
    })
}

/// `label` as a dependency of a library in `package`: `:name` when it is in
/// the same package.
pub(super) fn spell_label(label: &str, package: &str) -> String {
    match split_label(label) {
        Some((dep_package, name)) if dep_package == package => format!(":{name}"),
        _ => label.to_string(),
    }
}

/// Buildifier's order of labels in a list: `:` labels first, then `//`
/// labels, then other repositories', each by its `:`- and `.`-separated
/// components.
pub(super) fn compare_labels(left: &str, right: &str) -> Ordering {
    let key = |label: &str| {
        let phase = if label.starts_with(':') {
            1
        } else if label.starts_with("//") {
            2
        } else if label.starts_with('@') {
            3
        } else {
            0
        };
        let components: Vec<String> = label
            .replace(':', ".")
            .split('.')
            .map(str::to_string)
            .collect();
        (phase, components)
    };
    key(left).cmp(&key(right))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_labels_compare_in_buildifiers_order() {
        // Given
        let mut labels = vec!["@ext//:x", "//b:docs", ":z", "//a:docs", "//a/b:docs", ":a"];

        // When
        labels.sort_by(|left, right| compare_labels(left, right));

        // Then
        assert_eq!(
            labels,
            [":a", ":z", "//a:docs", "//a/b:docs", "//b:docs", "@ext//:x"]
        );
    }

    #[test]
    fn test_split_label_reads_package_and_name() {
        assert_eq!(split_label("//docs/api:ref"), Some(("docs/api", "ref")));
        assert_eq!(split_label("//docs/api"), Some(("docs/api", "api")));
        assert_eq!(split_label("//:site"), Some(("", "site")));
        assert_eq!(split_label("@other//docs:ref"), None);
    }

    #[test]
    fn test_spell_label_shortens_a_label_of_the_same_package() {
        assert_eq!(spell_label("//docs:guide", "docs"), ":guide");
        assert_eq!(spell_label("//api:docs", "docs"), "//api:docs");
        assert_eq!(spell_label("@ext//docs:x", "docs"), "@ext//docs:x");
    }
}

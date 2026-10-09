//! Which files under a project's source root are its documents, by Sphinx's
//! `exclude_patterns` and `include_patterns`.
//!
//! This is `sphinx.util.matching.get_matching_files` as `Project.discover`
//! calls it: a file is a document when an include pattern matches its path
//! and no exclude pattern does, and a directory an exclude pattern matches is
//! not entered at all. Paths are relative to the source root and
//! `/`-separated; the patterns are [`SphinxPattern`]s, the translation
//! toctree globs use too.

use rinx_toctree::SphinxPattern;

/// What Sphinx always excludes, on top of a project's own patterns
/// (`sphinx.project.EXCLUDE_PATHS`).
const ALWAYS_EXCLUDED: &[&str] = &["**/_sources", ".#*", "**/.#*", "*.lproj/**"];

/// What a project includes when it says nothing: everything.
pub(crate) const INCLUDE_EVERYTHING: &str = "**";

/// A project's exclude and include patterns, compiled.
#[derive(Debug, Clone)]
pub struct Exclusion {
    excluded: Vec<(String, SphinxPattern)>,
    included: Vec<(String, SphinxPattern)>,
}

impl Exclusion {
    /// Nothing excluded: every file under the root is considered, as in a
    /// folder with no configuration.
    #[must_use]
    pub fn none() -> Self {
        Self::from_patterns(&[], &[INCLUDE_EVERYTHING.to_string()])
    }

    /// A Sphinx project's: `excluded` (its `exclude_patterns` and the paths
    /// it excludes implicitly) plus what Sphinx always excludes, and
    /// `included` (its `include_patterns`).
    #[must_use]
    pub fn sphinx(excluded: &[String], included: &[String]) -> Self {
        let mut all: Vec<String> = excluded.to_vec();
        all.extend(ALWAYS_EXCLUDED.iter().map(|pattern| (*pattern).to_string()));
        Self::from_patterns(&all, included)
    }

    fn from_patterns(excluded: &[String], included: &[String]) -> Self {
        let compile = |patterns: &[String]| {
            patterns
                .iter()
                .map(|pattern| (pattern.clone(), SphinxPattern::new(pattern)))
                .collect()
        };
        Self {
            excluded: compile(excluded),
            included: compile(included),
        }
    }

    /// Whether the directory at `relative` is left out, with everything
    /// under it.
    #[must_use]
    pub fn excludes_directory(&self, relative: &str) -> bool {
        self.excluded
            .iter()
            .any(|(_, pattern)| pattern.is_match(relative))
    }

    /// Whether the file at `relative` is considered: included, and not
    /// excluded. Its directories are not asked; see
    /// [`Self::excludes_directory`].
    #[must_use]
    pub fn considers_file(&self, relative: &str) -> bool {
        self.included
            .iter()
            .any(|(_, pattern)| pattern.is_match(relative))
            && !self.excludes_directory(relative)
    }
}

/// Two exclusions are the same when they were written the same.
impl PartialEq for Exclusion {
    fn eq(&self, other: &Self) -> bool {
        let written = |patterns: &[(String, SphinxPattern)]| {
            patterns
                .iter()
                .map(|(pattern, _)| pattern.clone())
                .collect::<Vec<_>>()
        };
        written(&self.excluded) == written(&other.excluded)
            && written(&self.included) == written(&other.included)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(patterns: &[&str]) -> Vec<String> {
        patterns
            .iter()
            .map(|pattern| (*pattern).to_string())
            .collect()
    }

    #[test]
    fn test_none_considers_every_file_and_enters_every_directory() {
        // Given
        let exclusion = Exclusion::none();

        // When / Then
        assert!(exclusion.considers_file("guide/setup.rst"));
        assert!(!exclusion.excludes_directory("_build"));
        assert!(!exclusion.excludes_directory("a/_sources"));
    }

    #[test]
    fn test_sphinx_excludes_its_patterns_and_sphinxs_own() {
        // Given — CPython's
        let exclusion = Exclusion::sphinx(
            &strings(&["includes/*.rst", "venv/*", "README.rst"]),
            &strings(&["**"]),
        );

        // When / Then
        assert!(!exclusion.considers_file("includes/tzinfo.rst"));
        assert!(!exclusion.considers_file("README.rst"));
        assert!(exclusion.considers_file("library/os.rst"));
        assert!(exclusion.excludes_directory("venv/lib"));
        assert!(!exclusion.excludes_directory("venv"));
        assert!(exclusion.excludes_directory("html/_sources"));
        assert!(!exclusion.considers_file("en.lproj/strings.rst"));
        assert!(!exclusion.considers_file(".#draft.rst"));
    }

    #[test]
    fn test_sphinx_excludes_a_directory_named_by_a_pattern() {
        // Given — the quickstart's own exclusions
        let exclusion = Exclusion::sphinx(
            &strings(&["_build", "Thumbs.db", ".DS_Store", "**.ipynb_checkpoints"]),
            &strings(&["**"]),
        );

        // When / Then
        assert!(exclusion.excludes_directory("_build"));
        assert!(!exclusion.excludes_directory("guide/_build"));
        assert!(exclusion.excludes_directory("notebooks/.ipynb_checkpoints"));
    }

    #[test]
    fn test_sphinx_considers_only_included_files() {
        // Given
        let exclusion = Exclusion::sphinx(&[], &strings(&["guide/**", "index.rst"]));

        // When / Then
        assert!(exclusion.considers_file("guide/a/b.rst"));
        assert!(exclusion.considers_file("index.rst"));
        assert!(!exclusion.considers_file("other.rst"));
    }

    #[test]
    fn test_exclusions_are_equal_when_written_the_same() {
        // When / Then
        assert_eq!(
            Exclusion::sphinx(&strings(&["_build"]), &strings(&["**"])),
            Exclusion::sphinx(&strings(&["_build"]), &strings(&["**"]))
        );
        assert_ne!(
            Exclusion::sphinx(&strings(&["_build"]), &strings(&["**"])),
            Exclusion::none()
        );
    }
}

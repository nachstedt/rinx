//! The vocabulary the role table is written in: what one role's [`RoleSpec`]
//! says.
//!
//! A leaf, as `directives/spec/vocabulary.rs` is for directives: it names no
//! role, so every role module builds its spec from it beside its handler, and
//! which roles exist is the table's to say (`inline/regexes.rs`'s `ROLES`).
//! It says nothing about how a role is written down either — the
//! ``:name:`text` `` pattern each spec is matched by is built from it in
//! `inline/regexes.rs` — so a reader of another syntax could name the same
//! roles.

use rinx_ast::Domain;

/// One role this build implements, with a fixed name.
///
/// The roles whose names are not fixed — an entity schema's and the ones a
/// document defines with `.. role::` — have no spec: a schema or a document
/// says what they are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoleSpec {
    /// Every spelling the role is written as, without a domain: one role may
    /// have several (`sub` and `subscript`), and one construct may stand for
    /// several roles that differ only in their name (`pep`, `rfc`, …).
    pub names: &'static [&'static str],
    /// The domains its name may be qualified with (`py:func`, `c:func`).
    /// Whether the bare name is accepted too is [`Self::accepts_bare_name`]'s
    /// to say.
    pub domains: &'static [Domain],
    /// What an `:external:`/`:external+name:` prefix does to it.
    pub external: ExternalPrefix,
    /// What the role is for, in one sentence.
    pub doc: &'static str,
}

/// What an `:external:`/`:external+name:` prefix written before a role's name
/// does to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalPrefix {
    /// It asks for the target in another site's inventory.
    Accepted,
    /// It is read, so it can be refused by name: the role's targets are
    /// never in an inventory, whose entries carry no numbers.
    Refused,
    /// It is not read, and the role stays text with it written.
    Unread,
}

impl ExternalPrefix {
    /// Whether the role's pattern reads the prefix.
    #[must_use]
    pub const fn is_read(self) -> bool {
        !matches!(self, Self::Unread)
    }
}

impl RoleSpec {
    /// Whether the bare name means this role in a document whose default
    /// domain is `default_domain`: always for a role of no domain, or of the
    /// `std` domain, which Sphinx looks a bare role up in whatever the
    /// default; otherwise only when the role takes the default domain, so a
    /// bare `:macro:` is a role only where `c` is the default.
    #[must_use]
    pub fn accepts_bare_name(&self, default_domain: Domain) -> bool {
        self.domains.is_empty()
            || self.domains.contains(&Domain::Std)
            || self.domains.contains(&default_domain)
    }

    /// Every name the role may be written with in a document whose default
    /// domain is `default_domain`: each bare name the role accepts there,
    /// then each qualified with every domain it takes (`func`, `py:func`,
    /// `c:func`).
    pub fn spellings(&self, default_domain: Domain) -> impl Iterator<Item = String> + use<> {
        let names = self.names;
        let bare = if self.accepts_bare_name(default_domain) {
            names
        } else {
            &[]
        };
        bare.iter().map(ToString::to_string).chain(
            self.domains
                .iter()
                .flat_map(move |domain| names.iter().map(move |name| format!("{domain}:{name}"))),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(names: &'static [&'static str], domains: &'static [Domain]) -> RoleSpec {
        RoleSpec {
            names,
            domains,
            external: ExternalPrefix::Unread,
            doc: "",
        }
    }

    #[test]
    fn test_is_read_only_for_a_prefix_the_role_reads() {
        // When / Then
        assert!(ExternalPrefix::Accepted.is_read());
        assert!(ExternalPrefix::Refused.is_read());
        assert!(!ExternalPrefix::Unread.is_read());
    }

    #[test]
    fn test_accepts_bare_name_without_a_domain_whatever_the_default() {
        // Given
        let math = spec(&["math"], &[]);

        // When / Then
        assert!(math.accepts_bare_name(Domain::Py));
        assert!(math.accepts_bare_name(Domain::C));
    }

    #[test]
    fn test_accepts_bare_name_of_a_std_role_whatever_the_default() {
        // Given
        let doc = spec(&["doc"], &[Domain::Std]);

        // When / Then
        assert!(doc.accepts_bare_name(Domain::Py));
        assert!(doc.accepts_bare_name(Domain::C));
    }

    #[test]
    fn test_accepts_bare_name_of_a_domain_role_only_in_its_domain() {
        // Given
        let macro_ = spec(&["macro"], &[Domain::C]);

        // When / Then
        assert!(macro_.accepts_bare_name(Domain::C));
        assert!(!macro_.accepts_bare_name(Domain::Py));
    }

    #[test]
    fn test_spellings_lists_the_bare_names_before_the_qualified_ones() {
        // Given
        let func = spec(&["func", "function"], &[Domain::Py, Domain::C]);

        // When
        let spellings: Vec<String> = func.spellings(Domain::Py).collect();

        // Then
        assert_eq!(
            spellings,
            [
                "func",
                "function",
                "py:func",
                "py:function",
                "c:func",
                "c:function"
            ]
        );
    }

    #[test]
    fn test_spellings_leaves_out_a_bare_name_the_default_domain_lacks() {
        // Given
        let macro_ = spec(&["macro"], &[Domain::C]);

        // When / Then
        assert_eq!(
            macro_.spellings(Domain::Py).collect::<Vec<_>>(),
            ["c:macro"]
        );
    }
}

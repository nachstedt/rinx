//! The vocabulary the directive table is written in: what one directive's
//! [`DirectiveSpec`] says.
//!
//! A leaf: it names no directive, so every module can build its spec from it
//! without depending on the registry that collects them — which directives
//! exist is the registry's to say.

/// One directive this build implements.
#[derive(Debug)]
pub struct DirectiveSpec {
    /// Every spelling the directive is written as: this build's own name
    /// first, then the ones borrowed for compatibility (`entity-table`, then
    /// sphinx-needs' `needtable`).
    pub names: &'static [&'static str],
    /// What the directive is for, in one sentence.
    pub doc: &'static str,
}

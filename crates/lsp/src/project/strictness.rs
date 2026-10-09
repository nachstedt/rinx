//! How strictly a project's findings are reported (ADR-038 §5).
//!
//! A Sphinx project leans on extensions rinx does not run: autodoc's
//! directives are unknown to it, and every reference into an object autodoc
//! would have generated is broken. Reported as warnings, these drown what the
//! author can actually fix. So the server lowers them, here and nowhere else —
//! the parser and the renderer report what they find, unaware of any project.
//!
//! The lowering is keyed on what a diagnostic is about
//! ([`rinx_ast::DiagnosticSubject`]) and on which extensions the project
//! declares, as the [`ExtensionProfiles`] describe them:
//!
//! - an unknown directive a declared extension provides is *Information*;
//! - a reference resolving to nothing, into a domain a declared extension
//!   fills, is a *Hint* — an extension the table does not model may fill any;
//! - everything else stays a *Warning*, as the build reports it. That
//!   includes an unknown directive nothing known provides, even when the
//!   project declares extensions the table does not model: it is as likely a
//!   typo as theirs.

use lsp_types::DiagnosticSeverity;
use rinx_ast::{Diagnostic, DiagnosticCode, DiagnosticSubject};

use super::extension_profile::{ExtensionProfile, ExtensionProfiles, TargetScope};

/// How a project's findings are reported.
///
/// Only the legacy mode exists until a project read from its Bazel build
/// (roadmap #18) gets `Rinx`, under which every diagnostic is exactly what the
/// build reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Strictness {
    /// A project rinx does not build: lowered by what its extensions explain.
    Legacy(ExtensionPolicy),
}

impl Default for Strictness {
    /// A project that declares no extensions, whose findings are all reported
    /// as the build would.
    fn default() -> Self {
        Self::Legacy(ExtensionPolicy::default())
    }
}

impl Strictness {
    /// The legacy strictness of a project declaring `extensions`, as
    /// `conf.py` lists them, against the table this server was built with.
    #[must_use]
    pub fn legacy(extensions: &[String]) -> Self {
        Self::Legacy(ExtensionPolicy::declared(
            extensions,
            ExtensionProfiles::known(),
        ))
    }

    /// How `diagnostic` is reported under this strictness.
    #[must_use]
    pub fn judge(&self, diagnostic: &Diagnostic) -> Judgement {
        match self {
            Self::Legacy(policy) => policy.judge(diagnostic),
        }
    }

    /// The declared extensions the table does not describe, in declaration
    /// order — what the server names once, so an author knows why references
    /// are lowered.
    #[must_use]
    pub fn unmodelled_extensions(&self) -> &[String] {
        match self {
            Self::Legacy(policy) => &policy.unmodelled,
        }
    }
}

/// How one diagnostic is reported: its severity, and its message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Judgement {
    pub severity: DiagnosticSeverity,
    pub message: String,
}

impl Judgement {
    /// `diagnostic` as the build reports it: a warning, worded as found.
    #[must_use]
    pub fn as_reported(diagnostic: &Diagnostic) -> Self {
        Self {
            severity: DiagnosticSeverity::WARNING,
            message: diagnostic.message.clone(),
        }
    }
}

/// The extensions a project declares, split by whether the table describes
/// them.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExtensionPolicy {
    /// The declared extensions the table has a profile for.
    modelled: Vec<ExtensionProfile>,
    /// The declared extensions it does not — each assumed to define targets
    /// in every domain.
    unmodelled: Vec<String>,
}

impl ExtensionPolicy {
    /// The policy of a project declaring `extensions`, as `conf.py` lists
    /// them, against `profiles`. An extension declared twice counts once.
    #[must_use]
    pub fn declared(extensions: &[String], profiles: &ExtensionProfiles) -> Self {
        let mut policy = Self::default();
        for name in extensions {
            match profiles.get(name) {
                Some(profile) if !policy.modelled.contains(profile) => {
                    policy.modelled.push(profile.clone());
                }
                None if !policy.unmodelled.contains(name) => policy.unmodelled.push(name.clone()),
                _ => {}
            }
        }
        policy
    }

    fn judge(&self, diagnostic: &Diagnostic) -> Judgement {
        let warning = || Judgement::as_reported(diagnostic);
        let Some(subject) = &diagnostic.subject else {
            return warning();
        };
        match subject {
            DiagnosticSubject::Directive(name)
                if diagnostic.code == DiagnosticCode::DirectiveUnknown =>
            {
                self.provider_of(name).map_or_else(warning, |extension| Judgement {
                    severity: DiagnosticSeverity::INFORMATION,
                    message: format!(
                        "`{name}` comes from `{extension}`, which rinx does not analyse; contents not checked"
                    ),
                })
            }
            DiagnosticSubject::DomainReference(_) | DiagnosticSubject::AnyReference => {
                let definers = self.definers_of(subject);
                if definers.is_empty() {
                    return warning();
                }
                Judgement {
                    severity: DiagnosticSeverity::HINT,
                    message: format!(
                        "{}; it may be defined by {}, which rinx does not analyse",
                        diagnostic.message,
                        quoted_list(&definers)
                    ),
                }
            }
            DiagnosticSubject::Directive(_) => warning(),
        }
    }

    /// The first declared, modelled extension providing the directive
    /// `name`.
    fn provider_of(&self, name: &str) -> Option<&str> {
        self.modelled
            .iter()
            .find(|profile| profile.directives.contains(name))
            .map(|profile| profile.name.as_str())
    }

    /// The declared extensions that may define what a reference about
    /// `subject` missed: the modelled ones whose targets cover it, then every
    /// unmodelled one.
    fn definers_of(&self, subject: &DiagnosticSubject) -> Vec<&str> {
        self.modelled
            .iter()
            .filter(|profile| profile.produces_targets.covers(subject))
            .map(|profile| profile.name.as_str())
            .chain(
                self.unmodelled
                    .iter()
                    .filter(|_| TargetScope::Everything.covers(subject))
                    .map(String::as_str),
            )
            .collect()
    }
}

/// `names` as `` `a` ``, `` `a` and `b` `` or `` `a`, `b` and `c` ``.
fn quoted_list(names: &[&str]) -> String {
    let quoted: Vec<String> = names.iter().map(|name| format!("`{name}`")).collect();
    match quoted.split_last() {
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::{Domain, Position, Span};

    const SPAN: Span = Span::new(Position::new(2, 1), Position::new(2, 20));

    fn policy(extensions: &[&str]) -> Strictness {
        let extensions: Vec<String> = extensions.iter().map(|e| (*e).to_string()).collect();
        Strictness::legacy(&extensions)
    }

    fn unknown_directive(name: &str) -> Diagnostic {
        Diagnostic::new(
            DiagnosticCode::DirectiveUnknown,
            format!("unknown directive type '{name}'"),
            SPAN,
        )
        .about(DiagnosticSubject::Directive(name.to_string()))
    }

    fn broken_reference(code: DiagnosticCode, subject: DiagnosticSubject) -> Diagnostic {
        Diagnostic::new(code, "broken domain object 'spam.eggs'", SPAN).about(subject)
    }

    fn py_reference() -> Diagnostic {
        broken_reference(
            DiagnosticCode::LinkBrokenObject,
            DiagnosticSubject::DomainReference(Domain::Py),
        )
    }

    #[test]
    fn test_declared_splits_modelled_from_unmodelled_extensions_once_each() {
        // Given
        let extensions = [
            "sphinx.ext.autodoc",
            "pyspecific",
            "sphinx.ext.autodoc",
            "pyspecific",
        ]
        .map(String::from);

        // When
        let policy = ExtensionPolicy::declared(&extensions, ExtensionProfiles::known());

        // Then
        let modelled: Vec<&str> = policy.modelled.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(modelled, ["sphinx.ext.autodoc"]);
        assert_eq!(policy.unmodelled, ["pyspecific"]);
    }

    #[test]
    fn test_unmodelled_extensions_are_named_in_declaration_order() {
        // Given / When
        let strictness = policy(&["b_ext", "sphinx.ext.todo", "a_ext"]);

        // Then
        assert_eq!(strictness.unmodelled_extensions(), ["b_ext", "a_ext"]);
    }

    #[test]
    fn test_a_directive_of_a_declared_extension_is_information() {
        // Given / When
        let judgement = policy(&["sphinx.ext.autodoc"]).judge(&unknown_directive("automodule"));

        // Then
        assert_eq!(judgement.severity, DiagnosticSeverity::INFORMATION);
        assert_eq!(
            judgement.message,
            "`automodule` comes from `sphinx.ext.autodoc`, which rinx does not analyse; contents not checked"
        );
    }

    #[test]
    fn test_a_directive_of_an_undeclared_extension_stays_a_warning() {
        // Given — autodoc provides it, but this project does not use autodoc
        let diagnostic = unknown_directive("automodule");

        // When
        let judgement = policy(&["sphinx.ext.todo"]).judge(&diagnostic);

        // Then
        assert_eq!(judgement.severity, DiagnosticSeverity::WARNING);
        assert_eq!(judgement.message, diagnostic.message);
    }

    #[test]
    fn test_an_unmodelled_extension_does_not_explain_an_unknown_directive() {
        // Given — nothing says `audit-event` is `pyspecific`'s, or a typo
        let diagnostic = unknown_directive("audit-event");

        // When / Then
        assert_eq!(
            policy(&["pyspecific"]).judge(&diagnostic).severity,
            DiagnosticSeverity::WARNING
        );
    }

    #[test]
    fn test_a_reference_into_a_domain_a_declared_extension_fills_is_a_hint() {
        // Given / When
        let judgement = policy(&["sphinx.ext.autodoc"]).judge(&py_reference());

        // Then
        assert_eq!(judgement.severity, DiagnosticSeverity::HINT);
        assert_eq!(
            judgement.message,
            "broken domain object 'spam.eggs'; it may be defined by `sphinx.ext.autodoc`, which rinx does not analyse"
        );
    }

    #[test]
    fn test_a_reference_into_a_domain_no_declared_extension_fills_stays_a_warning() {
        // Given — autodoc fills `py`, not `c`
        let diagnostic = broken_reference(
            DiagnosticCode::LinkBrokenObject,
            DiagnosticSubject::DomainReference(Domain::C),
        );

        // When / Then
        assert_eq!(
            policy(&["sphinx.ext.autodoc"]).judge(&diagnostic).severity,
            DiagnosticSeverity::WARNING
        );
    }

    #[test]
    fn test_an_unmodelled_extension_may_define_a_reference_of_any_domain() {
        // Given
        let diagnostic = broken_reference(
            DiagnosticCode::LinkBrokenAny,
            DiagnosticSubject::AnyReference,
        );

        // When
        let judgement =
            policy(&["sphinx.ext.autodoc", "c_annotations", "pyspecific"]).judge(&diagnostic);

        // Then — autodoc fills one domain, so it cannot explain an `:any:`
        assert_eq!(judgement.severity, DiagnosticSeverity::HINT);
        assert!(
            judgement.message.ends_with(
                "defined by `c_annotations` and `pyspecific`, which rinx does not analyse"
            ),
            "{}",
            judgement.message
        );
    }

    #[test]
    fn test_a_label_or_document_reference_stays_a_warning() {
        // Given — no subject: a `:ref:` or `:doc:` is never an extension's
        let diagnostic = Diagnostic::new(DiagnosticCode::LinkBrokenRef, "broken ref", SPAN);

        // When / Then
        assert_eq!(
            policy(&["pyspecific"]).judge(&diagnostic).severity,
            DiagnosticSeverity::WARNING
        );
    }

    #[test]
    fn test_a_project_declaring_nothing_reports_every_finding_as_a_warning() {
        // Given / When / Then
        assert_eq!(
            Strictness::default().judge(&py_reference()).severity,
            DiagnosticSeverity::WARNING
        );
    }

    #[test]
    fn test_as_reported_is_a_warning_worded_as_found() {
        // Given / When
        let judgement = Judgement::as_reported(&py_reference());

        // Then
        assert_eq!(
            judgement,
            Judgement {
                severity: DiagnosticSeverity::WARNING,
                message: "broken domain object 'spam.eggs'".to_string(),
            }
        );
    }

    #[test]
    fn test_quoted_list_joins_one_two_and_three_names() {
        // Given / When / Then
        assert_eq!(quoted_list(&["a"]), "`a`");
        assert_eq!(quoted_list(&["a", "b"]), "`a` and `b`");
        assert_eq!(quoted_list(&["a", "b", "c"]), "`a`, `b` and `c`");
    }
}

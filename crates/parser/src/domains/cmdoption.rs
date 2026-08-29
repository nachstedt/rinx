use super::parse_body;
use crate::headings::Adornment;
use rusty_sphinx_ast::{Domain, DomainObjectBody, NonEmptyVector};

/// Parses a `.. option::`/`.. cmdoption::` body. Unlike every other domain
/// object type, `StdCmdoption` has no directive-specific option lines to strip
/// off the front — real Sphinx's `Cmdoption` directive class has none — so the
/// whole body is the docstring, parsed the same way [`parse_body`] handles
/// it for `c:function`/`c:macro`.
///
/// The only extra work here is a diagnostic pass over each raw signature
/// line's comma-separated specs (see
/// [`rusty_sphinx_ast::split_option_line_specs`]/
/// [`rusty_sphinx_ast::extract_option_name`]), flagging any spec that
/// doesn't match Sphinx's `option_desc_re` shape. The spec itself is never
/// dropped — [`extract_option_name`](rusty_sphinx_ast::extract_option_name)
/// falls back to the whole spec rather than losing it — only its
/// malformedness is surfaced here; the actual split+extract that drives
/// indexing/rendering is cheap enough to safely redo later, directly from
/// these same stored raw strings (see `index_domain_object`/
/// `render_domain_object`), rather than caching it on the AST.
pub(super) fn parse_cmdoption(
    signatures: NonEmptyVector<String>,
    body_lines: &[&str],
    adornment_order: &mut Vec<Adornment>,
    diagnostics: &mut Vec<String>,
    default_domain: Domain,
) -> DomainObjectBody {
    for line in signatures.as_slice() {
        for spec in rusty_sphinx_ast::split_option_line_specs(line) {
            if !looks_like_option_spec(&spec) {
                diagnostics.push(format!(
                    "option: malformed option spec '{spec}', should look like \"opt\", \"-opt args\", \"--opt args\", \"/opt args\", or \"+opt args\""
                ));
            }
        }
    }

    DomainObjectBody::StdCmdoption {
        signatures,
        body: parse_body(body_lines, adornment_order, diagnostics, default_domain),
    }
}

/// Whether `spec` matches Sphinx's `option_desc_re` shape (a `-`/`--`/`/`/`+`
/// sigil followed by at least one non-whitespace, non-`=` character) — used
/// only to decide whether [`parse_cmdoption`] should emit a diagnostic.
/// [`rusty_sphinx_ast::extract_option_name`] is still what derives the
/// actual name used for indexing/rendering, malformed or not; this is a
/// separate, cheap-to-recompute check rather than a shared "did it fall
/// back" flag, since duplicating a five-line scan is simpler than plumbing
/// that flag out of a function whose contract is just "give me a name".
fn looks_like_option_spec(spec: &str) -> bool {
    let spec = spec.trim();
    let sigil_len = if spec.starts_with("--") {
        2
    } else if spec.starts_with(['-', '/', '+']) {
        1
    } else {
        return false;
    };
    spec[sigil_len..]
        .chars()
        .next()
        .is_some_and(|c| !c.is_whitespace() && c != '=')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use rusty_sphinx_ast::{Directive, Node};

    #[test]
    fn test_looks_like_option_spec_accepts_short_and_long_flags() {
        // Given / When / Then
        assert!(looks_like_option_spec("-m <module>"));
        assert!(looks_like_option_spec("--module <module>"));
        assert!(looks_like_option_spec("/Wall"));
        assert!(looks_like_option_spec("+x"));
        assert!(looks_like_option_spec("-h"));
    }
    #[test]
    fn test_looks_like_option_spec_rejects_missing_sigil() {
        // Given / When / Then
        assert!(!looks_like_option_spec("not-a-flag"));
    }
    #[test]
    fn test_looks_like_option_spec_rejects_bare_sigil() {
        // Given / When / Then
        assert!(!looks_like_option_spec("-"));
    }
    #[test]
    fn test_parse_creates_cmdoption_domain_object_with_single_flag() {
        // Given
        let input = ".. option:: -m <module-name>\n\n   Search sys.path.";

        // When
        let doc = parse("test.rst", input);

        // Then
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::StdCmdoption {
            signatures,
            body,
        })) = &doc.nodes[0]
        {
            assert_eq!(signatures.as_slice(), ["-m <module-name>"]);
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected StdCmdoption, got {:?}", doc.nodes[0]);
        }
    }
    #[test]
    fn test_parse_creates_cmdoption_domain_object_with_comma_separated_flags() {
        // Given — real Sphinx renders this as one shared `<dt>`; the parser
        // stores the raw line verbatim, unsplit (see `DomainObjectBody::StdCmdoption`).
        let input = ".. option:: -c, --compress\n\n   Compress files.";

        // When
        let doc = parse("test.rst", input);

        // Then
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::StdCmdoption {
            signatures,
            ..
        })) = &doc.nodes[0]
        {
            assert_eq!(signatures.as_slice(), ["-c, --compress"]);
        } else {
            panic!("Expected StdCmdoption, got {:?}", doc.nodes[0]);
        }
    }
    #[test]
    fn test_parse_creates_cmdoption_domain_object_from_continuation_lines() {
        // Given — the `mimetypes.rst` shape: one flag per line, no commas.
        let input = ".. cmdoption:: -h\n                --help\n\n   Show help.";

        // When
        let doc = parse("test.rst", input);

        // Then
        if let Node::Directive(Directive::DomainObject(DomainObjectBody::StdCmdoption {
            signatures,
            ..
        })) = &doc.nodes[0]
        {
            assert_eq!(signatures.as_slice(), ["-h", "--help"]);
        } else {
            panic!("Expected StdCmdoption, got {:?}", doc.nodes[0]);
        }
    }
    #[test]
    fn test_parse_cmdoption_recognized_regardless_of_default_domain() {
        // Given — `std`-domain, so unlike `py`/`c` object types it must not
        // be gated by `default_domain`.
        let input = ".. option:: -h";

        // When
        let doc = crate::parse_with_domain("test.rst", input, rusty_sphinx_ast::Domain::C);

        // Then
        assert!(matches!(
            &doc.nodes[0],
            Node::Directive(Directive::DomainObject(
                DomainObjectBody::StdCmdoption { .. }
            ))
        ));
    }
    #[test]
    fn test_parse_malformed_option_spec_emits_diagnostic_but_keeps_it() {
        // Given — no leading sigil at all.
        let input = ".. option:: not-a-flag";

        // When
        let doc = parse("test.rst", input);

        // Then — never lose content, matching `extract_c_object_name`'s
        // fallback convention.
        assert!(matches!(
            &doc.nodes[0],
            Node::Directive(Directive::DomainObject(
                DomainObjectBody::StdCmdoption { .. }
            ))
        ));
        assert!(
            doc.diagnostics
                .iter()
                .any(|d| d.contains("malformed option spec"))
        );
    }
}

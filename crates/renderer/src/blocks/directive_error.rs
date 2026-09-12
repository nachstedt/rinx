//! The visible block a directive this build could not turn into content is
//! drawn as.
//!
//! Both [`rusty_sphinx_ast::Directive::Unknown`] and
//! [`rusty_sphinx_ast::Directive::Malformed`] render through here, because the
//! reason they exist is the same one: a directive that produced nothing at all
//! took every word written inside it off the page with it, and a reader had no
//! way to tell. Quoting the source back is what makes the loss visible, the way
//! `crate::math` shows LaTeX its backend could not read.
//!
//! The two differ only in what they say: an unknown name is named as unknown,
//! while a malformed directive prints the message its parse-time diagnostic
//! reported, so the page and the build log give one explanation.

use rusty_sphinx_ast::Directive;

/// Renders the error block for `Unknown`/`Malformed`, or nothing for any other
/// directive.
pub(super) fn render_directive_error(html: &mut String, directive: &Directive) {
    let (name, argument, body, message) = match directive {
        Directive::Unknown {
            name,
            argument,
            body,
        } => (
            name,
            argument,
            body,
            format!("unknown directive type '{name}'"),
        ),
        Directive::Malformed {
            name,
            argument,
            body,
            message,
        } => (name, argument, body, message.clone()),
        _ => return,
    };
    html.push_str("<div class=\"directive-error\">\n<p class=\"directive-error-message\">");
    html.push_str(&html_escape::encode_text(&message));
    html.push_str("</p>\n<pre class=\"directive-error-source\">");
    html.push_str(&html_escape::encode_text(&directive_source(
        name, argument, body,
    )));
    html.push_str("</pre>\n</div>\n");
}

/// Reassembles the directive as its author would have written it: the `.. `
/// marker line, then the body indented under it.
///
/// The indent is three spaces — docutils' own convention — rather than
/// whatever the author used, because the original width is not recorded: the
/// parser stores the body with its *relative* indentation preserved and its
/// common prefix stripped. So this is the directive's source in shape and
/// content, not byte-for-byte, and nothing should be built on it round-tripping.
fn directive_source(name: &str, argument: &str, body: &str) -> String {
    let mut source = format!(".. {name}::");
    if !argument.is_empty() {
        source.push(' ');
        source.push_str(argument);
    }
    for line in body.lines() {
        source.push('\n');
        if !line.is_empty() {
            source.push_str("   ");
            source.push_str(line);
        }
    }
    source
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_directive_error_names_an_unknown_directive() {
        // Given
        let directive = Directive::Unknown {
            name: "mermaid".to_string(),
            argument: "flow".to_string(),
            body: "graph TD;".to_string(),
        };
        let mut html = String::new();

        // When
        render_directive_error(&mut html, &directive);

        // Then
        assert!(html.contains("<div class=\"directive-error\">"));
        assert!(html.contains("unknown directive type 'mermaid'"));
        assert!(html.contains(".. mermaid:: flow\n   graph TD;"));
    }

    #[test]
    fn test_render_directive_error_prints_a_malformed_directives_own_message() {
        // Given
        let directive = Directive::Malformed {
            name: "figure".to_string(),
            argument: String::new(),
            body: "A caption.".to_string(),
            message: "figure: the directive needs an image path".to_string(),
        };
        let mut html = String::new();

        // When
        render_directive_error(&mut html, &directive);

        // Then
        assert!(html.contains("figure: the directive needs an image path"));
        assert!(!html.contains("unknown directive type"));
        assert!(html.contains(".. figure::\n   A caption."));
    }

    #[test]
    fn test_render_directive_error_escapes_the_source_it_quotes() {
        // Given
        let directive = Directive::Unknown {
            name: "raw".to_string(),
            argument: "html".to_string(),
            body: "<script>alert(\"x\")</script>".to_string(),
        };
        let mut html = String::new();

        // When
        render_directive_error(&mut html, &directive);

        // Then
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn test_render_directive_error_ignores_any_other_directive() {
        // Given
        let directive = Directive::CNamespacePop;
        let mut html = String::new();

        // When
        render_directive_error(&mut html, &directive);

        // Then
        assert_eq!(html, "");
    }

    #[test]
    fn test_directive_source_keeps_relative_indentation_and_blank_lines() {
        // Given
        let body = "first\n\n  nested";

        // When
        let source = directive_source("grid", "2 2", body);

        // Then
        assert_eq!(source, ".. grid:: 2 2\n   first\n\n     nested");
    }

    #[test]
    fn test_directive_source_omits_the_space_when_there_is_no_argument() {
        // Given
        let body = "";

        // When
        let source = directive_source("mermaid", "", body);

        // Then
        assert_eq!(source, ".. mermaid::");
    }
}

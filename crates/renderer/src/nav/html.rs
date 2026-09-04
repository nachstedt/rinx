//! Writing an expanded navigation tree as the `<ul>` an in-page
//! `.. toctree::` renders to.
//!
//! The sidebar does not come through here — it hands the same
//! [`ResolvedNavEntry`] tree to the page template instead, so a theme can lay
//! it out however it likes.

use std::fmt::Write as _;

use super::entry::{ResolvedNavEntry, format_secnumber};

/// Writes `entries` as a `<ul>`, preceded by `caption` when the toctree set
/// one.
pub(crate) fn write_nav_list(
    html: &mut String,
    entries: &[ResolvedNavEntry],
    caption: Option<&str>,
) {
    if let Some(caption) = caption {
        let escaped = html_escape::encode_text(caption);
        let _ = writeln!(
            html,
            "<p class=\"caption\"><span class=\"caption-text\">{escaped}</span></p>"
        );
    }

    let _ = writeln!(html, "<ul>");
    for entry in entries {
        write_entry(html, entry);
    }
    let _ = writeln!(html, "</ul>");
}

/// Writes one entry and, recursively, whatever nests under it.
fn write_entry(html: &mut String, entry: &ResolvedNavEntry) {
    match entry {
        ResolvedNavEntry::External { title, href } => {
            let href_text = href.to_string();
            let href_attr = html_escape::encode_double_quoted_attribute(&href_text);
            let escaped = html_escape::encode_text(title);
            let _ = writeln!(
                html,
                "  <li><a class=\"reference external\" href=\"{href_attr}\">{escaped}</a></li>"
            );
        }
        ResolvedNavEntry::Page {
            title,
            href,
            secnumber,
            is_current,
            children,
            ..
        } => {
            let href_text = href.to_string();
            let href_attr = html_escape::encode_double_quoted_attribute(&href_text);
            let escaped = html_escape::encode_text(title);
            let class = if *is_current {
                " class=\"current\""
            } else {
                ""
            };
            let number = secnumber.as_ref().map_or_else(String::new, |number| {
                format!(
                    "<span class=\"section-number\">{} </span>",
                    format_secnumber(number)
                )
            });
            let _ = write!(
                html,
                "  <li{class}><a href=\"{href_attr}\">{number}{escaped}</a>"
            );

            if children.is_empty() {
                let _ = writeln!(html, "</li>");
            } else {
                let _ = writeln!(html, "\n<ul>");
                for child in children {
                    write_entry(html, child);
                }
                let _ = writeln!(html, "</ul>\n  </li>");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(title: &str, href: &str, children: Vec<ResolvedNavEntry>) -> ResolvedNavEntry {
        ResolvedNavEntry::Page {
            title: title.to_string(),
            href: minijinja::Value::from_safe_string(href.to_string()),
            anchor: None,
            secnumber: None,
            is_current: false,
            is_ancestor: false,
            children,
        }
    }

    #[test]
    fn test_write_nav_list_renders_a_leaf_as_a_list_item() {
        // Given
        let entries = vec![page("Intro", "intro.html", vec![])];
        let mut html = String::new();

        // When
        write_nav_list(&mut html, &entries, None);

        // Then
        assert_eq!(
            html,
            "<ul>\n  <li><a href=\"intro.html\">Intro</a></li>\n</ul>\n"
        );
    }

    #[test]
    fn test_write_nav_list_nests_children_in_an_inner_list() {
        // Given
        let entries = vec![page(
            "Guide",
            "guide.html",
            vec![page("Setup", "guide.html#setup", vec![])],
        )];
        let mut html = String::new();

        // When
        write_nav_list(&mut html, &entries, None);

        // Then
        assert!(html.contains("<a href=\"guide.html\">Guide</a>"), "{html}");
        assert!(
            html.contains("<a href=\"guide.html#setup\">Setup</a>"),
            "{html}"
        );
        assert_eq!(html.matches("<ul>").count(), 2, "{html}");
    }

    #[test]
    fn test_write_nav_list_renders_a_caption_above_the_list() {
        // Given
        let entries = vec![page("Intro", "intro.html", vec![])];
        let mut html = String::new();

        // When
        write_nav_list(&mut html, &entries, Some("Getting Started"));

        // Then
        assert!(
            html.starts_with(
                "<p class=\"caption\"><span class=\"caption-text\">Getting Started</span></p>\n<ul>"
            ),
            "{html}"
        );
    }

    #[test]
    fn test_write_nav_list_escapes_a_caption() {
        // Given
        let mut html = String::new();

        // When
        write_nav_list(&mut html, &[], Some("A & B <script>"));

        // Then
        assert!(html.contains("A &amp; B &lt;script&gt;"), "{html}");
        assert!(!html.contains("<script>"), "{html}");
    }

    #[test]
    fn test_write_nav_list_marks_the_current_entry() {
        // Given
        let entries = vec![ResolvedNavEntry::Page {
            title: "Intro".to_string(),
            href: minijinja::Value::from_safe_string("intro.html".to_string()),
            anchor: None,
            secnumber: None,
            is_current: true,
            is_ancestor: false,
            children: Vec::new(),
        }];
        let mut html = String::new();

        // When
        write_nav_list(&mut html, &entries, None);

        // Then
        assert!(html.contains("<li class=\"current\">"), "{html}");
    }

    #[test]
    fn test_write_nav_list_renders_a_section_number() {
        // Given
        let entries = vec![ResolvedNavEntry::Page {
            title: "Install".to_string(),
            href: minijinja::Value::from_safe_string("guide.html".to_string()),
            anchor: None,
            secnumber: Some(vec![2, 1]),
            is_current: false,
            is_ancestor: false,
            children: Vec::new(),
        }];
        let mut html = String::new();

        // When
        write_nav_list(&mut html, &entries, None);

        // Then
        assert!(
            html.contains("<span class=\"section-number\">2.1. </span>Install"),
            "{html}"
        );
    }

    #[test]
    fn test_write_nav_list_marks_an_external_entry() {
        // Given
        let entries = vec![ResolvedNavEntry::External {
            title: "Upstream".to_string(),
            href: minijinja::Value::from_safe_string("https://example.org".to_string()),
        }];
        let mut html = String::new();

        // When
        write_nav_list(&mut html, &entries, None);

        // Then
        assert!(
            html.contains("class=\"reference external\" href=\"https://example.org\""),
            "{html}"
        );
    }

    #[test]
    fn test_write_nav_list_escapes_entry_titles() {
        // Given
        let entries = vec![page("A & B", "a.html", vec![])];
        let mut html = String::new();

        // When
        write_nav_list(&mut html, &entries, None);

        // Then
        assert!(html.contains("A &amp; B"), "{html}");
    }

    #[test]
    fn test_write_nav_list_of_nothing_is_an_empty_list() {
        // Given
        let mut html = String::new();

        // When
        write_nav_list(&mut html, &[], None);

        // Then
        assert_eq!(html, "<ul>\n</ul>\n");
    }
}

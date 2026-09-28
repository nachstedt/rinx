//! What one render knows about `numfig`: whether it is on, the formats, and
//! the number each of this page's captions shows.
//!
//! Flat at the crate root because both trees reach it — `blocks/` writes a
//! number in front of a caption, `inline/` shows one in a `:numref:`.
//!
//! A caption's number is looked up by the directive's *address*. The analyzer
//! stored numbers by an element's position in
//! [`rinx_ast::enumerable_elements`]; calling that same function on the same
//! document here, once per page, recovers which directive each position is
//! without the renderer having to count elements itself — a count its own
//! traversal could get wrong by descending into one container fewer.

use std::collections::HashMap;

use rinx_ast::{Directive, Document, enumerable_elements};
use rinx_index::{ProjectIndex, join_number};

use crate::config::{NumfigFormat, SiteConfig};

/// `numfig`'s state for one page.
pub(crate) struct Numbering<'a> {
    enabled: bool,
    formats: &'a NumfigFormat,
    /// The number text each numbered caption on this page starts with, keyed
    /// by the directive's address. Empty when `numfig` is off.
    captions: HashMap<*const Directive, String>,
}

impl<'a> Numbering<'a> {
    /// Reads `config`'s settings and, when `numfig` is on, formats the number
    /// of every captioned element of `doc` the index numbered.
    pub(crate) fn new(doc: &Document, index: &ProjectIndex, config: &'a SiteConfig) -> Self {
        let mut captions = HashMap::new();
        if config.numfig
            && let Some(numbers) = index.element_numbers.get(&doc.path)
        {
            for (ordinal, element) in enumerable_elements(&doc.nodes).into_iter().enumerate() {
                if let Some(number) = numbers.get(ordinal) {
                    let text = config
                        .numfig_format
                        .for_kind(element.kind)
                        .apply(&join_number(number), None);
                    captions.insert(std::ptr::from_ref(element.directive), text);
                }
            }
        }
        Self {
            enabled: config.numfig,
            formats: &config.numfig_format,
            captions,
        }
    }

    /// Whether `numfig` is on for this site.
    pub(crate) const fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// The site's formats.
    pub(crate) const fn formats(&self) -> &NumfigFormat {
        self.formats
    }

    /// The number text `directive`'s caption starts with, if it has one.
    pub(crate) fn caption_number(&self, directive: &Directive) -> Option<&str> {
        self.captions
            .get(&std::ptr::from_ref(directive))
            .map(String::as_str)
    }
}

/// Writes a caption's number the way Sphinx does, a space included inside
/// the span; nothing for a caption with no number.
pub(crate) fn write_caption_number(html: &mut String, number: Option<&str>) {
    if let Some(number) = number {
        html.push_str("<span class=\"caption-number\">");
        html.push_str(&html_escape::encode_text(number));
        html.push_str(" </span>");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinx_ast::{AssetUri, Figure, ImageOptions, InlineNode, Node};
    use rinx_index::ElementNumbers;

    fn figure(caption: Option<&str>) -> Node {
        let mut figure = Figure::new(ImageOptions::new(AssetUri::new("a.png")));
        figure.caption = caption.map(|text| vec![InlineNode::Text(text.to_string())]);
        Node::Directive(Directive::Figure(Box::new(figure)))
    }

    fn directive(node: &Node) -> &Directive {
        let Node::Directive(directive) = node else {
            unreachable!()
        };
        directive
    }

    /// A page with an uncaptioned figure between two captioned ones, the
    /// second of which the index numbered 1.2.
    fn page() -> (Document, ProjectIndex) {
        let doc = Document::new(
            "guide.rst".to_string(),
            vec![figure(Some("One")), figure(None), figure(Some("Two"))],
        );
        let mut numbers = ElementNumbers::default();
        numbers.set(1, vec![1, 2]);
        let mut index = ProjectIndex::default();
        index
            .element_numbers
            .insert("guide.rst".to_string(), numbers);
        (doc, index)
    }

    #[test]
    fn test_caption_number_finds_each_directive_by_its_position() {
        // Given
        let (doc, index) = page();
        let config = SiteConfig {
            numfig: true,
            ..SiteConfig::default()
        };

        // When
        let numbering = Numbering::new(&doc, &index, &config);

        // Then — the uncaptioned figure is no position at all
        assert_eq!(numbering.caption_number(directive(&doc.nodes[0])), None);
        assert_eq!(numbering.caption_number(directive(&doc.nodes[1])), None);
        assert_eq!(
            numbering.caption_number(directive(&doc.nodes[2])),
            Some("Fig. 1.2")
        );
    }

    #[test]
    fn test_no_caption_is_numbered_while_numfig_is_off() {
        // Given
        let (doc, index) = page();
        let config = SiteConfig::default();

        // When
        let numbering = Numbering::new(&doc, &index, &config);

        // Then
        assert!(!numbering.is_enabled());
        assert_eq!(numbering.caption_number(directive(&doc.nodes[2])), None);
    }

    #[test]
    fn test_write_caption_number_puts_the_space_inside_the_span() {
        // Given
        let mut html = String::new();

        // When
        write_caption_number(&mut html, Some("Table 3"));
        write_caption_number(&mut html, None);

        // Then
        assert_eq!(html, "<span class=\"caption-number\">Table 3 </span>");
    }

    #[test]
    fn test_a_rendered_page_numbers_each_kind_of_caption() {
        // Given a figure, a list-table and a code block, each captioned and
        // numbered 1 by the index
        let table = Node::Directive(Directive::DataTable {
            source: rinx_ast::TableSource::List,
            title: Some("A table".to_string()),
            header_rows: 0,
            stub_columns: 0,
            widths: None,
            width: None,
            align: None,
            classes: Vec::new(),
            name: None,
            rows: Vec::new(),
        });
        let code = Node::Directive(Directive::CodeBlock(rinx_ast::CodeBlock {
            source: rinx_ast::CodeBlockSource::CodeBlock,
            language: rinx_ast::CodeLanguage::Inherit,
            content: "x".to_string(),
            caption: Some("A listing".to_string()),
            name: None,
            classes: Vec::new(),
            linenos: false,
            lineno_start: None,
            emphasize_lines: Vec::new(),
            force: false,
            span: None,
        }));
        let parsed = Document::new(
            "guide.rst".to_string(),
            vec![figure(Some("A figure")), table, code],
        );
        let mut numbers = ElementNumbers::default();
        for ordinal in 0..3 {
            numbers.set(ordinal, vec![1]);
        }
        let mut index = ProjectIndex::default();
        index
            .element_numbers
            .insert("guide.rst".to_string(), numbers);
        let config = SiteConfig {
            numfig: true,
            ..SiteConfig::default()
        };

        // When
        let html = crate::render_with_config(&parsed, &index, "guide.rst", &config).html;

        // Then
        assert!(
            html.contains("<p><span class=\"caption-number\">Fig. 1 </span>A figure</p>"),
            "{html}"
        );
        assert!(
            html.contains(
                "<caption><span class=\"caption-number\">Table 1 </span>A table</caption>"
            ),
            "{html}"
        );
        assert!(
            html.contains(
                "<div class=\"code-block-caption\"><span class=\"caption-number\">Listing 1 </span>\
                 <span class=\"caption-text\">A listing</span></div>"
            ),
            "{html}"
        );
    }
}

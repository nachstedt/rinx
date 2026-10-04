use serde::{Deserialize, Serialize};

use crate::TargetName;

/// Where a hyperlink, or a hyperlink target, leads.
///
/// docutils reads the text inside `<…>` of an embedded reference, and after
/// the colon of a target, the same way: ending in an unescaped underscore it
/// names another target (an *alias*, or *indirect* target), otherwise it is a
/// URI. The parser decides which once, while it can still see the escapes, so
/// no later phase has to guess from the string — a relative `page.html` or
/// `#anchor` is as much a URI as `https://…` is.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum LinkDestination {
    /// A URI, used as written: absolute, relative or a `#fragment`.
    Uri(String),
    /// Another target of the same document, by name.
    Name(TargetName),
}

/// What a named hyperlink reference (`` `text`_ ``) points at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HyperlinkTarget {
    /// `name_` or `` `name`_ ``: whatever target the document gives this name.
    Reference(TargetName),
    /// `` `text <destination>`_ ``: the destination written in the reference.
    /// Its text also becomes a target of the document, as in docutils, so a
    /// later `` `text`_ `` reaches the same destination.
    Embedded(LinkDestination),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hyperlink_target_round_trips_through_json() {
        // Given
        let targets = [
            HyperlinkTarget::Reference(TargetName::new("Python")),
            HyperlinkTarget::Embedded(LinkDestination::Uri("#usage".to_string())),
            HyperlinkTarget::Embedded(LinkDestination::Name(TargetName::new("Python"))),
        ];

        // When / Then
        for target in targets {
            let json = serde_json::to_string(&target).expect("serializes");
            assert_eq!(
                serde_json::from_str::<HyperlinkTarget>(&json).expect("deserializes"),
                target
            );
        }
    }
}

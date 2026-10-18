//! What a conversion could not carry over.
//!
//! The conversion is deliberately lossy in one direction only: anything
//! sphinx-needs expresses that the entity model cannot is listed here rather
//! than dropped. Each entry is a question about the entity model, not a
//! defect in the project.

use serde::Serialize;

/// The kind of construct an entry is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum ReportCategory {
    /// A link whose targets a dynamic function computes.
    #[serde(rename = "link predicates")]
    LinkPredicates,
    /// A link copied onto the needs it points at.
    #[serde(rename = "link copy")]
    LinkCopy,
    /// A `schemas.json` rule selecting on a condition rather than a type.
    #[serde(rename = "conditional schema rule")]
    ConditionalSchemaRule,
    /// A `schemas.json` rule constraining linked needs.
    #[serde(rename = "network schema rule")]
    NetworkSchemaRule,
    /// A `schemas.json` rule on a value that cannot be carried onto its type.
    #[serde(rename = "value schema rule")]
    ValueSchemaRule,
    /// `needs_constraints`.
    #[serde(rename = "constraints")]
    Constraints,
    /// `needs_variants` and variant syntax.
    #[serde(rename = "variants")]
    Variants,
    /// `needs_global_options`.
    #[serde(rename = "global options")]
    GlobalOptions,
    /// sphinx-needs' own `.. need::`, typed by an option.
    #[serde(rename = "generic need directive")]
    GenericNeedDirective,
}

impl ReportCategory {
    /// The category's name, as the benchmark's report groups by it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LinkPredicates => "link predicates",
            Self::LinkCopy => "link copy",
            Self::ConditionalSchemaRule => "conditional schema rule",
            Self::NetworkSchemaRule => "network schema rule",
            Self::ValueSchemaRule => "value schema rule",
            Self::Constraints => "constraints",
            Self::Variants => "variants",
            Self::GlobalOptions => "global options",
            Self::GenericNeedDirective => "generic need directive",
        }
    }
}

/// One construct that could not be carried over, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReportEntry {
    pub category: ReportCategory,
    pub detail: String,
}

impl ReportEntry {
    #[must_use]
    pub fn new(category: ReportCategory, detail: impl Into<String>) -> Self {
        Self {
            category,
            detail: detail.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_as_str_is_the_name_the_entry_serializes_under() {
        // Given every category
        let categories = [
            ReportCategory::LinkPredicates,
            ReportCategory::LinkCopy,
            ReportCategory::ConditionalSchemaRule,
            ReportCategory::NetworkSchemaRule,
            ReportCategory::ValueSchemaRule,
            ReportCategory::Constraints,
            ReportCategory::Variants,
            ReportCategory::GlobalOptions,
            ReportCategory::GenericNeedDirective,
        ];

        for category in categories {
            // When
            let serialized = serde_json::to_value(category).expect("serializable");

            // Then
            assert_eq!(serialized, category.as_str());
        }
    }

    #[test]
    fn test_an_entry_serializes_as_category_and_detail() {
        // Given
        let entry = ReportEntry::new(ReportCategory::LinkCopy, "[needs.links.a] sets copy = true");

        // When
        let serialized = serde_json::to_value(&entry).expect("serializable");

        // Then
        assert_eq!(
            serialized,
            serde_json::json!({"category": "link copy", "detail": "[needs.links.a] sets copy = true"})
        );
    }
}

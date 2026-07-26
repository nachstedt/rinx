use serde::{Deserialize, Serialize};

/// One entry in the site-wide general index (`genindex.html`), sourced
/// either from a `.. index::` directive or automatically from a domain
/// object definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenIndexEntry {
    /// The main, alphabetized term (e.g. `"execution"`, `"Greeter.greet (method)"`).
    pub primary: String,
    /// An optional nested sub-term (e.g. `"context"` in `single: execution; context`).
    pub subentry: Option<String>,
    /// Whether this occurrence should be emphasized as the entry's primary
    /// definition (from a leading `!` in a `.. index::` entry).
    pub main: bool,
    /// The document this entry's anchor lives on.
    pub doc_path: String,
    /// The HTML anchor `id` on `doc_path` this entry links to.
    pub anchor: String,
}

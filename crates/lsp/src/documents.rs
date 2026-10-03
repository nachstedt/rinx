//! The text of every document open in the editor.
//!
//! Kept in memory because an open buffer is the truth: what the author sees,
//! saved or not, is what gets diagnosed. Synchronisation is whole-document
//! (`TextDocumentSyncKind::FULL`), so a change simply replaces the text.

use lsp_types::Uri;
use std::collections::HashMap;

/// One open document: its latest text and the version the client gave it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenDocument {
    pub version: i32,
    pub text: String,
}

/// The documents currently open, by URI.
#[derive(Debug, Default)]
pub struct DocumentStore {
    documents: HashMap<Uri, OpenDocument>,
}

impl DocumentStore {
    /// Records `uri` as open with `text` at `version`.
    pub fn open(&mut self, uri: Uri, version: i32, text: String) {
        self.documents.insert(uri, OpenDocument { version, text });
    }

    /// Replaces the text of `uri` with `text` at `version`.
    ///
    /// A change for a document never opened is recorded as an open, rather
    /// than dropped: the client is the authority on what is open, and losing
    /// its text would leave the document undiagnosed until it is reopened.
    pub fn replace(&mut self, uri: Uri, version: i32, text: String) {
        self.open(uri, version, text);
    }

    /// Forgets `uri`.
    pub fn close(&mut self, uri: &Uri) {
        self.documents.remove(uri);
    }

    /// The open document at `uri`, if any.
    #[must_use]
    pub fn get(&self, uri: &Uri) -> Option<&OpenDocument> {
        self.documents.get(uri)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uri() -> Uri {
        "file:///docs/index.rst".parse().expect("valid uri")
    }

    #[test]
    fn test_open_records_text_and_version() {
        // Given
        let mut store = DocumentStore::default();

        // When
        store.open(uri(), 1, "Title\n".to_string());

        // Then
        assert_eq!(
            store.get(&uri()),
            Some(&OpenDocument {
                version: 1,
                text: "Title\n".to_string()
            })
        );
    }

    #[test]
    fn test_replace_overwrites_the_text() {
        // Given
        let mut store = DocumentStore::default();
        store.open(uri(), 1, "old\n".to_string());

        // When
        store.replace(uri(), 2, "new\n".to_string());

        // Then
        let document = store.get(&uri()).expect("still open");
        assert_eq!((document.version, document.text.as_str()), (2, "new\n"));
    }

    #[test]
    fn test_replace_of_an_unopened_document_opens_it() {
        // Given
        let mut store = DocumentStore::default();

        // When
        store.replace(uri(), 3, "text\n".to_string());

        // Then
        assert_eq!(store.get(&uri()).map(|document| document.version), Some(3));
    }

    #[test]
    fn test_close_forgets_the_document() {
        // Given
        let mut store = DocumentStore::default();
        store.open(uri(), 1, "text\n".to_string());

        // When
        store.close(&uri());

        // Then
        assert_eq!(store.get(&uri()), None);
    }
}

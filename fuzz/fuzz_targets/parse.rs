//! Any text through the parser: it must never panic, since a document being
//! typed in the editor is incomplete by definition (the parser's resilience
//! promise, see CLAUDE.md).

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|text: &str| {
    let _ = rinx_parser::parse("fuzz.rst", text);
});

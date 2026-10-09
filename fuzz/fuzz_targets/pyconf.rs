//! Any text through the `conf.py` reader: it must never panic, since the
//! language server reads whatever a project's `conf.py` holds, mid-edit
//! included.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|text: &str| {
    let _ = rinx_pyconf::read_module(text);
});

//! Rusty Sphinx Application Entry Point
//!
//! This is the main binary which wraps the core functionality
//! provided by the rusty-sphinx library.

use rusty_sphinx::greeting;

fn main() {
    let msg = greeting("World");
    println!("{msg}");
}

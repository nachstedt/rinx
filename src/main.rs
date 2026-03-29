//! Rusty Sphinx Application Entry Point
//!
//! This is the main binary which wraps the core functionality
//! provided by the rusty-sphinx library.

use rusty_sphinx::process_rst;
use std::env;
use std::fs;
use std::process;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <input.rst>", args[0]);
        process::exit(1);
    }

    let input_path = &args[1];
    let input = fs::read_to_string(input_path).unwrap_or_else(|err| {
        eprintln!("Error reading file '{}': {}", input_path, err);
        process::exit(1);
    });

    let html = process_rst(&input);
    println!("{html}");
}

//! Fuzz target for Vyomafile parser

#![no_main]

use libfuzzer_sys::fuzz_target;
use vyoma_build::parser::Vyomafile;

/// Fuzz the Vyomafile content parser
///
/// This target fuzzes the Vyomafile parser to uncover crashes or
/// panics in the parsing logic when given malformed Vyomafile content.
fuzz_target!(|data: &[u8]| {
    // Convert input to string, replacing invalid UTF-8 with replacement characters
    let content = String::from_utf8_lossy(data);

    // Try to parse the content
    let _ = Vyomafile::parse_content(&content);
});

/// Fuzz individual Vyomafile instruction parsing
///
/// This target specifically fuzzes the line-by-line parsing to find
/// edge cases in instruction handling.
fuzz_target!(|data: &[u8]| {
    let content = String::from_utf8_lossy(data);

    // Split into lines and try to parse each as a single instruction
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Try to parse as individual instruction by forcing it through parse_content
        // The parser will only parse valid instructions, but we're testing edge cases
        let test_content = line.to_string();
        let _ = Vyomafile::parse_content(&test_content);
    }
});

/// Fuzz Vyomafile with various instruction combinations
///
/// This target tests various combinations of valid and invalid instructions.
fuzz_target!(|data: &[u8]| {
    let content = String::from_utf8_lossy(data);

    // Try different variations:
    // 1. Original content
    let _ = Vyomafile::parse_content(&content);

    // 2. Content with newlines
    let with_newlines = content.replace(" ", "\n");
    let _ = Vyomafile::parse_content(&with_newlines);

    // 3. Content with tabs
    let with_tabs = content.replace(" ", "\t");
    let _ = Vyomafile::parse_content(&with_tabs);
});
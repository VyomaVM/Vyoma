//! Vyoma Chaos Tests
//!
//! This file exists to document how to run the chaos tests.
//! The actual tests are implemented in crates/vyomad/src/chaos_tests.rs
//! and are compiled when the `chaos` feature is enabled.
//!
//! Run with: cargo test -p vyomad --features chaos --test chaos -- --ignored
//!
//! For integration testing, build the daemon with chaos feature:
//!   cargo build -p vyomad --features chaos
//!
//! Then run the test binary:
//!   ./target/debug/deps/vyomad-<hash> --ignored

fn main() {
    println!("This file is a placeholder.");
    println!("Run: cargo test -p vyomad --features chaos -- --ignored");
}
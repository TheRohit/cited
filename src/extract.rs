//! Pulling witnesses out of prose. A review bot posts its witness inside a
//! fenced block so a human sees the claim and a checker sees the evidence:
//!
//! ```text
//! ```witness
//! { "witness": 1, ... }
//! ```
//! ```

use regex::Regex;
use std::sync::LazyLock;

// Compiled once, on first use, shared across calls.
static BLOCK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)```witness\s*\n(.*?)\n```").unwrap());

/// Each block as parsed JSON. A block that is not JSON comes back as
/// `Err(raw_text)` so the caller can report it in place.
pub fn witness_blocks(markdown: &str) -> Vec<Result<serde_json::Value, String>> {
    BLOCK
        .captures_iter(markdown)
        .map(|c| {
            let raw = c.get(1).map_or("", |m| m.as_str());
            serde_json::from_str(raw).map_err(|e| format!("{e}: {raw}"))
        })
        .collect()
}

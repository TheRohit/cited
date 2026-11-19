//! Human-readable report. `fmt::Write` into a String rather than println!,
//! so the CLI and tests see the same text.

use crate::check::{Report, Verdict};
use std::fmt::Write;

pub fn report(r: &Report) -> String {
    let mut s = String::new();
    let mark = match r.verdict {
        Verdict::Verified => "✓",
        Verdict::Cited => "~",
        Verdict::Invalid => "✗",
    };
    let verdict = format!("{:?}", r.verdict).to_uppercase();
    let title = r.title.as_deref().unwrap_or("");
    let _ = writeln!(s, "{mark} {verdict}  {title}");
    let _ = writeln!(
        s,
        "  {} quote(s) re-read, {} edge(s) verified",
        r.quotes_passed, r.edges_passed
    );
    for p in &r.problems {
        let bullet = if p.code.is_note() { "·" } else { "✗" };
        let _ = writeln!(s, "  {bullet} {}: {}", p.at, p.message);
        if let Some(actual) = &p.actual {
            let _ = writeln!(s, "    actual:");
            let lines: Vec<&str> = actual.lines().collect();
            for l in lines.iter().take(6) {
                let _ = writeln!(s, "      │ {l}");
            }
            if lines.len() > 6 {
                let _ = writeln!(s, "      │ …");
            }
        }
    }
    s
}

//! The witness format.
//!
//! A finding may be posted only if it carries a witness: one concrete path
//! through the source, every step cited by locator, lines and a byte-exact
//! quote, ending in an observed behaviour and the expectation it violates.
//! The expectation names its source. "The model thinks so" is not a source.
//!
//! Everything here is checkable without a model. See [`crate::check`].

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// 1-based, inclusive line range. Serialized as `[start, end]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineRange(pub u32, pub u32);

/// A location in a pinned snapshot that must re-read byte-identical.
///
/// `file` is a locator relative to the snapshot. For multi-repo witnesses it
/// carries a `name:` prefix selecting the pin in [`Witness::repos`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Citation {
    pub file: String,
    pub lines: LineRange,
    /// Byte-exact substring of the file text within `lines`.
    pub quote: String,
}

/// How step N+1 is reached from step N. Optional: a witness without edges is
/// `cited`, not `verified`. The built-in verifier is syntactic; a stronger one
/// (program model, type checker) can be plugged in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Edge {
    pub kind: EdgeKind,
    /// Free text, e.g. the callee as written at the call site.
    pub via: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EdgeKind {
    Call,
    Route,
    Filter,
    Callback,
    Enqueue,
    Import,
    Match,
    Other,
}

/// One step on the path. Flattens a [`Citation`] (`file`, `lines`, `quote`)
/// into the same JSON object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Step {
    /// Enclosing symbol in the language's natural spelling (`Foo#bar`, `pkg::fn`).
    pub symbol: String,
    #[serde(flatten)]
    pub at: Citation,
    /// What executes or changes here. One sentence.
    pub effect: String,
    /// How this step is reached from the previous one. Absent on the first step.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edge: Option<Edge>,
}

/// Where an expectation comes from. Never "the model".
///
/// `#[serde(tag = "kind")]` makes this a discriminated union on the wire:
/// `{ "kind": "sibling", "symbol": ..., "file": ..., ... }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ExpectationSource {
    /// Modified code: the base revision did otherwise. Read at the `base` pin.
    BaseBehavior {
        #[serde(flatten)]
        at: Citation,
    },
    /// Another symbol in the same codebase follows the convention this one breaks.
    Sibling {
        symbol: String,
        #[serde(flatten)]
        at: Citation,
    },
    /// A test asserts the expected behaviour.
    Test {
        #[serde(flatten)]
        at: Citation,
    },
    /// Documentation states it.
    Doc {
        #[serde(flatten)]
        at: Citation,
    },
    /// A consumer, possibly in another repo, relies on it.
    Consumer {
        symbol: String,
        #[serde(flatten)]
        at: Citation,
    },
    /// The PR's own stated intent. Not re-readable; reported as `unchecked`.
    PrIntent { r#ref: String, statement: String },
    /// A general law of the language or framework, named by rule. Not re-readable.
    Pattern { rule: String, statement: String },
}

impl ExpectationSource {
    /// The citation to re-read, and the default pin it reads at, if any.
    pub fn citation(&self) -> Option<(&Citation, &'static str)> {
        match self {
            Self::BaseBehavior { at } => Some((at, "base")),
            Self::Sibling { at, .. } | Self::Test { at } | Self::Doc { at } | Self::Consumer { at, .. } => {
                Some((at, "head"))
            }
            Self::PrIntent { .. } | Self::Pattern { .. } => None,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Self::BaseBehavior { .. } => "base-behavior",
            Self::Sibling { .. } => "sibling",
            Self::Test { .. } => "test",
            Self::Doc { .. } => "doc",
            Self::Consumer { .. } => "consumer",
            Self::PrIntent { .. } => "pr-intent",
            Self::Pattern { .. } => "pattern",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Expected {
    pub statement: String,
    pub source: ExpectationSource,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supporting: Vec<ExpectationSource>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Witness {
    /// Format version. Must be 1.
    pub witness: u32,
    pub title: String,
    /// Pins by name. `head` is required; `base` only by `base-behavior`
    /// sources; any other name is a repo a citation selects with `name:`.
    /// `BTreeMap` rather than `HashMap` so serialization order is stable.
    pub repos: BTreeMap<String, String>,
    /// State required for the path to run.
    pub precondition: String,
    /// The request, call or event that starts the path.
    pub input: String,
    /// Ordered. Every step cited.
    pub path: Vec<Step>,
    /// What the user or consumer gets. Severity comes from here.
    pub observed: String,
    pub expected: Expected,
}

/// What serde cannot express: value constraints. Returned as `(path, message)`.
pub fn validate(w: &Witness) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if w.witness != 1 {
        out.push(("witness".into(), format!("unsupported version {}", w.witness)));
    }
    if !w.repos.contains_key("head") {
        out.push(("repos".into(), "`head` pin is required".into()));
    }
    for (name, sha) in &w.repos {
        if sha.len() < 7 {
            out.push((format!("repos.{name}"), "pin must be at least 7 characters".into()));
        }
    }
    if w.path.is_empty() {
        out.push(("path".into(), "path must have at least one step".into()));
    }
    let non_empty: [(&str, &str); 5] = [
        ("title", &w.title),
        ("precondition", &w.precondition),
        ("input", &w.input),
        ("observed", &w.observed),
        ("expected.statement", &w.expected.statement),
    ];
    for (at, v) in non_empty {
        if v.trim().is_empty() {
            out.push((at.into(), "must not be empty".into()));
        }
    }
    for (i, s) in w.path.iter().enumerate() {
        check_citation(&format!("path[{i}]"), &s.at, &mut out);
        if s.symbol.trim().is_empty() {
            out.push((format!("path[{i}].symbol"), "must not be empty".into()));
        }
        if s.effect.trim().is_empty() {
            out.push((format!("path[{i}].effect"), "must not be empty".into()));
        }
    }
    if let Some((c, _)) = w.expected.source.citation() {
        check_citation("expected.source", c, &mut out);
    }
    for (i, s) in w.expected.supporting.iter().enumerate() {
        if let Some((c, _)) = s.citation() {
            check_citation(&format!("expected.supporting[{i}]"), c, &mut out);
        }
    }
    out
}

fn check_citation(at: &str, c: &Citation, out: &mut Vec<(String, String)>) {
    if c.lines.0 == 0 || c.lines.0 > c.lines.1 {
        out.push((format!("{at}.lines"), "lines must be 1-based and start <= end".into()));
    }
    if c.quote.is_empty() {
        out.push((format!("{at}.quote"), "quote must not be empty".into()));
    }
    if c.file.is_empty() {
        out.push((format!("{at}.file"), "file must not be empty".into()));
    }
}

/// Split `repo:path` into `(repo, path)`. A bare path gets `default_repo`.
/// A repo name never contains `/` or `.`, which keeps `src/a.rs` and
/// Windows drive letters out of the prefix.
pub fn split_file<'a>(file: &'a str, default_repo: &'a str) -> (&'a str, &'a str) {
    if let Some(i) = file.find(':') {
        let (head, rest) = (&file[..i], &file[i + 1..]);
        if i > 0 && !head.contains('/') && !head.contains('.') {
            return (head, rest);
        }
    }
    (default_repo, file)
}

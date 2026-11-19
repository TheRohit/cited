//! The checks. None of them call a model.
//!
//! 1. Schema: the document parses and validates as a witness.
//! 2. Pins: every repo a citation names has a SHA.
//! 3. Quotes: every citation's file exists at its pin and `quote` occurs
//!    byte-identical inside `lines`.
//! 4. Expectation: the primary source re-reads the same way, or is
//!    `unchecked` for sources that are not text.
//! 5. Edges: step N+1 is reached from step N. The built-in verifier is
//!    syntactic. A missing edge is `unverified`, never a failure.
//!
//! Verdicts:
//! - `verified`: every check passed, every edge verified
//! - `cited`: every quote re-read; some edge unverified or absent
//! - `invalid`: a quote, pin, edge or the schema failed

use crate::schema::{Citation, Edge, EdgeKind, LineRange, Step, Witness, split_file, validate};
use crate::source::Source;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    Verified,
    Cited,
    Invalid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Code {
    Schema,
    UnpinnedRepo,
    FileMissing,
    LinesOutOfRange,
    QuoteMismatch,
    EdgeUnverified,
    EdgeFailed,
    SourceUnchecked,
}

impl Code {
    /// Notes explain a weaker verdict; failures make the witness invalid.
    pub fn is_note(self) -> bool {
        matches!(self, Code::EdgeUnverified | Code::SourceUnchecked)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Problem {
    /// Dot path into the witness, e.g. `path[2].quote`.
    pub at: String,
    pub code: Code,
    pub message: String,
    /// What the file actually says in `lines`, when relevant.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub verdict: Verdict,
    pub title: Option<String>,
    pub problems: Vec<Problem>,
    pub quotes_passed: usize,
    pub edges_passed: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeVerdict {
    Verified,
    Unverified,
    Failed,
}

/// Decides whether `to` is really reached from `from` by `edge`. The built-in
/// one is [`Syntactic`]; a program model or type checker can replace it.
pub trait EdgeVerifier {
    fn name(&self) -> &str;
    fn verify(&self, from: &Step, to: &Step, edge: &Edge, source: &dyn Source) -> (EdgeVerdict, String);
}

/// Purely textual, no parsing. Honest about its limits: route and match
/// edges have no callee name to compare, so they stay `unverified`.
pub struct Syntactic;

impl EdgeVerifier for Syntactic {
    fn name(&self) -> &str {
        "syntactic"
    }

    fn verify(&self, from: &Step, to: &Step, edge: &Edge, _: &dyn Source) -> (EdgeVerdict, String) {
        if !from.at.quote.contains(&edge.via) {
            return (
                EdgeVerdict::Failed,
                format!("\"{}\" does not appear in the previous step's quote", edge.via),
            );
        }
        if matches!(edge.kind, EdgeKind::Route | EdgeKind::Match | EdgeKind::Other) {
            return (
                EdgeVerdict::Unverified,
                format!("{} edges need a program-model verifier", kind_name(edge.kind)),
            );
        }
        let last = last_identifier(&edge.via);
        if !to.symbol.contains(last) {
            return (
                EdgeVerdict::Failed,
                format!("\"{last}\" is not part of the next step's symbol \"{}\"", to.symbol),
            );
        }
        (EdgeVerdict::Verified, String::new())
    }
}

fn kind_name(k: EdgeKind) -> &'static str {
    match k {
        EdgeKind::Call => "call",
        EdgeKind::Route => "route",
        EdgeKind::Filter => "filter",
        EdgeKind::Callback => "callback",
        EdgeKind::Enqueue => "enqueue",
        EdgeKind::Import => "import",
        EdgeKind::Match => "match",
        EdgeKind::Other => "other",
    }
}

/// Last identifier in `via`, ignoring a trailing `(`: `team.assign_roles(` → `assign_roles`.
fn last_identifier(via: &str) -> &str {
    let trimmed = via.trim_end().trim_end_matches('(').trim_end();
    let start = trimmed
        .rfind(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$'))
        .map(|i| i + 1)
        .unwrap_or(0);
    let id = &trimmed[start..];
    if id.is_empty() { via } else { id }
}

pub struct Checker<'a> {
    pub source: &'a dyn Source,
    pub edges: &'a dyn EdgeVerifier,
}

impl<'a> Checker<'a> {
    pub fn new(source: &'a dyn Source) -> Self {
        Self {
            source,
            edges: &Syntactic,
        }
    }

    pub fn with_edges(mut self, edges: &'a dyn EdgeVerifier) -> Self {
        self.edges = edges;
        self
    }

    /// Check a JSON value. Schema failures come back as `invalid` with one
    /// problem per issue, so a caller never has to parse first.
    pub fn check_value(&self, value: &serde_json::Value) -> Report {
        match serde_json::from_value::<Witness>(value.clone()) {
            Ok(w) => self.check(&w),
            Err(e) => Report {
                verdict: Verdict::Invalid,
                title: value.get("title").and_then(|t| t.as_str()).map(String::from),
                problems: vec![Problem {
                    at: "$".into(),
                    code: Code::Schema,
                    message: e.to_string(),
                    actual: None,
                }],
                quotes_passed: 0,
                edges_passed: 0,
            },
        }
    }

    pub fn check(&self, w: &Witness) -> Report {
        let mut problems = Vec::new();
        for (at, message) in validate(w) {
            problems.push(Problem {
                at,
                code: Code::Schema,
                message,
                actual: None,
            });
        }
        if !problems.is_empty() {
            return Report {
                verdict: Verdict::Invalid,
                title: Some(w.title.clone()),
                problems,
                quotes_passed: 0,
                edges_passed: 0,
            };
        }

        let mut quotes = 0;
        let mut edges = 0;
        let mut invalid = false;
        let mut all_edges = true;

        for (i, step) in w.path.iter().enumerate() {
            if self.citation(w, &step.at, &format!("path[{i}]"), "head", &mut problems) {
                quotes += 1;
            } else {
                invalid = true;
            }
        }

        let sources = std::iter::once((&w.expected.source, "expected.source".to_string(), true)).chain(
            w.expected
                .supporting
                .iter()
                .enumerate()
                .map(|(i, s)| (s, format!("expected.supporting[{i}]"), false)),
        );
        for (s, at, primary) in sources {
            match s.citation() {
                None => problems.push(Problem {
                    at,
                    code: Code::SourceUnchecked,
                    message: format!("{} sources are not text and are not re-read", s.kind()),
                    actual: None,
                }),
                Some((c, default_repo)) => {
                    if self.citation(w, c, &at, default_repo, &mut problems) {
                        quotes += 1;
                    } else if primary {
                        invalid = true; // a bad supporting source is reported, not fatal
                    }
                }
            }
        }

        // `windows(2)` yields overlapping pairs: [s0,s1], [s1,s2], ...
        for (i, pair) in w.path.windows(2).enumerate() {
            let (from, to) = (&pair[0], &pair[1]);
            let at = format!("path[{}].edge", i + 1);
            let Some(edge) = &to.edge else {
                all_edges = false;
                problems.push(Problem {
                    at,
                    code: Code::EdgeUnverified,
                    message: "no edge given; how is this step reached from the previous one?".into(),
                    actual: None,
                });
                continue;
            };
            let (v, msg) = self.edges.verify(from, to, edge, self.source);
            let name = self.edges.name();
            match v {
                EdgeVerdict::Verified => edges += 1,
                EdgeVerdict::Unverified => {
                    all_edges = false;
                    problems.push(Problem {
                        at,
                        code: Code::EdgeUnverified,
                        message: format!("{name}: {msg}"),
                        actual: None,
                    });
                }
                EdgeVerdict::Failed => {
                    invalid = true;
                    problems.push(Problem {
                        at,
                        code: Code::EdgeFailed,
                        message: format!("{name}: {msg}"),
                        actual: None,
                    });
                }
            }
        }

        let verdict = if invalid {
            Verdict::Invalid
        } else if all_edges {
            Verdict::Verified
        } else {
            Verdict::Cited
        };
        Report {
            verdict,
            title: Some(w.title.clone()),
            problems,
            quotes_passed: quotes,
            edges_passed: edges,
        }
    }

    /// Re-read one citation. Pushes a problem and returns false on any miss.
    fn citation(&self, w: &Witness, c: &Citation, at: &str, default_repo: &str, problems: &mut Vec<Problem>) -> bool {
        let (repo, path) = split_file(&c.file, default_repo);
        let Some(pin) = w.repos.get(repo) else {
            problems.push(Problem {
                at: format!("{at}.file"),
                code: Code::UnpinnedRepo,
                message: format!("repo \"{repo}\" has no pin in `repos`"),
                actual: None,
            });
            return false;
        };
        let short = &pin[..pin.len().min(10)];
        let Some(text) = self.source.read(repo, pin, path) else {
            problems.push(Problem {
                at: format!("{at}.file"),
                code: Code::FileMissing,
                message: format!("{path} does not exist at {repo}@{short}"),
                actual: None,
            });
            return false;
        };
        let lines: Vec<&str> = text.split('\n').collect();
        let LineRange(a, b) = c.lines;
        if b as usize > lines.len() {
            problems.push(Problem {
                at: format!("{at}.lines"),
                code: Code::LinesOutOfRange,
                message: format!("file has {} lines, citation ends at {b}", lines.len()),
                actual: None,
            });
            return false;
        }
        let region = lines[(a - 1) as usize..b as usize].join("\n");
        if !region.contains(&c.quote) {
            problems.push(Problem {
                at: format!("{at}.quote"),
                code: Code::QuoteMismatch,
                message: format!("quote does not occur in {path}:{a}-{b} at {repo}@{short}"),
                actual: Some(region),
            });
            return false;
        }
        true
    }
}

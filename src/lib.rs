//! `cited`: machine-checkable evidence for AI findings.
//!
//! A finding is a witness: a path through pinned source where every quote
//! must re-read byte-identical. The checker needs no model.

pub mod check;
pub mod extract;
pub mod format;
pub mod schema;
pub mod source;

pub use check::{Checker, EdgeVerdict, EdgeVerifier, Problem, Report, Syntactic, Verdict};
pub use schema::{Citation, Edge, EdgeKind, ExpectationSource, Expected, LineRange, Step, Witness};
pub use source::{GitSource, MemorySource, Source};

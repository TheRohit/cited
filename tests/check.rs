mod common;

use cited::check::{Code, EdgeVerdict, EdgeVerifier};
use cited::{Checker, GitSource, MemorySource, Source, Verdict};
use common::{good_witness, make_repo};
use std::collections::HashMap;

fn codes(r: &cited::Report) -> Vec<Code> {
    r.problems.iter().map(|p| p.code).collect()
}

#[test]
fn good_witness_is_verified() {
    let repo = make_repo();
    let source = GitSource::single(repo.dir.path());
    let r = Checker::new(&source).check_value(&good_witness(&repo.base, &repo.head));
    assert!(r.problems.is_empty(), "{:?}", r.problems);
    assert_eq!(r.verdict, Verdict::Verified);
    assert_eq!((r.quotes_passed, r.edges_passed), (3, 1));
}

#[test]
fn drifted_quote_is_invalid_and_shows_actual() {
    let repo = make_repo();
    let source = GitSource::single(repo.dir.path());
    let mut w = good_witness(&repo.base, &repo.head);
    // True at base, not at head.
    w["path"][0]["quote"] = "team.save!\n    render json: team".into();
    let r = Checker::new(&source).check_value(&w);
    assert_eq!(r.verdict, Verdict::Invalid);
    let p = r.problems.iter().find(|p| p.code == Code::QuoteMismatch).unwrap();
    assert_eq!(p.at, "path[0].quote");
    assert!(p.actual.as_deref().unwrap().contains("team.assign_roles"));
}

#[test]
fn lines_past_eof_are_invalid() {
    let repo = make_repo();
    let source = GitSource::single(repo.dir.path());
    let mut w = good_witness(&repo.base, &repo.head);
    w["path"][1]["lines"] = serde_json::json!([2, 40]);
    let r = Checker::new(&source).check_value(&w);
    assert_eq!(r.verdict, Verdict::Invalid);
    assert_eq!(codes(&r)[0], Code::LinesOutOfRange);
}

#[test]
fn missing_file_is_invalid() {
    let repo = make_repo();
    let source = GitSource::single(repo.dir.path());
    let mut w = good_witness(&repo.base, &repo.head);
    w["path"][1]["file"] = "app/models/nope.rb".into();
    let r = Checker::new(&source).check_value(&w);
    assert_eq!(codes(&r)[0], Code::FileMissing);
}

#[test]
fn base_behavior_needs_a_base_pin() {
    let repo = make_repo();
    let source = GitSource::single(repo.dir.path());
    let mut w = good_witness(&repo.base, &repo.head);
    w["repos"].as_object_mut().unwrap().remove("base");
    let r = Checker::new(&source).check_value(&w);
    assert_eq!(r.verdict, Verdict::Invalid);
    assert_eq!(r.problems[0].at, "expected.source.file");
    assert_eq!(r.problems[0].code, Code::UnpinnedRepo);
}

#[test]
fn no_edge_is_cited_not_verified() {
    let repo = make_repo();
    let source = GitSource::single(repo.dir.path());
    let mut w = good_witness(&repo.base, &repo.head);
    w["path"][1].as_object_mut().unwrap().remove("edge");
    let r = Checker::new(&source).check_value(&w);
    assert_eq!(r.verdict, Verdict::Cited);
    assert_eq!(codes(&r), vec![Code::EdgeUnverified]);
}

#[test]
fn edge_whose_callee_is_absent_fails() {
    let repo = make_repo();
    let source = GitSource::single(repo.dir.path());
    let mut w = good_witness(&repo.base, &repo.head);
    w["path"][1]["edge"] = serde_json::json!({ "kind": "call", "via": "team.destroy(" });
    let r = Checker::new(&source).check_value(&w);
    assert_eq!(r.verdict, Verdict::Invalid);
    assert_eq!(codes(&r), vec![Code::EdgeFailed]);
}

#[test]
fn pattern_source_is_unchecked_not_failed() {
    let repo = make_repo();
    let source = GitSource::single(repo.dir.path());
    let mut w = good_witness(&repo.base, &repo.head);
    w["expected"]["source"] = serde_json::json!({
        "kind": "pattern", "rule": "rails/persist-after-assign", "statement": "assignment is followed by save"
    });
    let r = Checker::new(&source).check_value(&w);
    assert_eq!(r.verdict, Verdict::Verified);
    assert_eq!(codes(&r), vec![Code::SourceUnchecked]);
}

#[test]
fn schema_errors_are_invalid() {
    let source = MemorySource(HashMap::new());
    let r = Checker::new(&source).check_value(&serde_json::json!({ "witness": 1, "title": "x" }));
    assert_eq!(r.verdict, Verdict::Invalid);
    assert_eq!(codes(&r), vec![Code::Schema]);
    assert_eq!(r.title.as_deref(), Some("x"));
}

#[test]
fn value_errors_are_reported_with_paths() {
    let repo = make_repo();
    let source = GitSource::single(repo.dir.path());
    let mut w = good_witness(&repo.base, &repo.head);
    w["path"][0]["lines"] = serde_json::json!([9, 4]);
    w["repos"]["head"] = "abc".into();
    let r = Checker::new(&source).check_value(&w);
    let ats: Vec<&str> = r.problems.iter().map(|p| p.at.as_str()).collect();
    assert_eq!(ats, vec!["repos.head", "path[0].lines"]);
}

struct Oracle;
impl EdgeVerifier for Oracle {
    fn name(&self) -> &str {
        "oracle"
    }
    fn verify(&self, _: &cited::Step, _: &cited::Step, _: &cited::Edge, _: &dyn Source) -> (EdgeVerdict, String) {
        (EdgeVerdict::Failed, "no such edge in the program model".into())
    }
}

#[test]
fn plugged_in_edge_verifier_replaces_syntactic() {
    let repo = make_repo();
    let source = GitSource::single(repo.dir.path());
    let r = Checker::new(&source)
        .with_edges(&Oracle)
        .check_value(&good_witness(&repo.base, &repo.head));
    assert_eq!(r.verdict, Verdict::Invalid);
    assert_eq!(r.problems[0].message, "oracle: no such edge in the program model");
}

#[test]
fn multi_repo_prefix_selects_the_pin() {
    let mut files = HashMap::new();
    files.insert("head@aaaaaaaaaa:a.ts".to_string(), "export const x = 1\n".to_string());
    files.insert(
        "consumer@bbbbbbbbbb:b.ts".to_string(),
        "import { x } from 'a'\nuse(x)\n".to_string(),
    );
    let w = serde_json::json!({
        "witness": 1,
        "title": "x stays exported",
        "repos": { "head": "aaaaaaaaaa", "consumer": "bbbbbbbbbb" },
        "precondition": "b imports a",
        "input": "build",
        "path": [{ "symbol": "x", "file": "a.ts", "lines": [1, 1], "quote": "export const x = 1", "effect": "defines x" }],
        "observed": "x is exported",
        "expected": {
            "statement": "x stays exported",
            "source": { "kind": "consumer", "symbol": "use", "file": "consumer:b.ts", "lines": [1, 2], "quote": "import { x } from 'a'" }
        }
    });
    let r = Checker::new(&MemorySource(files)).check_value(&w);
    assert!(r.problems.is_empty(), "{:?}", r.problems);
    assert_eq!(r.verdict, Verdict::Verified);
}

#[test]
fn witness_blocks_come_out_of_markdown() {
    let md = "Found a bug.\n\n```witness\n{\"witness\":1}\n```\n\ntext\n\n```witness\nnot json\n```\n";
    let out = cited::extract::witness_blocks(md);
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].as_ref().unwrap(), &serde_json::json!({ "witness": 1 }));
    assert!(out[1].is_err());
}

#[test]
fn report_is_readable() {
    let repo = make_repo();
    let source = GitSource::single(repo.dir.path());
    let mut w = good_witness(&repo.base, &repo.head);
    w["path"][0]["quote"] = "nope".into();
    let r = Checker::new(&source).check_value(&w);
    let s = cited::format::report(&r);
    assert!(s.contains("✗ INVALID"), "{s}");
    assert!(s.contains("path[0].quote"));
    assert!(s.contains("actual:"));
}

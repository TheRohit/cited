//! `cited check <file...> [--repo name=path] [--json] [--strict]`

use cited::{check::Checker, extract::witness_blocks, format, source::GitSource};
use clap::{Parser, Subcommand};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "cited",
    version,
    about = "A finding is a witness. Re-reads every quote at its pin; no model."
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Check witnesses in JSON files or ```witness blocks in markdown.
    Check {
        /// `.json` (one witness or an array) or `.md` files.
        files: Vec<PathBuf>,
        /// Checkout for a repo name: `--repo head=.` `--repo consumer=../other`. `*` is the fallback.
        #[arg(long, value_name = "NAME=PATH")]
        repo: Vec<String>,
        /// Print reports as JSON.
        #[arg(long)]
        json: bool,
        /// Exit 1 unless every witness is `verified` (default: unless none is `invalid`).
        #[arg(long)]
        strict: bool,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Check {
            files,
            repo,
            json,
            strict,
        } => check(files, repo, json, strict),
    }
}

fn check(files: Vec<PathBuf>, repo: Vec<String>, json: bool, strict: bool) -> Result<(), Box<dyn std::error::Error>> {
    if files.is_empty() {
        return Err("no files given".into());
    }
    let mut checkouts: HashMap<String, PathBuf> = HashMap::new();
    checkouts.insert("*".into(), std::env::current_dir()?);
    for spec in &repo {
        let (name, path) = spec
            .split_once('=')
            .ok_or_else(|| format!("--repo expects NAME=PATH, got \"{spec}\""))?;
        checkouts.insert(name.to_string(), PathBuf::from(path));
    }
    let source = GitSource::new(checkouts);
    let checker = Checker::new(&source);

    let mut reports = Vec::new();
    for f in &files {
        let text = std::fs::read_to_string(f).map_err(|e| format!("{}: {e}", f.display()))?;
        let is_md = f.extension().is_some_and(|e| e == "md");
        let docs: Vec<serde_json::Value> = if is_md {
            witness_blocks(&text)
                .into_iter()
                .map(|r| r.unwrap_or_else(|e| serde_json::json!({ "parse_error": e })))
                .collect()
        } else {
            match serde_json::from_str::<serde_json::Value>(&text)? {
                serde_json::Value::Array(a) => a,
                v => vec![v],
            }
        };
        for d in &docs {
            reports.push(checker.check_value(d));
        }
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&reports)?);
    } else {
        let out: Vec<String> = reports.iter().map(format::report).collect();
        print!("{}", out.join("\n"));
    }

    let bad = reports
        .iter()
        .any(|r| r.verdict == cited::Verdict::Invalid || (strict && r.verdict != cited::Verdict::Verified));
    if bad {
        std::process::exit(1);
    }
    Ok(())
}

# cited

**A finding is a witness.** Machine-checkable evidence for AI findings. Every quote must re-read byte-identical at a pinned snapshot. The checker needs no model.

```
$ cited check review.md --repo head=.
✓ VERIFIED  Team roles are assigned but never persisted
  3 quote(s) re-read, 1 edge(s) verified

✗ INVALID  Null check missing in TeamsController#update
  0 quote(s) re-read, 0 edge(s) verified
  ✗ path[0].quote: quote does not occur in app/controllers/teams_controller.rb:4-8 at head@64c73f816d
    actual:
      │   def update
      │     team = Team.find(params[:id])
      │     team.assign_roles(params[:role_ids])
```

## Why

AI code review posts claims. Nobody can check them cheaply, so they get ignored. Self-audits put the share of "verified" AI findings that do not hold up near half. The fix is not a better model. It is a rule: **a finding may be posted only if it carries a witness**, and the witness is checked in code.

A witness is one concrete path through the source. Every step cites a file, lines and a byte-exact quote at a pinned commit. The path ends in what was *observed* and what was *expected*, and the expectation names its *source*. "The model thinks so" is not a source.

A witness that fails a check is discarded. It is not downgraded to "low confidence".

## Install

```sh
cargo install cited
```

Or build from source: `cargo build --release`, binary at `target/release/cited`.

## Use

```sh
# one witness, or an array, as JSON
cited check findings.json

# ```witness blocks inside markdown (a bot's PR comment)
cited check review.md

# citations pinned in another checkout, or several repos
cited check w.json --repo head=. --repo consumer=../other-service

# CI: exit 1 unless every witness is verified
cited check findings.json --strict

# machine-readable
cited check findings.json --json
```

Exit code is 1 if any witness is `invalid`; with `--strict`, if any is not `verified`.

## The format

```jsonc
{
  "witness": 1,
  "title": "Team roles are assigned but never persisted",
  "repos": { "head": "<sha>", "base": "<sha>" },      // pins; `head` required
  "precondition": "a team exists; the caller is an admin",
  "input": "PATCH /teams/:id with role_ids",
  "path": [
    {
      "symbol": "TeamsController#update",
      "file": "app/controllers/teams_controller.rb",
      "lines": [4, 8],
      "quote": "team.assign_roles(params[:role_ids])\n    render json: team",
      "effect": "assigns roles in memory, then renders without saving"
    },
    {
      "symbol": "Team#assign_roles",
      "file": "app/models/team.rb",
      "lines": [2, 4],
      "quote": "self.role_ids = ids",
      "edge": { "kind": "call", "via": "team.assign_roles(" },   // how this step is reached
      "effect": "mutates role_ids on the in-memory record only"
    }
  ],
  "observed": "the response shows the new roles; the database still has the old ones",
  "expected": {
    "statement": "the controller saves the team after assigning roles",
    "source": {                                        // never "the model"
      "kind": "base-behavior",
      "file": "app/controllers/teams_controller.rb",
      "lines": [6, 7],
      "quote": "team.save!"
    }
  }
}
```

**Expectation sources.** `base-behavior` (the base revision did otherwise; read at the `base` pin), `sibling` (another symbol follows the convention), `test`, `doc`, `consumer` (something, possibly in another repo, relies on it). Each of those is a citation and is re-read. `pr-intent` and `pattern` are not text; they are reported as `unchecked`, not failed.

**Multi-repo.** Add a pin under any name and prefix the citation's file with it: `"file": "consumer:src/client.ts"`.

**Edges.** Optional. `kind` is one of `call`, `route`, `filter`, `callback`, `enqueue`, `import`, `match`, `other`; `via` is the text at the previous step that reaches this one.

## The checks

None of them call a model.

1. The document parses and validates as a witness.
2. Every repo a citation names has a pin.
3. Every citation's file exists at its pin and `quote` occurs byte-identical inside `lines`.
4. The expectation source re-reads the same way.
5. Each step is reached from the previous one by its `edge`.

| verdict | meaning |
| --- | --- |
| `verified` | every quote re-read, every edge verified |
| `cited` | every quote re-read; an edge is missing or could not be verified |
| `invalid` | a quote, pin or edge failed, or the schema did. Discard. |

The built-in edge verifier is **syntactic**: `via` must appear in the previous step's quote, and its last identifier must appear in the next step's symbol. Route and cross-repo `match` edges have no name to compare and stay `unverified`. That is honest, not strong. A program model or a type checker can replace it through the `EdgeVerifier` trait.

## For bot authors

Emit the witness inside your comment:

````markdown
Roles are assigned but never persisted.

```witness
{ "witness": 1, ... }
```
````

A human reads the sentence. `cited check comment.md` reads the block. If it says `invalid`, do not post.

## As a library

```rust
use cited::{Checker, GitSource, Verdict};

let source = GitSource::single(".");
let report = Checker::new(&source).check_value(&json);
assert_eq!(report.verdict, Verdict::Verified);
```

Plug in your own `Source` (anything that returns a file's text at a pin) or `EdgeVerifier`.

## Scope

The mechanism is general: claim, cited spans, re-read at a pinned snapshot, verdict. Code is the first target because the pain is sharpest there. Documents, logs and web snapshots are a `Source` implementation away.

## Roadmap

- GitHub Action: check every bot comment on a PR, label or hide the invalid ones.
- SARIF export, so verified witnesses show up in code scanning.
- Tree-sitter edge verifier for the common languages.
- `cited fix`: given a drifted quote, find where it moved after a rebase.

## License

MIT

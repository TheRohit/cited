//! A real git repo with two commits: base saves the team, head forgot to.

use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

pub const BASE_CONTROLLER: &str = "class TeamsController < ApplicationController
  before_action :require_admin

  def update
    team = Team.find(params[:id])
    team.assign_roles(params[:role_ids])
    team.save!
    render json: team
  end
end
";

pub const HEAD_CONTROLLER: &str = "class TeamsController < ApplicationController
  before_action :require_admin

  def update
    team = Team.find(params[:id])
    team.assign_roles(params[:role_ids])
    render json: team
  end
end
";

pub const MODEL: &str = "class Team < ApplicationRecord
  def assign_roles(ids)
    self.role_ids = ids
  end
end
";

pub struct Repo {
    /// Dropping this deletes the directory.
    pub dir: TempDir,
    pub base: String,
    pub head: String,
}

fn git(cwd: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .output()
        .expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

pub fn make_repo() -> Repo {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    git(p, &["init", "-q"]);
    fs::create_dir_all(p.join("app/controllers")).unwrap();
    fs::create_dir_all(p.join("app/models")).unwrap();
    fs::write(p.join("app/controllers/teams_controller.rb"), BASE_CONTROLLER).unwrap();
    fs::write(p.join("app/models/team.rb"), MODEL).unwrap();
    git(p, &["add", "-A"]);
    git(p, &["commit", "-qm", "base"]);
    let base = git(p, &["rev-parse", "HEAD"]);
    fs::write(p.join("app/controllers/teams_controller.rb"), HEAD_CONTROLLER).unwrap();
    git(p, &["commit", "-qam", "drop save"]);
    let head = git(p, &["rev-parse", "HEAD"]);
    Repo { dir, base, head }
}

/// The witness for that bug, as JSON. Tests mutate it with `serde_json::Value`
/// before parsing, which is how a bot's output would arrive.
pub fn good_witness(base: &str, head: &str) -> serde_json::Value {
    serde_json::json!({
        "witness": 1,
        "title": "Team roles are assigned but never persisted",
        "repos": { "head": head, "base": base },
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
                "edge": { "kind": "call", "via": "team.assign_roles(" },
                "effect": "mutates role_ids on the in-memory record only"
            }
        ],
        "observed": "the response shows the new roles; the database still has the old ones",
        "expected": {
            "statement": "the controller saves the team after assigning roles",
            "source": {
                "kind": "base-behavior",
                "file": "app/controllers/teams_controller.rb",
                "lines": [6, 7],
                "quote": "team.save!"
            }
        }
    })
}

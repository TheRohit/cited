//! Reading text at a pinned snapshot. The checker never reads the working
//! tree: a witness is pinned, and must re-read there.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Anything that can hand back a file's text at a pin. Git is the first
/// implementation; a docs store or an HTTP archive would be others.
pub trait Source {
    /// Full text of `file` at `pin` in `repo`, or `None` if it does not exist there.
    fn read(&self, repo: &str, pin: &str, file: &str) -> Option<String>;
}

/// `git show <sha>:<file>` against local checkouts, keyed by repo name.
/// The `"*"` key is the fallback checkout for any name not listed.
pub struct GitSource {
    checkouts: HashMap<String, PathBuf>,
    // `RefCell` lets `read(&self)` mutate the cache. Borrow rules are checked
    // at runtime instead of compile time; fine for a single-threaded CLI.
    cache: RefCell<HashMap<String, Option<String>>>,
}

impl GitSource {
    pub fn new(checkouts: HashMap<String, PathBuf>) -> Self {
        Self {
            checkouts,
            cache: RefCell::new(HashMap::new()),
        }
    }

    /// Every repo name resolves to `dir`.
    pub fn single(dir: impl AsRef<Path>) -> Self {
        let mut m = HashMap::new();
        m.insert("*".to_string(), dir.as_ref().to_path_buf());
        Self::new(m)
    }

    fn checkout(&self, repo: &str) -> Option<&PathBuf> {
        self.checkouts.get(repo).or_else(|| self.checkouts.get("*"))
    }
}

impl Source for GitSource {
    fn read(&self, repo: &str, pin: &str, file: &str) -> Option<String> {
        let key = format!("{repo}\0{pin}\0{file}");
        if let Some(hit) = self.cache.borrow().get(&key) {
            return hit.clone();
        }
        let cwd = self.checkout(repo)?;
        let out = Command::new("git")
            .args(["show", &format!("{pin}:{file}")])
            .current_dir(cwd)
            .output()
            .ok()?;
        let text = if out.status.success() {
            Some(String::from_utf8_lossy(&out.stdout).into_owned())
        } else {
            None
        };
        self.cache.borrow_mut().insert(key, text.clone());
        text
    }
}

/// In-memory source for tests and for callers that already hold the snapshot.
/// Keys are `repo@pin:file`.
pub struct MemorySource(pub HashMap<String, String>);

impl Source for MemorySource {
    fn read(&self, repo: &str, pin: &str, file: &str) -> Option<String> {
        self.0.get(&format!("{repo}@{pin}:{file}")).cloned()
    }
}

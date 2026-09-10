//! Repository detection for a process's working directory.
//!
//! The naive approach — ask git for the branch at `cwd` — produces confident
//! nonsense, because git walks *up* until it finds any repository. A redis
//! server with `cwd=/opt/homebrew/var/db/redis` gets reported as being on
//! branch `stable`, which is Homebrew's own branch and has nothing to do with
//! redis. So we find the root ourselves and reject roots that are clearly
//! package-manager or cache territory rather than a project someone is editing.

use crate::model::GitInfo;
use std::fs;
use std::path::{Path, PathBuf};

/// Path fragments that mean "this is not the user's project".
const NOT_A_PROJECT: &[&str] = &[
    "/homebrew",
    "/.cargo",
    "/.rustup",
    "/node_modules",
    "/.gradle",
    "/.nvm",
    "/.pyenv",
    "/.rbenv",
    "/.cache",
    "/site-packages",
    "/opt/local",
    "/Library/Caches",
    "/Library/Application Support",
    "/vendor/bundle",
    "/.venv",
];

pub fn detect(cwd: &Path) -> Option<GitInfo> {
    let root = find_root(cwd)?;
    if implausible(&root) {
        return None;
    }
    let branch = read_head(&root)?;
    Some(GitInfo { root, branch })
}

/// First ancestor of `start` (inclusive) containing a `.git` entry.
fn find_root(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .find(|dir| dir.join(".git").exists())
        .map(Path::to_path_buf)
}

fn implausible(root: &Path) -> bool {
    // "/" is never a meaningful project root.
    if root.parent().is_none() {
        return true;
    }
    // A repo rooted at $HOME is a dotfiles repo; attributing every daemon
    // under it to that branch is noise, not signal.
    if let Ok(home) = std::env::var("HOME") {
        if !home.is_empty() && root == Path::new(&home) {
            return true;
        }
    }
    let s = root.to_string_lossy();
    NOT_A_PROJECT.iter().any(|frag| s.contains(frag))
}

/// Branch name from `.git/HEAD`, or a short sha when detached.
///
/// `.git` is a file rather than a directory inside worktrees and submodules,
/// in which case it points at the real git directory.
fn read_head(root: &Path) -> Option<String> {
    let dot_git = root.join(".git");
    let git_dir = if dot_git.is_file() {
        let pointer = fs::read_to_string(&dot_git).ok()?;
        let target = pointer.trim().strip_prefix("gitdir:")?.trim();
        let target = Path::new(target);
        if target.is_absolute() {
            target.to_path_buf()
        } else {
            root.join(target)
        }
    } else {
        dot_git
    };

    let head = fs::read_to_string(git_dir.join("HEAD")).ok()?;
    let head = head.trim();
    match head.strip_prefix("ref: refs/heads/") {
        Some(branch) => Some(branch.to_string()),
        None => head.get(..8).map(|sha| format!("detached@{sha}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_package_manager_roots() {
        assert!(implausible(Path::new("/opt/homebrew")));
        assert!(implausible(Path::new("/opt/homebrew/var/db/redis")));
        assert!(implausible(Path::new("/Users/x/.cargo/registry/src/foo")));
        assert!(implausible(Path::new("/srv/app/node_modules/leftpad")));
        assert!(implausible(Path::new("/")));
    }

    #[test]
    fn accepts_real_project_roots() {
        assert!(!implausible(Path::new("/Users/x/work/api-gateway")));
        assert!(!implausible(Path::new("/srv/app")));
    }
}

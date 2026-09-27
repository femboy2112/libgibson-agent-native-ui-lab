//! Repository handle — a thin, read-only wrapper over git2.

use std::path::{Path, PathBuf};

use git2::Repository;

/// A opened git repository. Read-only by construction: no helper in this
/// module can mutate worktree, index or refs.
pub struct Repo {
    inner: Repository,
    root: PathBuf,
}

pub struct BranchInfo {
    pub name: String,
    pub oid: String,
    pub is_remote: bool,
    pub is_head: bool,
}

pub struct TagInfo {
    pub name: String,
    /// Peeled commit oid (annotated tags resolve to their target).
    pub oid: String,
}

pub struct HeadInfo {
    pub branch: Option<String>,
    pub detached: bool,
    pub oid: String,
}

impl Repo {
    /// Open the repository at `path` (or fail with a readable error).
    pub fn open(path: &Path) -> Result<Self, String> {
        let repo = Repository::open(path).map_err(|e| {
            format!(
                "cannot open repository at {}: {}",
                path.display(),
                short_err(&e)
            )
        })?;
        let root = repo
            .workdir()
            .or_else(|| repo.path().parent())
            .unwrap_or_else(|| repo.path())
            .to_path_buf();
        Ok(Self { inner: repo, root })
    }

    /// Discover the repository containing `path` (searches upward).
    pub fn discover(path: &Path) -> Result<Self, String> {
        let repo = Repository::discover(path).map_err(|e| {
            format!(
                "no repository found at or above {}: {}",
                path.display(),
                short_err(&e)
            )
        })?;
        let root = repo
            .workdir()
            .or_else(|| repo.path().parent())
            .unwrap_or_else(|| repo.path())
            .to_path_buf();
        Ok(Self { inner: repo, root })
    }

    pub fn inner(&self) -> &Repository {
        &self.inner
    }

    /// Worktree root (or the git dir itself for bare repos).
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn name(&self) -> String {
        self.root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "repository".to_string())
    }

    pub fn head(&self) -> Result<HeadInfo, String> {
        // An empty repository has an unborn HEAD: report it as a detached
        // empty state instead of failing the whole load.
        let Ok(head) = self.inner.head() else {
            return Ok(HeadInfo {
                branch: None,
                detached: true,
                oid: String::new(),
            });
        };
        let target = head
            .target()
            .ok_or_else(|| "head has no target".to_string())?;
        // A direct (Oid) head reference means a detached HEAD.
        let is_detached = head.kind() == Some(git2::ReferenceType::Direct);
        Ok(HeadInfo {
            branch: if is_detached {
                None
            } else {
                head.shorthand().map(|s| s.to_string())
            },
            detached: is_detached,
            oid: target.to_string(),
        })
    }

    /// All branches (local first, then remote), most recently updated first.
    pub fn branches(&self) -> Result<Vec<BranchInfo>, String> {
        let mut out = Vec::new();
        let mut locals: Vec<BranchInfo> = Vec::new();
        let mut remotes: Vec<BranchInfo> = Vec::new();
        for item in self
            .inner
            .branches(None)
            .map_err(|e| format!("branches: {}", short_err(&e)))?
        {
            let (branch, kind) = item.map_err(|e| format!("branch iter: {}", short_err(&e)))?;
            let name = branch.name().ok().flatten().unwrap_or("?").to_string();
            let Some(target) = branch.get().target() else {
                continue;
            };
            let info = BranchInfo {
                name,
                oid: target.to_string(),
                is_remote: kind == git2::BranchType::Remote,
                is_head: branch.is_head(),
            };
            if info.is_remote {
                remotes.push(info);
            } else {
                locals.push(info);
            }
        }
        out.append(&mut locals);
        out.append(&mut remotes);
        Ok(out)
    }

    /// All tags, resolved to their commit oid.
    pub fn tags(&self) -> Result<Vec<TagInfo>, String> {
        let mut out = Vec::new();
        for item in self
            .inner
            .references()
            .map_err(|e| format!("refs: {}", short_err(&e)))?
        {
            let r = item.map_err(|e| format!("ref iter: {}", short_err(&e)))?;
            if !r.is_tag() {
                continue;
            }
            let Some(name) = r.shorthand() else { continue };
            let Some(target) = r.target() else { continue };
            // Peel annotated tags down to the commit.
            let peeled = self
                .inner
                .find_object(target, None)
                .and_then(|obj| obj.peel(git2::ObjectType::Commit))
                .map(|obj| obj.id())
                .unwrap_or(target);
            out.push(TagInfo {
                name: name.to_string(),
                oid: peeled.to_string(),
            });
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    /// Resolve an arbitrary rev: branch, tag, full or abbreviated oid, HEAD~n.
    pub fn resolve(&self, rev: &str) -> Result<String, String> {
        let obj = self
            .inner
            .revparse_single(rev)
            .map_err(|e| format!("cannot resolve `{}`: {}", rev, short_err(&e)))?;
        let commit = obj
            .peel(git2::ObjectType::Commit)
            .map_err(|e| format!("`{}` is not a commit: {}", rev, short_err(&e)))?;
        Ok(commit.id().to_string())
    }

    /// Read the text content of a blob at a given commit + path.
    pub fn blob_text_at(&self, commit_oid: &str, path: &str) -> Result<Option<String>, String> {
        let oid = git2::Oid::from_str(commit_oid).map_err(|e| e.to_string())?;
        let commit = self
            .inner
            .find_commit(oid)
            .map_err(|e| format!("commit: {}", short_err(&e)))?;
        let tree = commit
            .tree()
            .map_err(|e| format!("tree: {}", short_err(&e)))?;
        let entry = match tree.get_path(std::path::Path::new(path)) {
            Ok(e) => e,
            Err(_) => return Ok(None),
        };
        if entry.kind() != Some(git2::ObjectType::Blob) {
            return Ok(None);
        }
        let blob = self
            .inner
            .find_blob(entry.id())
            .map_err(|e| format!("blob: {}", short_err(&e)))?;
        Ok(Some(String::from_utf8_lossy(blob.content()).into_owned()))
    }

    /// The default "now" for age computations: the newest commit time in the repo.
    pub fn newest_commit_time(&self) -> i64 {
        let mut walk = match self.inner.revwalk() {
            Ok(w) => w,
            Err(_) => return 0,
        };
        if walk.push_head().is_err() {
            return 0;
        }
        let _ = walk.set_sorting(git2::Sort::TIME);
        for oid in walk.flatten() {
            if let Ok(c) = self.inner.find_commit(oid) {
                return c.time().seconds();
            }
        }
        0
    }
}

fn short_err(e: &git2::Error) -> String {
    match e.message() {
        "" => format!("{:?}", e.code()),
        m => m.to_string(),
    }
}

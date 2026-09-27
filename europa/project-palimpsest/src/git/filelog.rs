//! File world-lines: the history of a single path, following renames.

use git2::Oid;

use super::repo::Repo;

#[derive(Clone)]
pub struct FileEvent {
    pub oid: String,
    pub time: i64,
    pub summary: String,
    pub author: String,
    pub status: char,
    /// Adds/dels for this file in this commit (cheap: per-file patch only).
    pub adds: u32,
    pub dels: u32,
    /// Previous path when the event is a rename.
    pub prev_path: Option<String>,
}

/// Walk the loaded history window following `path` backwards through renames.
///
/// Bounded: scans at most `scan_cap` commits newest-first. Rename tracking
/// uses diff rename detection per commit pair (bounded by the cap).
pub fn file_events(
    repo: &Repo,
    history: &crate::git::history::History,
    path: &str,
    scan_cap: usize,
) -> Vec<FileEvent> {
    let mut out = Vec::new();
    let repo_i = repo.inner();
    let mut current = path.to_string();

    for (scanned, row) in history.rows.iter().enumerate() {
        if scanned >= scan_cap || out.len() >= 512 {
            break;
        }
        let Ok(oid) = Oid::from_str(&row.oid) else {
            continue;
        };
        let Ok(commit) = repo_i.find_commit(oid) else {
            continue;
        };
        let Ok(tree) = commit.tree() else { continue };
        let parent_tree = match commit.parent(0) {
            Ok(p) => p.tree().ok(),
            Err(_) => None,
        };
        let mut opts = git2::DiffOptions::new();
        opts.include_typechange(true);
        // NOTE: no pathspec here. Rename detection needs both sides of a
        // rename inside the delta set; a pathspec-filtered diff cannot pair
        // old path with new path. Matching happens on the delta list below.
        let Ok(mut diff) =
            repo_i.diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), Some(&mut opts))
        else {
            continue;
        };
        let mut find = git2::DiffFindOptions::new();
        find.renames(true).rename_threshold(50);
        let _ = diff.find_similar(Some(&mut find));
        let mut matched_idx = None;
        let mut status = '?';
        let mut prev_path = None;
        for (di, d) in diff.deltas().enumerate() {
            let np = d
                .new_file()
                .path()
                .map(|p| p.to_string_lossy().into_owned());
            let op = d
                .old_file()
                .path()
                .map(|p| p.to_string_lossy().into_owned());
            if np.as_deref() == Some(current.as_str()) || op.as_deref() == Some(current.as_str()) {
                matched_idx = Some(di);
                status = match d.status() {
                    git2::Delta::Added => 'A',
                    git2::Delta::Deleted => 'D',
                    git2::Delta::Renamed => 'R',
                    git2::Delta::Copied => 'C',
                    _ => 'M',
                };
                if status == 'R' {
                    prev_path = op;
                }
                break;
            }
        }
        let Some(delta_idx) = matched_idx else {
            continue;
        };

        // Per-file adds/dels: patch just for this file.
        let mut adds = 0;
        let mut dels = 0;
        if let Ok(Some(patch)) = git2::Patch::from_diff(&diff, delta_idx) {
            for h in 0..patch.num_hunks() {
                if let Ok((_, n_lines)) = patch.hunk(h) {
                    for l in 0..n_lines {
                        if let Ok(line) = patch.line_in_hunk(h, l) {
                            match line.origin() {
                                '+' => adds += 1,
                                '-' => dels += 1,
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
        out.push(FileEvent {
            oid: row.oid.clone(),
            time: row.time,
            summary: row.summary.clone(),
            author: row.author.clone(),
            status,
            adds,
            dels,
            prev_path: prev_path.clone(),
        });
        // Follow the rename backwards.
        if let Some(p) = prev_path {
            current = p;
        }
    }
    out
}

/// The file list of a commit (paths, with directories implied).
pub struct TreeEntry {
    pub path: String,
    pub kind: EntryKind,
    pub size: u32,
}

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum EntryKind {
    Dir,
    File,
}

/// List the tree of `oid`, bounded to `max` entries, depth-first.
pub fn list_tree(repo: &Repo, oid: &str, max: usize) -> Result<Vec<TreeEntry>, String> {
    let repo_i = repo.inner();
    let oid = Oid::from_str(oid).map_err(|e| e.to_string())?;
    let commit = repo_i
        .find_commit(oid)
        .map_err(|e| format!("commit: {}", e))?;
    let tree = commit.tree().map_err(|e| format!("tree: {}", e))?;
    let mut out = Vec::new();
    walk_tree(repo_i, &tree, "", &mut out, max, 0);
    Ok(out)
}

fn walk_tree(
    repo: &git2::Repository,
    tree: &git2::Tree,
    prefix: &str,
    out: &mut Vec<TreeEntry>,
    max: usize,
    depth: usize,
) {
    if out.len() >= max || depth > 8 {
        return;
    }
    for entry in tree.iter() {
        if out.len() >= max {
            return;
        }
        let name = entry.name().unwrap_or("?");
        let path = if prefix.is_empty() {
            name.to_string()
        } else {
            format!("{}/{}", prefix, name)
        };
        match entry.kind() {
            Some(git2::ObjectType::Tree) => {
                out.push(TreeEntry {
                    path: path.clone(),
                    kind: EntryKind::Dir,
                    size: 0,
                });
                if let Ok(sub) = repo.find_tree(entry.id()) {
                    walk_tree(repo, &sub, &path, out, max, depth + 1);
                }
            }
            Some(git2::ObjectType::Blob) => {
                out.push(TreeEntry {
                    path,
                    kind: EntryKind::File,
                    size: entry.filemode() as u32,
                });
            }
            _ => {}
        }
    }
}

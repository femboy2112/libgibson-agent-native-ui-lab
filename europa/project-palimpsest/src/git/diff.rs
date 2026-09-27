//! Diff lens data: per-commit file lists, per-file hunks, and intraline
//! emphasis. All computed lazily on demand — a diff is only built for the
//! commit under inspection.

use git2::Oid;

use super::repo::Repo;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LineKind {
    Context,
    Add,
    Del,
}

#[derive(Clone)]
pub struct DiffLine {
    pub kind: LineKind,
    pub old_no: Option<u32>,
    pub new_no: Option<u32>,
    pub text: String,
    /// Intraline emphasis range: (start, len) in display chars, if any.
    pub emph: Option<(usize, usize)>,
}

#[derive(Clone)]
pub struct Hunk {
    pub header: String,
    pub old_start: u32,
    pub old_len: u32,
    pub new_start: u32,
    pub new_len: u32,
    pub lines: Vec<DiffLine>,
}

#[derive(Clone)]
pub struct FileDiff {
    pub old_path: String,
    pub new_path: String,
    pub status: char,
    pub adds: u32,
    pub dels: u32,
    pub binary: bool,
    pub hunks: Vec<Hunk>,
}

#[derive(Clone)]
pub struct CommitDiff {
    pub oid: String,
    pub files: Vec<FileDiff>,
    pub total_adds: u32,
    pub total_dels: u32,
}

impl FileDiff {
    pub fn path(&self) -> &str {
        if self.new_path.is_empty() {
            &self.old_path
        } else {
            &self.new_path
        }
    }

    /// `A`/`M`/`D`/`R` status glyph used by the lens file list.
    pub fn status_word(&self) -> &'static str {
        match self.status {
            'A' => "add",
            'D' => "del",
            'R' => "ren",
            'C' => "cpy",
            _ => "mod",
        }
    }
}

fn status_char(delta: &git2::DiffDelta) -> char {
    match delta.status() {
        git2::Delta::Added => 'A',
        git2::Delta::Deleted => 'D',
        git2::Delta::Renamed => 'R',
        git2::Delta::Copied => 'C',
        git2::Delta::Typechange => 'T',
        _ => 'M',
    }
}

fn opts_with_renames() -> git2::DiffOptions {
    let mut opts = git2::DiffOptions::new();
    opts.include_typechange(true);
    opts
}

/// Apply rename/copy detection to an already-computed diff (git2 does this
/// as a post-pass via `find_similar`, not as a diff option).
fn detect_renames(diff: &mut git2::Diff) {
    let mut find = git2::DiffFindOptions::new();
    find.renames(true).copies(true).rename_threshold(50);
    let _ = diff.find_similar(Some(&mut find));
}

/// Whole-commit diff with per-file hunk structure (bounded to `max_files`).
pub fn commit_diff(repo: &Repo, oid: &str, max_files: usize) -> Result<CommitDiff, String> {
    let repo_i = repo.inner();
    let oid = Oid::from_str(oid).map_err(|e| e.to_string())?;
    let commit = repo_i
        .find_commit(oid)
        .map_err(|e| format!("commit: {}", e))?;
    let tree = commit.tree().map_err(|e| format!("tree: {}", e))?;
    let parent_tree = match commit.parent(0) {
        Ok(p) => Some(p.tree().map_err(|e| format!("ptree: {}", e))?),
        Err(_) => None,
    };
    let mut opts = opts_with_renames();
    let mut diff = repo_i
        .diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), Some(&mut opts))
        .map_err(|e| format!("diff: {}", e))?;
    detect_renames(&mut diff);

    let mut files = Vec::new();
    for (idx, delta) in diff.deltas().enumerate() {
        if idx >= max_files {
            break;
        }
        let old_path = delta
            .old_file()
            .path()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        let new_path = delta
            .new_file()
            .path()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        let status = status_char(&delta);
        let mut adds = 0;
        let mut dels = 0;
        for line in delta_line_iter(repo_i, &diff, idx) {
            match line.kind {
                LineKind::Add => adds += 1,
                LineKind::Del => dels += 1,
                LineKind::Context => {}
            }
        }
        let mut fd = FileDiff {
            old_path,
            new_path,
            status,
            adds,
            dels,
            binary: delta.flags().contains(git2::DiffFlags::BINARY)
                || delta.status() == git2::Delta::Untracked,
            hunks: Vec::new(),
        };
        if !fd.binary {
            fd.hunks = file_hunks(repo_i, &diff, idx);
        }
        files.push(fd);
    }

    let total_adds = files.iter().map(|f| f.adds).sum();
    let total_dels = files.iter().map(|f| f.dels).sum();
    Ok(CommitDiff {
        oid: oid.to_string(),
        files,
        total_adds,
        total_dels,
    })
}

/// Tier-B aggregate line stat for a commit (no hunk retention).
pub fn commit_line_stat(repo: &Repo, oid: &str) -> Option<super::history::LineStat> {
    let repo_i = repo.inner();
    let oid = Oid::from_str(oid).ok()?;
    let commit = repo_i.find_commit(oid).ok()?;
    let tree = commit.tree().ok()?;
    let parent_tree = match commit.parent(0) {
        Ok(p) => Some(p.tree().ok()?),
        Err(_) => None,
    };
    let mut opts = opts_with_renames();
    let mut diff = repo_i
        .diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), Some(&mut opts))
        .ok()?;
    detect_renames(&mut diff);
    let stats = diff.stats().ok()?;
    Some(super::history::LineStat {
        files: stats.files_changed() as u32,
        adds: stats.insertions() as u32,
        dels: stats.deletions() as u32,
    })
}

/// Iterate the lines of one file's patch inside a diff.
fn delta_line_iter<'d>(
    repo: &'d git2::Repository,
    diff: &'d git2::Diff,
    idx: usize,
) -> impl Iterator<Item = DiffLine> + 'd {
    let mut all: Vec<DiffLine> = Vec::new();
    if let Ok(Some(patch)) = git2::Patch::from_diff(diff, idx) {
        let n = patch.num_hunks();
        for h in 0..n {
            if let Ok((_, n_lines)) = patch.hunk(h) {
                for l in 0..n_lines {
                    if let Ok(line) = patch.line_in_hunk(h, l) {
                        if let Some(dl) = convert_line(&line) {
                            all.push(dl);
                        }
                    }
                }
            }
        }
    }
    let _ = repo;
    all.into_iter()
}

/// Build hunks for one file, applying intraline emphasis between adjacent
/// del/add pairs.
fn file_hunks(repo: &git2::Repository, diff: &git2::Diff, idx: usize) -> Vec<Hunk> {
    let _ = repo;
    let mut hunks = Vec::new();
    let Ok(Some(patch)) = git2::Patch::from_diff(diff, idx) else {
        return hunks;
    };
    let n = patch.num_hunks();
    for h in 0..n {
        let Ok((hunk, n_lines)) = patch.hunk(h) else {
            continue;
        };
        let mut lines: Vec<DiffLine> = Vec::with_capacity(n_lines);
        for l in 0..n_lines {
            let Ok(line) = patch.line_in_hunk(h, l) else {
                continue;
            };
            if let Some(dl) = convert_line(&line) {
                lines.push(dl);
            }
        }
        mark_intraline(&mut lines);
        hunks.push(Hunk {
            header: String::from_utf8_lossy(hunk.header())
                .trim_end()
                .to_string(),
            old_start: hunk.old_start(),
            old_len: hunk.old_lines(),
            new_start: hunk.new_start(),
            new_len: hunk.new_lines(),
            lines,
        });
    }
    hunks
}

fn convert_line(line: &git2::DiffLine) -> Option<DiffLine> {
    let (kind, old_no, new_no) = match line.origin() {
        '+' => (LineKind::Add, None, line.new_lineno()),
        '-' => (LineKind::Del, line.old_lineno(), None),
        ' ' => (LineKind::Context, line.old_lineno(), line.new_lineno()),
        _ => return None, // EOF markers, mode lines, binary notes
    };
    let content = String::from_utf8_lossy(line.content());
    Some(DiffLine {
        kind,
        old_no,
        new_no,
        // Tabs are hostile to alignment in a terminal; expand conservatively.
        text: content.trim_end_matches(['\n', '\r']).replace('\t', "    "),
        emph: None,
    })
}

/// Word-level intraline emphasis: within each run of dels followed by adds,
/// the common prefix and common suffix are dimmed by marking the interior
/// difference. Runs pairwise longest-common prefix/suffix — cheap and honest.
fn mark_intraline(lines: &mut [DiffLine]) {
    let mut i = 0;
    while i < lines.len() {
        // find a del run
        if lines[i].kind != LineKind::Del {
            i += 1;
            continue;
        }
        let d_start = i;
        let mut d_end = i;
        while d_end < lines.len() && lines[d_end].kind == LineKind::Del {
            d_end += 1;
        }
        let a_start = d_end;
        let mut a_end = a_start;
        while a_end < lines.len() && lines[a_end].kind == LineKind::Add {
            a_end += 1;
        }
        if a_end > a_start {
            // pair up to min(len) lines: emphasize the differing interior
            let pairs = (d_end - d_start).min(a_end - a_start);
            for k in 0..pairs {
                let del_txt = lines[d_start + k].text.clone();
                let add_txt = lines[a_start + k].text.clone();
                if let Some((s, l)) = interior_difference(&del_txt, &add_txt) {
                    lines[d_start + k].emph = Some((s, l));
                    lines[a_start + k].emph = Some((s, l));
                }
            }
        }
        i = a_end.max(d_end);
    }
}

/// Common-prefix/suffix trim: returns (start, len) of the differing interior
/// in char indices, when it is a strict interior (both prefix & suffix > 0).
pub fn interior_difference(a: &str, b: &str) -> Option<(usize, usize)> {
    let ac: Vec<char> = a.chars().collect();
    let bc: Vec<char> = b.chars().collect();
    if ac.is_empty() || bc.is_empty() || ac == bc {
        return None;
    }
    let mut pre = 0;
    while pre < ac.len() && pre < bc.len() && ac[pre] == bc[pre] {
        pre += 1;
    }
    let mut suf = 0;
    while suf < ac.len() - pre
        && suf < bc.len() - pre
        && ac[ac.len() - 1 - suf] == bc[bc.len() - 1 - suf]
    {
        suf += 1;
    }
    let interior_len = ac.len() - pre - suf;
    // Only emphasize when it genuinely narrows the change.
    if pre == 0 && suf == 0 {
        return None;
    }
    Some((pre, interior_len))
}

/// A bounded "export" rendering of a commit diff (for scrollback), capped at
/// `max_lines` lines total across files.
pub fn export_diff_text(repo: &Repo, oid: &str, max_lines: usize) -> Result<String, String> {
    let d = commit_diff(repo, oid, 64)?;
    let mut out = String::new();
    let mut used = 0usize;
    for f in &d.files {
        if used >= max_lines {
            out.push_str("    … (diff truncated at export bound)\n");
            break;
        }
        out.push_str(&format!(
            "{} {}\n",
            f.status,
            if f.new_path.is_empty() {
                &f.old_path
            } else {
                &f.new_path
            }
        ));
        used += 1;
        for h in &f.hunks {
            if used >= max_lines {
                break;
            }
            out.push_str(&format!("    {}\n", h.header));
            used += 1;
            for l in &h.lines {
                if used >= max_lines {
                    break;
                }
                let sign = match l.kind {
                    LineKind::Add => '+',
                    LineKind::Del => '-',
                    LineKind::Context => ' ',
                };
                out.push_str(&format!("    {}{}\n", sign, l.text));
                used += 1;
            }
        }
    }
    Ok(out)
}
#[cfg(test)]
mod tests {
    use super::*;

    fn line(kind: LineKind, text: &str) -> DiffLine {
        DiffLine {
            kind,
            old_no: None,
            new_no: None,
            text: text.to_string(),
            emph: None,
        }
    }

    #[test]
    fn interior_difference_locates_the_changed_middle() {
        // common prefix "fn alpha(a: " is 12 chars; 'i32' vs 'u64' is the
        // changed interior; no common suffix ('2' vs '4' differ)
        let (s, l) = interior_difference("fn alpha(a: i32", "fn alpha(a: u64").unwrap();
        assert_eq!(s, 12);
        assert_eq!(l, 3);
        assert_eq!(&"fn alpha(a: i32"[s..s + l], "i32");
        assert_eq!(&"fn alpha(a: u64"[s..s + l], "u64");
    }

    #[test]
    fn interior_difference_rejects_unhelpful_cases() {
        assert!(interior_difference("", "x").is_none(), "empty side");
        assert!(interior_difference("same", "same").is_none(), "identical");
        // no common prefix or suffix: nothing to dim
        assert!(interior_difference("abc", "xyz").is_none());
    }

    #[test]
    fn interior_difference_handles_common_suffix_only() {
        let (s, l) = interior_difference("count = 1;", "total = 1;").unwrap();
        assert_eq!(s, 0);
        assert_eq!(&"count = 1;"[s..s + l], "count");
        assert_eq!(&"total = 1;"[s..s + l], "total");
    }

    #[test]
    fn intraline_marks_only_paired_lines() {
        let mut lines = vec![
            line(LineKind::Context, "fn f() {"),
            line(LineKind::Del, "    let x = alpha(1);"),
            line(LineKind::Add, "    let x = alpha(2);"),
            line(LineKind::Del, "    totally unrelated removal"),
            line(LineKind::Add, "    let y = 7;"),
            line(LineKind::Context, "}"),
        ];
        mark_intraline(&mut lines);
        // first del/add pair shares a frame -> both get an emphasis range
        assert!(lines[1].emph.is_some(), "paired del is emphasized");
        assert!(lines[2].emph.is_some(), "paired add is emphasized");
        // both pairs differ, so both are marked
        assert!(lines[3].emph.is_some());
        assert!(lines[4].emph.is_some());
        // context lines are never touched
        assert!(lines[0].emph.is_none());
        assert!(lines[5].emph.is_none());
    }

    #[test]
    fn intraline_ignores_lonely_runs() {
        let mut lines = vec![
            line(LineKind::Del, "only a deletion"),
            line(LineKind::Context, "context"),
            line(LineKind::Add, "only an addition"),
        ];
        mark_intraline(&mut lines);
        // del followed by context is not a pair; the trailing add run has no
        // del partner: nothing is marked
        assert!(lines.iter().all(|l| l.emph.is_none()));
    }
}

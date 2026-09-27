//! Provenance fibers: per-line blame of a file at a given commit.

use git2::Oid;

use super::repo::Repo;

#[derive(Clone)]
pub struct BlameLine {
    pub line_no: u32,
    pub text: String,
    /// Commit that last touched this line.
    pub oid: String,
    pub author: String,
    pub time: i64,
    /// Line number in the introducing commit.
    pub orig_line: u32,
}

#[derive(Clone)]
pub struct BlameData {
    pub path: String,
    pub oid: String,
    pub lines: Vec<BlameLine>,
}

/// Blame `path` at commit `oid`. `max_lines` bounds retention.
pub fn blame_file(
    repo: &Repo,
    oid: &str,
    path: &str,
    max_lines: usize,
) -> Result<BlameData, String> {
    let repo_i = repo.inner();
    let commit_oid = Oid::from_str(oid).map_err(|e| e.to_string())?;
    let commit = repo_i
        .find_commit(commit_oid)
        .map_err(|e| format!("commit: {}", e))?;

    // Blob content at this commit.
    let tree = commit.tree().map_err(|e| format!("tree: {}", e))?;
    let entry = tree
        .get_path(std::path::Path::new(path))
        .map_err(|e| format!("`{}` not in this commit: {}", path, e))?;
    let blob = repo_i
        .find_blob(entry.id())
        .map_err(|e| format!("blob: {}", e))?;

    let mut opts = git2::BlameOptions::new();
    // libgit2 blames the working-directory file by default; `newest_commit`
    // pins the blame to the file's content *at that commit*, which is exactly
    // the "provenance at a point in time" Palimpsest needs.
    let _ = opts.newest_commit(commit_oid).track_copies_same_file(true);

    let blame = repo_i
        .blame_file(std::path::Path::new(path), Some(&mut opts))
        .map_err(|e| format!("blame `{}`: {}", path, e))?;

    let content = blob.content();
    let text = String::from_utf8_lossy(content);
    let mut raw_lines: Vec<&str> = text.split('\n').collect();
    // A trailing newline yields a phantom final line; blame has no line there.
    if raw_lines.len() > 1 && raw_lines.last() == Some(&"") {
        raw_lines.pop();
    }
    let total = raw_lines.len();

    let mut out: Vec<BlameLine> = Vec::with_capacity(total.min(max_lines));
    for (i, raw) in raw_lines.iter().enumerate() {
        if out.len() >= max_lines {
            break;
        }
        let line_no = (i + 1) as u32;
        let mut info: Option<(String, String, i64, u32)> = None;
        // blame hunks are 1-indexed; get_line finds the hunk covering a line.
        if let Some(hunk) = blame.get_line(line_no as usize) {
            let orig_commit = hunk.orig_commit_id();
            let author = hunk.orig_signature().name().unwrap_or("?").to_string();
            let time = hunk.orig_signature().when().seconds();
            info = Some((
                orig_commit.to_string(),
                author,
                time,
                hunk.orig_start_line() as u32,
            ));
        }
        let (oid_l, author, time, orig_line) = info.unwrap_or((
            oid.to_string(),
            "?".to_string(),
            commit.time().seconds(),
            line_no,
        ));
        out.push(BlameLine {
            line_no,
            text: raw.trim_end_matches('\r').replace('\t', "    "),
            oid: oid_l,
            author,
            time,
            orig_line,
        });
    }

    Ok(BlameData {
        path: path.to_string(),
        oid: oid.to_string(),
        lines: out,
    })
}

//! Diff-lens helpers: flattening hunks into a displayable line model and
//! searching within the diff.

use crate::git::diff::{CommitDiff, LineKind};

/// One displayable row of the hunk pane.
pub struct DisplayLine {
    pub file_idx: usize,
    pub hunk_idx: usize,
    /// None => file header row; Some(i) => line i within the hunk.
    pub line_idx: Option<usize>,
    pub text: String,
    pub kind: LineKind,
    pub old_no: Option<u32>,
    pub new_no: Option<u32>,
    pub emph: Option<(usize, usize)>,
}

impl DisplayLine {
    pub fn is_meta(&self) -> bool {
        self.line_idx.is_none()
    }
}

/// Flatten a commit diff into display rows, bounded to `max_lines`.
pub fn flatten(diff: &CommitDiff, max_lines: usize) -> Vec<DisplayLine> {
    let mut out = Vec::with_capacity(max_lines.min(4096));
    for (fi, f) in diff.files.iter().enumerate() {
        if out.len() >= max_lines {
            break;
        }
        out.push(DisplayLine {
            file_idx: fi,
            hunk_idx: 0,
            line_idx: None,
            text: format!("── {} ──", f.path()),
            kind: LineKind::Context,
            old_no: None,
            new_no: None,
            emph: None,
        });
        for (hi, h) in f.hunks.iter().enumerate() {
            if out.len() >= max_lines {
                break;
            }
            out.push(DisplayLine {
                file_idx: fi,
                hunk_idx: hi,
                line_idx: None,
                text: format!("  {}", h.header),
                kind: LineKind::Context,
                old_no: None,
                new_no: None,
                emph: None,
            });
            for (li, l) in h.lines.iter().enumerate() {
                if out.len() >= max_lines {
                    break;
                }
                out.push(DisplayLine {
                    file_idx: fi,
                    hunk_idx: hi,
                    line_idx: Some(li),
                    text: l.text.clone(),
                    kind: l.kind,
                    old_no: l.old_no,
                    new_no: l.new_no,
                    emph: l.emph,
                });
            }
        }
    }
    out
}

/// Find `needle` in the flattened diff; returns (file, line) coordinates.
pub fn find_matches(diff: &CommitDiff, needle: &str, cap: usize) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    if needle.is_empty() {
        return out;
    }
    let n = needle.to_lowercase();
    'outer: for (fi, f) in diff.files.iter().enumerate() {
        if f.path().to_lowercase().contains(&n) {
            out.push((fi, usize::MAX)); // path-level match
            if out.len() >= cap {
                break;
            }
        }
        for (hi, h) in f.hunks.iter().enumerate() {
            for l in &h.lines {
                if l.text.to_lowercase().contains(&n) {
                    out.push((fi, hi));
                    if out.len() >= cap {
                        break 'outer;
                    }
                    break; // one hit per hunk
                }
            }
        }
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::diff::{CommitDiff, DiffLine, FileDiff, Hunk};

    fn hunk(header: &str, lines: Vec<(&str, LineKind)>) -> Hunk {
        Hunk {
            header: header.to_string(),
            old_start: 1,
            old_len: lines.len() as u32,
            new_start: 1,
            new_len: lines.len() as u32,
            lines: lines
                .into_iter()
                .map(|(t, k)| DiffLine {
                    kind: k,
                    old_no: None,
                    new_no: None,
                    text: t.to_string(),
                    emph: None,
                })
                .collect(),
        }
    }

    fn fixture_diff() -> CommitDiff {
        CommitDiff {
            oid: "0123456789abcdef".to_string(),
            files: vec![
                FileDiff {
                    old_path: "src/a.rs".to_string(),
                    new_path: "src/a.rs".to_string(),
                    status: 'M',
                    adds: 1,
                    dels: 1,
                    binary: false,
                    hunks: vec![hunk(
                        "@@ -1,3 +1,3 @@",
                        vec![
                            ("fn a() {}", LineKind::Context),
                            ("    old", LineKind::Del),
                            ("    new", LineKind::Add),
                        ],
                    )],
                },
                FileDiff {
                    old_path: String::new(),
                    new_path: "src/b.rs".to_string(),
                    status: 'A',
                    adds: 2,
                    dels: 0,
                    binary: false,
                    hunks: vec![hunk(
                        "@@ -0,0 +1,2 @@",
                        vec![("one", LineKind::Add), ("two", LineKind::Add)],
                    )],
                },
            ],
            total_adds: 3,
            total_dels: 1,
        }
    }

    #[test]
    fn flatten_emits_meta_rows_between_files() {
        let d = fixture_diff();
        let rows = flatten(&d, 4096);
        assert!(rows[0].is_meta(), "first row is the file header");
        assert!(rows[0].text.contains("src/a.rs"));
        assert!(rows[1].is_meta(), "second row is the hunk header");
        assert!(rows[1].text.contains("@@"));
        assert_eq!(rows[2].text, "fn a() {}", "context line follows");
        assert_eq!(rows[2].kind, LineKind::Context);
        // second file begins with its own meta row
        let b = rows
            .iter()
            .position(|r| r.is_meta() && r.text.contains("src/b.rs"))
            .expect("second file header present");
        assert!(b > 2, "second file comes after the first file's lines");
    }

    #[test]
    fn flatten_respects_the_line_bound() {
        let d = fixture_diff();
        let rows = flatten(&d, 4);
        assert!(rows.len() <= 4, "bound respected");
        assert!(rows[0].is_meta());
    }

    #[test]
    fn find_matches_reports_path_and_hunk_hits() {
        let d = fixture_diff();
        // path-level match
        let m = find_matches(&d, "b.rs", 100);
        assert!(m.contains(&(1, usize::MAX)), "path match: {m:?}");
        // text-level match inside the first file's hunk
        let m = find_matches(&d, "old", 100);
        assert!(m.contains(&(0, 0)), "text match: {m:?}");
        // no match
        assert!(find_matches(&d, "zzz", 100).is_empty());
        // empty needle matches nothing
        assert!(find_matches(&d, "", 100).is_empty());
    }

    #[test]
    fn find_matches_caps_results() {
        let d = fixture_diff();
        // "src/" matches everything; cap at 1
        let m = find_matches(&d, "src/", 1);
        assert!(m.len() <= 1, "cap respected: {m:?}");
    }
}

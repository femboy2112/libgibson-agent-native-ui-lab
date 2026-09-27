//! Repository health metrics — honest, bounded, and clearly labeled when
//! estimated. No manufactured certainty: sampled quantities are marked with
//! their sample size.

use std::collections::HashMap;

use super::history::History;
use super::repo::{BranchInfo, Repo};

pub struct AuthorStat {
    pub name: String,
    pub commits: u32,
    pub share: f32,
}

pub struct FileChurn {
    pub path: String,
    pub touches: u32,
    /// True when extrapolated from a partial sample.
    pub estimated: bool,
}

pub struct LargestCommit {
    pub oid: String,
    pub short: String,
    pub summary: String,
    pub files: u32,
    pub author: String,
    pub time: i64,
}

pub struct BranchAge {
    pub name: String,
    pub last_commit: i64,
    pub commits: u32,
    pub is_remote: bool,
    /// This tip may be on a branch unreachable from the indexed HEAD walk.
    pub in_window: bool,
}

pub struct Metrics {
    pub commits: usize,
    pub scanned: usize,
    pub truncated: bool,
    pub authors: Vec<AuthorStat>,
    pub hottest: Vec<FileChurn>,
    pub largest: Vec<LargestCommit>,
    pub branches: Vec<BranchAge>,
    pub tag_count: usize,
    pub merge_count: u32,
    pub merge_ratio: f32,
    pub span_days: i64,
    /// Monthly activity (month key "YYYY-MM" asc, count).
    pub monthly: Vec<(String, u32)>,
    /// Share of sampled file events belonging to the ten hottest files.
    pub concentration: f32,
    /// Whether every indexed commit was examined for file statistics.
    pub complete_scan: bool,
    pub sample_size: usize,
}

/// Number of commits sampled for per-file churn (tier-A delta enumerations).
const CHURN_SAMPLE: usize = 400;

pub fn compute(repo: &Repo, history: &History, branches: &[BranchInfo], scanned: usize) -> Metrics {
    let rows = &history.rows;

    // Authors.
    let mut author_counts: HashMap<&str, u32> = HashMap::new();
    for r in rows {
        *author_counts.entry(r.author.as_str()).or_insert(0) += 1;
    }
    let total_commits = rows.len().max(1) as f32;
    let mut authors: Vec<AuthorStat> = author_counts
        .into_iter()
        .map(|(name, commits)| AuthorStat {
            name: name.to_string(),
            commits,
            share: commits as f32 / total_commits,
        })
        .collect();
    authors.sort_by(|a, b| b.commits.cmp(&a.commits).then(a.name.cmp(&b.name)));

    // Merge ratio.
    let merge_count = rows.iter().filter(|r| r.is_merge).count() as u32;
    let merge_ratio = merge_count as f32 / total_commits;

    // Monthly activity.
    let mut monthly_map: std::collections::BTreeMap<String, u32> = Default::default();
    for r in rows {
        let key = month_key(r.time);
        *monthly_map.entry(key).or_insert(0) += 1;
    }
    let monthly: Vec<(String, u32)> = monthly_map.into_iter().collect();

    // Branch ages include tips unreachable from HEAD or beyond --limit.
    // Missing branch objects are skipped, never assigned a made-up age.
    let mut branch_ages: Vec<BranchAge> = Vec::new();
    for b in branches {
        let idx = history.idx_of(&b.oid);
        let timestamp = if let Some(idx) = idx {
            rows[idx as usize].time
        } else {
            let Ok(oid) = git2::Oid::from_str(&b.oid) else {
                continue;
            };
            let Ok(commit) = repo.inner().find_commit(oid) else {
                continue;
            };
            commit.time().seconds()
        };
        branch_ages.push(BranchAge {
            name: b.name.clone(),
            last_commit: timestamp,
            commits: 0,
            is_remote: b.is_remote,
            in_window: idx.is_some(),
        });
    }
    branch_ages.sort_by_key(|b| std::cmp::Reverse(b.last_commit));

    // Per-file churn: sampled across the loaded set (tier-A enumerations).
    let (hottest, concentration, complete_scan, sample_size, largest) = churn_sample(repo, history);

    let span_days = ((history.t_max - history.t_min) / 86_400).max(0);
    Metrics {
        commits: rows.len(),
        scanned,
        truncated: history.truncated,
        authors,
        hottest,
        largest,
        branches: branch_ages,
        tag_count: rows.iter().map(|r| r.tags.len()).sum(),
        merge_count,
        merge_ratio,
        span_days,
        monthly,
        concentration,
        complete_scan,
        sample_size,
    }
}

/// Evenly sample `CHURN_SAMPLE` commits across the loaded window and count
/// per-path deltas. The step is derived from the loaded size; results are
/// extrapolated back to full-window estimates. The same bounded walk yields
/// actual changed-path counts for the largest-commit sample.
fn churn_sample(
    repo: &Repo,
    history: &History,
) -> (Vec<FileChurn>, f32, bool, usize, Vec<LargestCommit>) {
    let n = history.rows.len();
    if n == 0 {
        return (Vec::new(), 0.0, false, 0, Vec::new());
    }
    let step = ((n as f32 / CHURN_SAMPLE as f32).ceil() as usize).max(1);
    let complete_scan = step == 1;

    let mut churn: HashMap<String, u32> = HashMap::new();
    let mut total_touches = 0u32;
    let mut sampled_count = 0usize;
    let mut largest = Vec::new();
    let mut i = 0usize;
    while i < n {
        let row = &history.rows[i];
        sampled_count += 1;
        if let Ok(files) = super::history::History::files_of(repo, &row.oid) {
            largest.push(LargestCommit {
                oid: row.oid.clone(),
                short: row.short.clone(),
                summary: row.summary.clone(),
                files: files.len() as u32,
                author: row.author.clone(),
                time: row.time,
            });
            for f in files {
                *churn.entry(f).or_insert(0) += 1;
                total_touches += 1;
            }
        }
        i += step;
    }
    largest.sort_by(|a, b| b.files.cmp(&a.files).then(a.oid.cmp(&b.oid)));
    largest.truncate(8);

    let mut raw_churn: Vec<(String, u32)> = churn.into_iter().collect();
    raw_churn.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    // Scaling cancels in a share: calculate against sampled event counts.
    let concentration = if total_touches == 0 {
        0.0
    } else {
        raw_churn.iter().take(10).map(|(_, n)| *n).sum::<u32>() as f32 / total_touches as f32
    };
    let hottest: Vec<FileChurn> = raw_churn
        .into_iter()
        .take(10)
        .map(|(path, touches)| FileChurn {
            path,
            touches: if complete_scan {
                touches
            } else {
                ((touches as u64 * n as u64 + sampled_count as u64 / 2) / sampled_count as u64)
                    as u32
            },
            estimated: !complete_scan,
        })
        .collect();

    (
        hottest,
        concentration,
        complete_scan,
        sampled_count,
        largest,
    )
}

fn month_key(ts: i64) -> String {
    crate::theme::fmt_date(ts)[..7].to_string()
}

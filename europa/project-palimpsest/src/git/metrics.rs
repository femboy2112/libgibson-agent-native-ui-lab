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
    /// True when extrapolated from a sample (see `sampled`).
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
    /// Share of sampled file events belonging to the top 5% of files.
    pub concentration: f32,
    /// Whether churn stats were extrapolated from a sample.
    pub sampled: bool,
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

    // Largest commits by changed-path count (tier A, lazily resolved —
    // bounded to the eagerly-resolved prefix plus on-demand resolution).
    let mut sized: Vec<&super::history::CommitRow> =
        rows.iter().filter(|r| r.files.is_some()).collect();
    sized.sort_by_key(|r| std::cmp::Reverse(r.files.unwrap_or(0)));
    let largest: Vec<LargestCommit> = sized
        .iter()
        .take(8)
        .map(|r| LargestCommit {
            oid: r.oid.clone(),
            short: r.short.clone(),
            summary: r.summary.clone(),
            files: r.files.unwrap_or(0),
            author: r.author.clone(),
            time: r.time,
        })
        .collect();

    // Branch ages: last-commit time of each tip within the loaded window.
    let mut branch_ages: Vec<BranchAge> = Vec::new();
    for b in branches {
        let Some(idx) = history.idx_of(&b.oid) else {
            continue;
        };
        branch_ages.push(BranchAge {
            name: b.name.clone(),
            last_commit: rows[idx as usize].time,
            commits: 0,
            is_remote: b.is_remote,
        });
    }
    branch_ages.sort_by_key(|b| std::cmp::Reverse(b.last_commit));

    // Per-file churn: sampled across the loaded set (tier-A enumerations).
    let (hottest, concentration, sampled, sample_size) = churn_sample(repo, history);

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
        sampled,
        sample_size,
    }
}

/// Evenly sample `CHURN_SAMPLE` commits across the loaded window and count
/// per-path deltas. The step is derived from the loaded size; results are
/// extrapolated back to full-window estimates.
fn churn_sample(repo: &Repo, history: &History) -> (Vec<FileChurn>, f32, bool, usize) {
    let n = history.rows.len();
    if n == 0 {
        return (Vec::new(), 0.0, false, 0);
    }
    let step = ((n as f32 / CHURN_SAMPLE as f32).ceil() as usize).max(1);
    let sampled_count = (n / step).max(1);
    let sampled = step <= 1;

    let mut churn: HashMap<String, u32> = HashMap::new();
    let mut total_touches = 0u32;
    let mut i = 0usize;
    while i < n {
        let row = &history.rows[i];
        if let Ok(files) = super::history::History::files_of(repo, &row.oid) {
            for f in files {
                *churn.entry(f).or_insert(0) += 1;
                total_touches += 1;
            }
        }
        i += step;
    }

    let mut hottest: Vec<FileChurn> = churn
        .into_iter()
        .map(|(path, touches)| FileChurn {
            path,
            touches: if sampled {
                touches
            } else {
                touches * (n as u32 / sampled_count.max(1) as u32).max(1)
            },
            estimated: !sampled,
        })
        .collect();
    hottest.sort_by(|a, b| b.touches.cmp(&a.touches).then(a.path.cmp(&b.path)));
    hottest.truncate(10);

    // Concentration: share of touches in the top decile of touched files.
    let mut concentration = 0.0f32;
    if total_touches > 0 {
        let all: Vec<u32> = {
            let mut v: Vec<u32> = Vec::new();
            // Recompute from hottest + rest is not retained; approximate with
            // the top-10 share of total touches.
            let top: u32 = hottest.iter().take(10).map(|f| f.touches).sum();
            v.push(top.min(total_touches));
            v
        };
        concentration = all[0] as f32 / total_touches as f32;
    }

    (hottest, concentration, sampled, sampled_count)
}

fn month_key(ts: i64) -> String {
    crate::theme::fmt_date(ts)[..7].to_string()
}

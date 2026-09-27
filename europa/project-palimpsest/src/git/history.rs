//! Bounded commit-history indexing.
//!
//! Policy: the application never retains unbounded history. The commit scan is
//! capped (`--limit`), per-commit change statistics are computed lazily in two
//! tiers, and window extraction is O(visible) via a time-sorted secondary
//! index. This is the application-policy workaround for the library's lack of
//! built-in virtualization (libgibson issue #38) — the pain is measured by
//! `--profile`.

use std::collections::{BTreeMap, HashMap, VecDeque};

use git2::Oid;

use super::repo::Repo;

/// One indexed commit. Kept deliberately small: this is the structure the UI
/// retains for thousands of rows.
#[derive(Clone)]
pub struct CommitRow {
    pub oid: String,
    pub short: String,
    pub summary: String,
    pub author: String,
    pub email: String,
    pub time: i64,
    /// Full parent oids, newest-first, capped at 2 recorded + count.
    pub parent1: Option<String>,
    pub parent2: Option<String>,
    pub parent_count: u8,
    /// Braid lane index (assigned by the lane scheduler).
    pub lane: u16,
    pub is_merge: bool,
    /// Cheap tier-A stats: changed-path count (delta enumeration only).
    pub files: Option<u32>,
    /// Tag names whose peeled target is this commit.
    pub tags: Vec<String>,
}

impl CommitRow {
    pub fn is_root(&self) -> bool {
        self.parent_count == 0
    }

    /// Visual mass from tier-A stats. Shape alone carries the mass in mono.
    pub fn mass_glyph(&self, selected: bool, search_hit: bool) -> &'static str {
        use crate::theme::glyphs;
        if selected && search_hit {
            return glyphs::NODE_SELECTED_SEARCH;
        }
        if selected {
            return glyphs::NODE_SELECTED;
        }
        if search_hit {
            return glyphs::NODE_SEARCH;
        }
        if self.is_merge {
            return glyphs::NODE_MERGE;
        }
        match self.files {
            Some(f) if f >= 10 => glyphs::NODE_HEAVY,
            Some(f) if f >= 3 => glyphs::NODE,
            Some(_) => glyphs::NODE_ROOT,
            None => glyphs::NODE,
        }
    }
}

/// Tier-B statistics (line counts), computed lazily per commit on demand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineStat {
    pub files: u32,
    pub adds: u32,
    pub dels: u32,
}

/// The bounded history index.
pub struct History {
    /// Rows, newest first (display order: index 0 = newest).
    pub rows: Vec<CommitRow>,
    /// oid -> row index.
    index: HashMap<String, u32>,
    /// (time asc, row idx) for O(log n) window extraction.
    sorted: Vec<(i64, u32)>,
    /// lane count in use.
    pub lanes: u16,
    /// oid of every branch tip we know, for fast tip lookup.
    pub tips: HashMap<String, Vec<String>>,
    /// Full history span (loaded portion).
    pub t_min: i64,
    pub t_max: i64,
    /// Commit density minimap: bucket index -> count (at most 240 buckets).
    pub density: Vec<(i64, u32)>,
    /// Seconds per density bucket (the `density` keys are sparse indices).
    pub density_width: i64,
    /// True when the walk stopped at the bound with more commits remaining.
    pub truncated: bool,
    /// Tier-B line-stat cache with a hard bound (LRU eviction).
    stat_cache: HashMap<String, LineStat>,
    stat_order: VecDeque<String>,
    /// Per-frame stat resolution budget (tier A).
    stat_budget: usize,
}

/// How much tier-A work (delta enumerations) one render pass may perform.
const STATS_PER_FRAME: usize = 24;
/// Hard bound on the tier-B LRU cache.
const STAT_CACHE_MAX: usize = 512;

impl History {
    /// Scan at most `limit` commits, newest first. Returns the index plus the
    /// number of commits the walk actually saw (for the truncation notice).
    pub fn load(repo: &Repo, limit: usize) -> Result<(Self, usize), String> {
        let mut walk = repo
            .inner()
            .revwalk()
            .map_err(|e| format!("revwalk: {}", e))?;
        walk.push_head()
            .or_else(|_| walk.push_glob("refs/*"))
            .map_err(|e| format!("no starting points: {}", e))?;
        let _ = walk.set_sorting(git2::Sort::TIME | git2::Sort::TOPOLOGICAL);

        let mut rows: Vec<CommitRow> = Vec::with_capacity(limit.min(8192));
        let mut seen = 0usize;
        let mut truncated = false;
        for item in walk {
            if rows.len() >= limit {
                // probe once more: the honest truncation flag
                truncated = true;
                break;
            }
            let oid = match item {
                Ok(o) => o,
                Err(_) => continue,
            };
            seen += 1;
            match Self::row_of(repo, oid) {
                Some(r) => rows.push(r),
                None => continue,
            }
        }

        let hist = Self::assemble(rows, truncated);
        Ok((hist, seen))
    }

    /// Assemble the index from scanned rows: braid lanes, oid index,
    /// time-sorted window key, density minimap. Split from [`History::load`]
    /// so tests can drive the pure indexing logic without a repository.
    fn assemble(rows: Vec<CommitRow>, truncated: bool) -> Self {
        let (rows, lanes, density, density_width) = Self::with_lanes_and_density(rows);
        let index: HashMap<String, u32> = rows
            .iter()
            .enumerate()
            .map(|(i, r)| (r.oid.clone(), i as u32))
            .collect();
        let mut sorted: Vec<(i64, u32)> = rows
            .iter()
            .enumerate()
            .map(|(i, r)| (r.time, i as u32))
            .collect();
        // Time ascending, stable on row order (newest-first rows ⇒ oldest last
        // wins ties deterministically).
        sorted.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));

        let t_min = rows.iter().map(|r| r.time).min().unwrap_or(0);
        let t_max = rows.iter().map(|r| r.time).max().unwrap_or(0);

        Self {
            rows,
            index,
            sorted,
            lanes,
            tips: HashMap::new(),
            t_min,
            t_max,
            density,
            density_width,
            truncated,
            stat_cache: HashMap::new(),
            stat_order: VecDeque::new(),
            stat_budget: STATS_PER_FRAME,
        }
    }

    fn row_of(repo: &Repo, oid: Oid) -> Option<CommitRow> {
        let commit = repo.inner().find_commit(oid).ok()?;
        let author = commit.author();
        let full = commit.id().to_string();
        let mut parent_iter = commit.parent_ids();
        let parent1 = parent_iter.next().map(|p| p.to_string());
        let parent2 = parent_iter.next().map(|p| p.to_string());
        let parent_count = commit.parent_count().min(u8::MAX as usize) as u8;
        let summary = commit.summary().unwrap_or("").to_string();
        // Truncation policy: store only the first 240 chars of summary.
        let summary = crate::theme::truncate(&summary, 240);
        Some(CommitRow {
            short: full.get(..7).unwrap_or("0000000").to_string(),
            oid: full,
            summary,
            author: author.name().unwrap_or("?").to_string(),
            email: author.email().unwrap_or("").to_string(),
            time: commit.time().seconds(),
            parent1,
            parent2,
            parent_count,
            lane: 0,
            is_merge: parent_count > 1,
            files: None,
            tags: Vec::new(),
        })
    }

    /// Lane scheduling (the braid algorithm) + density minimap.
    ///
    /// Walks newest -> oldest. A commit claimed by a child keeps the lowest
    /// lane that claimed it; unclaimed commits take a free lane. First parents
    /// inherit the child's lane; extra parents fork into fresh lanes. Rails
    /// are later drawn per edge, so lane reuse across distant eras never draws
    /// a false connection.
    fn with_lanes_and_density(
        mut rows: Vec<CommitRow>,
    ) -> (Vec<CommitRow>, u16, Vec<(i64, u32)>, i64) {
        let n = rows.len();
        let mut lanes_claim: HashMap<&str, Vec<u16>> = HashMap::new();
        let mut free: Vec<u16> = Vec::new();
        let mut next_lane: u16 = 0;

        for row in rows.iter_mut() {
            let oid = row.oid.as_str();
            let claimed = lanes_claim.remove(oid).unwrap_or_default();
            let lane = if let Some(min) = claimed.iter().copied().min() {
                for l in claimed.into_iter() {
                    if l != min {
                        free.push(l);
                    }
                }
                min
            } else if let Some(l) = free.pop() {
                l
            } else {
                next_lane += 1;
                next_lane - 1
            };
            row.lane = lane;
            if let Some(p1) = row.parent1.as_deref() {
                if row.parent_count >= 1 {
                    lanes_claim.entry(p1).or_default().push(lane);
                }
            }
            if let Some(p2) = row.parent2.as_deref() {
                let nl = free.pop().unwrap_or_else(|| {
                    next_lane += 1;
                    next_lane - 1
                });
                lanes_claim.entry(p2).or_default().push(nl);
            }
        }
        let lanes = next_lane.max(1);

        // Density minimap: bucket by span into <= 240 buckets. Keys are sparse
        // indices (seconds-per-bucket persisted separately for reconstruction).
        let mut density: Vec<(i64, u32)> = Vec::new();
        let mut density_width: i64 = 1;
        if n > 0 {
            let t_min = rows.iter().map(|r| r.time).min().unwrap_or(0);
            let t_max = rows
                .iter()
                .map(|r| r.time)
                .max()
                .unwrap_or(1)
                .max(t_min + 1);
            let span = (t_max - t_min).max(1);
            let buckets = 240u32;
            let width = (span / buckets as i64 + 1).max(1);
            density_width = width;
            let mut counts: BTreeMap<i64, u32> = BTreeMap::new();
            for r in &rows {
                *counts.entry((r.time - t_min) / width).or_insert(0) += 1;
            }
            density = counts.into_iter().collect();
        }

        (rows, lanes, density, density_width)
    }

    /// Enrich rows with branch-tip and tag knowledge (labels for the atlas).
    pub fn enrich(&mut self, branches: &[super::repo::BranchInfo], tags: &[super::repo::TagInfo]) {
        self.tips.clear();
        for b in branches {
            if self.index.contains_key(&b.oid) {
                self.tips
                    .entry(b.oid.clone())
                    .or_default()
                    .push(b.name.clone());
            }
        }
        for t in tags {
            if let Some(idx) = self.index.get(&t.oid) {
                self.rows[*idx as usize].tags.push(t.name.clone());
            }
        }
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn idx_of(&self, oid: &str) -> Option<u32> {
        self.index.get(oid).copied()
    }

    /// Row indices whose commit time lies in `[t0, t1]` (inclusive),
    /// ordered oldest-first (left to right on the timeline).
    pub fn window(&self, t0: i64, t1: i64) -> Vec<u32> {
        let lo = self.sorted.partition_point(|(t, _)| *t < t0);
        let hi = self.sorted.partition_point(|(t, _)| *t <= t1);
        let mut out: Vec<u32> = self.sorted[lo.min(hi)..hi]
            .iter()
            .map(|(_, i)| *i)
            .collect();
        out.sort_by_key(|&i| self.rows[i as usize].time);
        out
    }

    /// Resolve tier-A stats (changed-path count) for up to `budget` unstat'ed
    /// commits among `window`, mutating rows in place. Returns the number
    /// resolved this pass (used by `--profile` to measure the pain).
    pub fn resolve_window_stats(&mut self, repo: &Repo, window: &[u32]) -> usize {
        let mut resolved = 0usize;
        for &i in window {
            if resolved >= self.stat_budget {
                break;
            }
            if self.rows[i as usize].files.is_some() {
                continue;
            }
            match Self::count_files(repo, &self.rows[i as usize].oid) {
                Ok(f) => {
                    self.rows[i as usize].files = Some(f);
                    resolved += 1;
                }
                Err(_) => {
                    self.rows[i as usize].files = Some(0);
                    resolved += 1;
                }
            }
        }
        resolved
    }

    /// Cheap tier-A stat: number of changed paths vs first parent.
    fn count_files(repo: &Repo, oid: &str) -> Result<u32, String> {
        let paths = Self::files_of(repo, oid)?;
        Ok(paths.len() as u32)
    }

    /// Changed paths of a commit vs its first parent (delta enumeration only:
    /// no blob contents, so this is the cheap tier). Renames collapse to the
    /// new path.
    pub fn files_of(repo: &Repo, oid: &str) -> Result<Vec<String>, String> {
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
        let mut opts = git2::DiffOptions::new();
        opts.include_typechange(true);
        let diff = repo_i
            .diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), Some(&mut opts))
            .map_err(|e| format!("diff: {}", e))?;
        Ok(diff
            .deltas()
            .filter_map(|d| {
                d.new_file()
                    .path()
                    .or_else(|| d.old_file().path())
                    .map(|p| p.to_string_lossy().into_owned())
            })
            .collect())
    }

    /// Tier-B stat: changed files + insertions + deletions (blob-level diff).
    /// Cached with an LRU bound. Only invoked for inspected commits.
    pub fn line_stat(&mut self, repo: &Repo, oid: &str) -> Option<LineStat> {
        if let Some(s) = self.stat_cache.get(oid) {
            return Some(*s);
        }
        let stat = super::diff::commit_line_stat(repo, oid)?;
        if self.stat_cache.len() >= STAT_CACHE_MAX {
            if let Some(evict) = self.stat_order.pop_front() {
                self.stat_cache.remove(&evict);
            }
        }
        self.stat_cache.insert(oid.to_string(), stat);
        self.stat_order.push_back(oid.to_string());
        Some(stat)
    }

    /// Search over summary, author, oid (case-insensitive substring).
    /// Returns row indices, newest first.
    pub fn search(&self, q: &str) -> Vec<u32> {
        let needle = q.to_lowercase();
        if needle.is_empty() {
            return Vec::new();
        }
        self.rows
            .iter()
            .enumerate()
            .filter(|(_, r)| {
                r.summary.to_lowercase().contains(&needle)
                    || r.author.to_lowercase().contains(&needle)
                    || r.oid.to_lowercase().contains(&needle)
            })
            .map(|(i, _)| i as u32)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(oid: &str, parents: &[&str], time: i64) -> CommitRow {
        let ps: Vec<String> = parents.iter().map(|p| p.to_string()).collect();
        CommitRow {
            oid: oid.to_string(),
            short: oid[..7.min(oid.len())].to_string(),
            summary: format!("commit {}", oid),
            author: "A".to_string(),
            email: String::new(),
            time,
            parent1: ps.first().cloned(),
            parent2: ps.get(1).cloned(),
            parent_count: ps.len() as u8,
            lane: 0,
            is_merge: ps.len() > 1,
            files: None,
            tags: Vec::new(),
        }
    }

    fn lanes_of(rows: Vec<CommitRow>) -> (Vec<CommitRow>, u16) {
        let (rows, lanes, density, width) = History::with_lanes_and_density(rows);
        assert!(density.is_empty() || width >= 1, "density width sane");
        (rows, lanes)
    }

    #[test]
    fn linear_history_uses_a_single_lane() {
        // newest first: C3 <- C2 <- C1
        let (rows, lanes) = lanes_of(vec![
            row("C3C3C3C3", &["C2C2C2C2"], 30),
            row("C2C2C2C2", &["C1C1Q1Q1"], 20),
            row("C1C1Q1Q1", &[], 10),
        ]);
        assert_eq!(lanes, 1, "a straight line braids into one lane");
        assert!(rows.iter().all(|r| r.lane == 0));
    }

    #[test]
    fn fork_and_merge_uses_two_lanes() {
        //       M (merge)
        //      / \
        //     T2  F
        //      \  /
        //       T1
        //       |
        //       R
        // newest first
        let rows = vec![
            row("MMMMMMMM", &["T2T2T2T2", "FFFFFFFF"], 50),
            row("T2T2T2T2", &["T1T1T1T1"], 40),
            row("FFFFFFFF", &["T1T1T1T1"], 30),
            row("T1T1T1T1", &["RRRRRRRR"], 20),
            row("RRRRRRRR", &[], 10),
        ];
        let (rows, lanes) = lanes_of(rows);
        assert_eq!(lanes, 2, "one fork = one extra lane");
        let m = rows.iter().find(|r| r.oid == "MMMMMMMM").unwrap();
        assert_eq!(m.lane, 0, "the merge stays on the trunk lane");
        assert!(m.is_merge);
        let f = rows.iter().find(|r| r.oid == "FFFFFFFF").unwrap();
        assert_eq!(f.lane, 1, "the side strand takes the fresh lane");
        let t1 = rows.iter().find(|r| r.oid == "T1T1T1T1").unwrap();
        assert_eq!(t1.lane, 0, "first parent inherits the child's lane");
    }

    #[test]
    fn sequential_features_reuse_the_side_lane() {
        // Two sequential features: lane 1 frees after the first merge and is
        // claimed again by the second strand. Max lanes stays 2.
        let rows = vec![
            row("M2M2M2M2", &["T4T4T4T4", "G2G2G2G2"], 90),
            row("G2G2G2G2", &["T4T4T4T4"], 80),
            row("T4T4T4T4", &["M1M1M1M1"], 70),
            row("M1M1M1M1", &["T2T2T2T2", "G1G1G1G1"], 60),
            row("G1G1G1G1", &["T2T2T2T2"], 50),
            row("T2T2T2T2", &["R0R0R0R0"], 40),
            row("R0R0R0R0", &[], 30),
        ];
        let (rows, lanes) = lanes_of(rows);
        assert_eq!(lanes, 2, "sequential strands must reuse the freed lane");
        let g1 = rows.iter().find(|r| r.oid == "G1G1G1G1").unwrap();
        let g2 = rows.iter().find(|r| r.oid == "G2G2G2G2").unwrap();
        assert_eq!(g1.lane, g2.lane, "both strands ride the same reused lane");
        assert_eq!(g1.lane, 1);
    }

    #[test]
    fn concurrent_strands_get_distinct_lanes() {
        // Trunk T1..T6 runs continuously; strand A (forks at T2, merges at
        // M1) and strand B (forks at T2, merges at M2) are both alive while
        // the trunk advances — three world-lines at once:
        //
        //   R─T1─T2─T3─T4─T5─M1─T6─M2      (trunk, lane 0)
        //            └─A4──┘  (merges at M1, lane 2)
        //            └─B4───────┘  (merges at M2, lane 1)
        let rows = vec![
            row("M2M2M2M2", &["T6T6T6T6", "B4B4B4B4"], 100),
            row("T6T6T6T6", &["M1M1M1M1"], 90),
            row("M1M1M1M1", &["T5T5T5T5", "A4A4A4A4"], 80),
            row("T5T5T5T5", &["T4T4T4T4"], 70),
            row("B4B4B4B4", &["T2T2T2T2"], 65),
            row("T4T4T4T4", &["T3T3T3T3"], 60),
            row("A4A4A4A4", &["T2T2T2T2"], 55),
            row("T3T3T3T3", &["T2T2T2T2"], 50),
            row("T2T2T2T2", &["T1T1T1T1"], 40),
            row("T1T1T1T1", &["R0R0R0R0"], 30),
            row("R0R0R0R0", &[], 20),
        ];
        let (rows, lanes) = lanes_of(rows);
        assert_eq!(
            lanes, 3,
            "trunk + two concurrent strands need exactly 3 lanes"
        );
        let lane_of = |oid: &str| rows.iter().find(|r| r.oid == oid).unwrap().lane;
        assert_eq!(lane_of("A4A4A4A4"), 2, "strand A gets its own lane");
        assert_eq!(lane_of("B4B4B4B4"), 1, "strand B gets its own lane");
        assert_eq!(lane_of("T2T2T2T2"), 0, "the trunk keeps lane 0");
        assert_eq!(lane_of("M1M1M1M1"), 0, "merges land on the trunk lane");
        assert_eq!(lane_of("M2M2M2M2"), 0);
    }

    #[test]
    fn density_buckets_cover_the_span() {
        let rows: Vec<CommitRow> = (0..30)
            .map(|i| row(&format!("ID{}/{}", i, "000000"), &[], i * 86_400))
            .collect();
        let (_, _, density, width) = History::with_lanes_and_density(rows);
        assert!(!density.is_empty(), "density must bucket commits");
        let total: u32 = density.iter().map(|(_, c)| *c).sum();
        assert_eq!(total, 30, "every commit lands in exactly one bucket");
        assert!(density.len() <= 240, "at most 240 buckets");
        assert!(width >= 1);
        // keys are strictly increasing (BTreeMap order)
        let keys: Vec<i64> = density.iter().map(|(k, _)| *k).collect();
        let mut sorted = keys.clone();
        sorted.sort_unstable();
        assert_eq!(keys, sorted);
    }

    #[test]
    fn search_matches_summary_author_and_oid_case_insensitively() {
        let hist = History::assemble(
            vec![
                CommitRow {
                    summary: "feat: add the FROBNICATOR".to_string(),
                    author: "Grace Hopper".to_string(),
                    ..row("BBBB0002", &["AAAA0001"], 20)
                },
                CommitRow {
                    summary: "docs: explain anchors".to_string(),
                    author: "Ada Lovelace".to_string(),
                    ..row("AAAA0001", &[], 10)
                },
            ],
            false,
        );
        assert_eq!(
            hist.search("frobnicator"),
            vec![0],
            "summary match, any case"
        );
        assert_eq!(hist.search("GRACE"), vec![0], "author match, any case");
        assert_eq!(hist.search("aaaa000"), vec![1], "oid prefix match");
        assert!(hist.search("").is_empty(), "empty query matches nothing");
        assert!(hist.search("zzz").is_empty());
        // hits are row indices, i.e. newest first
        assert_eq!(hist.search("a"), vec![0, 1], "matches both, newest first");
    }

    #[test]
    fn window_extraction_is_inclusive_and_sorted() {
        let rows: Vec<CommitRow> = (0..10)
            .map(|i| row(&format!("TT{}00000", i), &[], i * 100))
            .collect();
        let hist = History::assemble(rows, false);
        let win = hist.window(200, 500);
        // times 200,300,400,500 inclusive => rows for i = 2..=5 (times 200..500)
        assert_eq!(win.len(), 4, "bounds are inclusive");
        let times: Vec<i64> = win.iter().map(|&i| hist.rows[i as usize].time).collect();
        assert_eq!(times, vec![200, 300, 400, 500], "oldest first");
        // window is a small slice even with the full span
        assert!(hist.window(0, 900).len() == 10);
        assert!(hist.window(1000, 2000).is_empty(), "outside range is empty");
    }

    #[test]
    fn idx_of_and_truncation_flag() {
        let hist = History::assemble(
            vec![row("BBBB0002", &["AAAA0001"], 20), row("AAAA0001", &[], 10)],
            true,
        );
        assert_eq!(hist.idx_of("AAAA0001"), Some(1));
        assert_eq!(hist.idx_of("nope"), None);
        assert!(hist.truncated, "truncation flag survives assembly");
        assert_eq!(hist.t_min, 10);
        assert_eq!(hist.t_max, 20);
    }
}

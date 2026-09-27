//! Deterministic synthetic git fixtures.
//!
//! Everything is built in-process with git2 (no `git` binary, no network,
//! no randomness beyond a seeded LCG), with fixed author/committer clocks,
//! so every build of a profile yields byte-identical history — stable visual
//! structure for tests and `--demo`.

use std::path::Path;

use git2::{Oid, Repository, Signature, TreeBuilder};

use crate::git::repo::Repo;

pub struct FixtureSpec {
    pub profile: &'static str,
    pub main_commits: usize,
    pub feature_branches: usize,
    /// Commits per feature branch.
    pub per_branch: usize,
    pub hot_files: usize,
    pub start_time: i64,
    /// Seconds between commits (base cadence).
    pub cadence: i64,
}

impl FixtureSpec {
    pub fn tiny() -> Self {
        Self {
            profile: "tiny",
            main_commits: 12,
            feature_branches: 2,
            per_branch: 4,
            hot_files: 2,
            start_time: 1_700_000_000,
            cadence: 43_200, // 12h
        }
    }

    pub fn medium() -> Self {
        Self {
            profile: "medium",
            main_commits: 90,
            feature_branches: 4,
            per_branch: 14,
            hot_files: 4,
            start_time: 1_704_067_200, // 2024-01-01
            cadence: 21_600,           // 6h
        }
    }

    pub fn large() -> Self {
        Self {
            profile: "large",
            main_commits: 1_600,
            feature_branches: 8,
            per_branch: 160,
            hot_files: 8,
            start_time: 1_609_459_200, // 2021-01-01
            cadence: 10_800,           // 3h
        }
    }
}

/// A tiny seeded LCG so "jitter" is deterministic.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 16
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
}

const AUTHORS: [(&str, &str); 4] = [
    ("Ada Lovelace", "ada@palimpsest.dev"),
    ("Grace Hopper", "grace@palimpsest.dev"),
    ("Alan Turing", "alan@palimpsest.dev"),
    ("Edsger Dijkstra", "edsger@palimpsest.dev"),
];

struct Builder<'r> {
    repo: &'r Repository,
    time: i64,
    rng: Lcg,
}

impl<'r> Builder<'r> {
    fn sig(&self, author_idx: usize) -> Signature<'static> {
        let (name, email) = AUTHORS[author_idx % AUTHORS.len()];
        Signature::new(name, email, &git2::Time::new(self.time, 0)).expect("signature")
    }

    fn commit_tree(
        &self,
        refname: Option<&str>,
        author: usize,
        msg: &str,
        tree: &Oid,
        parents: &[&git2::Commit],
    ) -> Oid {
        let sig = self.sig(author);
        let parent_refs: Vec<&git2::Commit> = parents.to_vec();
        self.repo
            .commit(
                refname,
                &sig,
                &sig,
                msg,
                &self.repo.find_tree(*tree).unwrap(),
                &parent_refs,
            )
            .expect("fixture commit")
    }

    /// Build a tree from (path, content) pairs; nested paths become subtrees.
    fn tree_of(&self, files: &[(&str, String)]) -> Oid {
        fn build<'r>(
            repo: &'r Repository,
            prefix: &str,
            files: &[(&str, String)],
        ) -> TreeBuilder<'r> {
            let mut tb = repo.treebuilder(None).expect("treebuilder");
            // Group by the first path segment.
            let mut dirs: std::collections::BTreeMap<String, Vec<(&str, String)>> =
                Default::default();
            let mut blobs: Vec<(&str, String)> = Vec::new();
            for (path, content) in files {
                if let Some((head, tail)) = path.split_once('/') {
                    dirs.entry(head.to_string())
                        .or_default()
                        .push((tail, content.clone()));
                } else {
                    blobs.push((path, content.clone()));
                }
            }
            for (dir, sub) in dirs {
                let sub_prefix = if prefix.is_empty() {
                    dir.clone()
                } else {
                    format!("{}/{}", prefix, dir)
                };
                let sub_tree = build(repo, &sub_prefix, &sub).write().unwrap();
                tb.insert(&dir, sub_tree, 0o040000).unwrap();
            }
            for (name, content) in blobs {
                let oid = repo.blob(content.as_bytes()).unwrap();
                tb.insert(name, oid, 0o100644).unwrap();
            }
            tb
        }
        build(self.repo, "", files).write().unwrap()
    }
}

/// Build the fixture at `root/<profile>-repo` and return the repo path.
///
/// Deterministic by construction, so an existing fixture is REUSED: the
/// directory is only (re)created when missing. A marker file pins the
/// profile. A process-global lock serializes concurrent builders (tests run
/// in parallel threads).
#[allow(clippy::too_many_arguments)]
pub fn build(root: &Path, spec: &FixtureSpec) -> Result<std::path::PathBuf, String> {
    static BUILD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = BUILD_LOCK.lock().unwrap_or_else(|p| p.into_inner());

    let dir = root.join(format!("palimpsest-fixture-{}", spec.profile));
    let marker = dir.join(".palimpsest-fixture");
    if marker.exists() {
        if let Ok(m) = std::fs::read_to_string(&marker) {
            if m.trim() == spec.profile {
                return Ok(dir);
            }
        }
    }
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| format!("clean {}: {}", dir.display(), e))?;
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("mkdir {}: {}", dir.display(), e))?;

    let git2repo = Repository::init(&dir).map_err(|e| format!("init: {}", e))?;
    // Favor speed over size for fixture object writes.
    if let Ok(mut cfg) = git2repo.config() {
        let _ = cfg.set_i32("core.compression", 1);
    }
    // HEAD must point at the branch we commit to, regardless of the host's
    // init.defaultBranch configuration.
    git2repo
        .set_head("refs/heads/main")
        .map_err(|e| format!("set head: {}", e))?;
    let mut b = Builder {
        repo: &git2repo,
        time: spec.start_time,
        rng: Lcg(0x5EED_1234_ABCD_0001),
    };

    // ---- initial state -----------------------------------------------------
    let readme = format!(
        "# Fixture repository ({})\n\nDeterministic synthetic history for PALIMPSEST.\nBuilt in-process with libgit2.\n",
        spec.profile
    );
    let files: Vec<(&str, String)> = vec![
        ("README.md", readme),
        (
            "src/main.rs",
            "fn main() {\n    println!(\"palimpsest fixture\");\n}\n".to_string(),
        ),
        (
            "src/core.rs",
            "pub fn anchor(a: i64, b: i64) -> i64 {\n    a.min(b)\n}\n".to_string(),
        ),
    ];
    let tree = b.tree_of(&files);
    let first = b.commit_tree(
        Some("refs/heads/main"),
        0,
        " genesis: establish the workbench",
        &tree,
        &[],
    );
    let first_commit = git2repo.find_commit(first).unwrap();

    let mut head = first_commit;
    let mut next_id = 1usize;

    let bump = |b: &mut Builder| {
        b.time += b.rng.below(spec.cadence as u64 / 2) as i64 + spec.cadence / 2;
    };

    // ---- hot files churn (they get touched throughout) ----------------------
    let mut hot_contents: Vec<String> = Vec::new();
    for i in 0..spec.hot_files {
        hot_contents.push(format!(
            "// hot module {i}: evolves constantly\npub fn stage_{i}() -> i32 {{\n    {i} * 7\n}}\n"
        ));
    }

    let mut main_files = files.clone();

    // ---- interleaved braid structure -----------------------------------------
    // All strands fork early from staggered trunk points, commits go
    // round-robin across strands (so they interleave in time), and merges
    // come back one by one. This produces genuinely concurrent world-lines:
    // long-lived parallel lanes the atlas can braid.

    struct Strand {
        refname: String,
        head: Oid,
        files: Vec<(&'static str, String)>,
        count: usize,
    }
    let mut strands: Vec<Strand> = Vec::new();
    let mut trunk_i = 0usize;

    // Phase A: trunk prelude, forking strand k every few commits
    for k in 0..spec.feature_branches {
        for _ in 0..3 {
            let oid = trunk_commit_oid(
                &mut b,
                &git2repo,
                &mut main_files,
                &mut hot_contents,
                spec.hot_files,
                trunk_i,
                head.id(),
                &mut next_id,
            );
            head = git2repo.find_commit(oid).unwrap();
            trunk_i += 1;
        }
        let branch_name = format!("feature/strand-{}", k + 1);
        strands.push(Strand {
            refname: format!("refs/heads/{}", branch_name),
            head: head.id(),
            files: main_files.clone(),
            count: 0,
        });
    }

    // Phase B: round-robin commits across all strands, with trunk commits
    // interleaved so the braids stay temporally adjacent.
    #[allow(clippy::needless_range_loop)]
    loop {
        let mut all_done = true;
        for si in 0..strands.len() {
            if strands[si].count < spec.per_branch {
                all_done = false;
                strands[si].count += 1;
                let j = strands[si].count;
                bump(&mut b);
                let fpath = alloc(&mut next_id);
                let fname = leak(format!("src/modules/{}.rs", fpath));
                let content = module_source(&mut b.rng, fpath, j);
                strands[si].files.push((fname, content));
                let t = b.tree_of(&strands[si].files);
                let author = (si + j) % AUTHORS.len();
                let msg = format!(
                    " feat(strand-{}): {} module {}",
                    si + 1,
                    verb(&mut b.rng, j),
                    j
                );
                let parent = git2repo.find_commit(strands[si].head).unwrap();
                let c = b.commit_tree(
                    Some(&strands[si].refname.clone()),
                    author,
                    &msg,
                    &t,
                    &[&parent],
                );
                strands[si].head = c;
            }
        }
        if all_done {
            break;
        }
        if trunk_i < spec.main_commits {
            let oid = trunk_commit_oid(
                &mut b,
                &git2repo,
                &mut main_files,
                &mut hot_contents,
                spec.hot_files,
                trunk_i,
                head.id(),
                &mut next_id,
            );
            head = git2repo.find_commit(oid).unwrap();
            trunk_i += 1;
        }
    }

    // Phase C: merge strands back one at a time, with trunk commits between
    #[allow(clippy::needless_range_loop)]
    for si in 0..strands.len() {
        if trunk_i < spec.main_commits {
            let oid = trunk_commit_oid(
                &mut b,
                &git2repo,
                &mut main_files,
                &mut hot_contents,
                spec.hot_files,
                trunk_i,
                head.id(),
                &mut next_id,
            );
            head = git2repo.find_commit(oid).unwrap();
            trunk_i += 1;
        }
        let strand_head = git2repo.find_commit(strands[si].head).unwrap();
        let merged = union_tree(&b, &main_files, &strands[si].files);
        let merge_msg = format!(" merge: braid strand-{} into main", si + 1);
        let author = si % AUTHORS.len();
        let mc = b.commit_tree(
            Some("refs/heads/main"),
            author,
            &merge_msg,
            &merged,
            &[&head, &strand_head],
        );
        head = git2repo.find_commit(mc).unwrap();
        let additions: Vec<(&str, String)> = strands[si]
            .files
            .iter()
            .filter(|(p, _)| !main_files.iter().any(|(q, _)| *q == *p))
            .cloned()
            .collect();
        main_files.extend(additions);
    }

    // Phase D: remaining trunk commits
    while trunk_i < spec.main_commits {
        let oid = trunk_commit_oid(
            &mut b,
            &git2repo,
            &mut main_files,
            &mut hot_contents,
            spec.hot_files,
            trunk_i,
            head.id(),
            &mut next_id,
        );
        head = git2repo.find_commit(oid).unwrap();
        trunk_i += 1;
    }

    // ---- a rename event -----------------------------------------------------
    bump(&mut b);
    let before = main_files.clone();
    main_files.retain(|(p, _)| *p != "src/core.rs");
    main_files.push((
        leak("src/foundation.rs".to_string()),
        "pub fn anchor(a: i64, b: i64) -> i64 {\n    a.min(b)\n}\n\npub fn span(a: i64, b: i64) -> i64 {\n    (a - b).abs()\n}\n".to_string(),
    ));
    let rt = b.tree_of(&main_files);
    let rc = b.commit_tree(
        Some("refs/heads/main"),
        1,
        " refactor: rename src/core.rs to src/foundation.rs",
        &rt,
        &[&head],
    );
    head = git2repo.find_commit(rc).unwrap();
    let _ = before;

    // ---- a long-lived parallel lane that never merges ------------------------
    // Fork from an early trunk commit so the atlas shows a long parallel
    // world-line running beneath the merged braids.
    if let Some(early) = early_commit(&git2repo, 4) {
        let lane_ref = "refs/heads/exp/never-merges";
        let _ = git2repo.reference(lane_ref, early.id(), false, "fixture lane fork");
        let mut lane_files: Vec<(&str, String)> = vec![(
            "experiments/sandbox.rs",
            "pub fn trial() -> bool {\n    true\n}\n".to_string(),
        )];
        let mut lane_head = early.clone();
        let per = spec.per_branch.clamp(3, 10);
        for j in 0..per {
            bump(&mut b);
            let fpath = alloc(&mut next_id);
            lane_files.push((
                leak(format!("experiments/{}.rs", fpath)),
                format!("pub fn exp_{}() -> i32 {{\n    {} * 3\n}}\n", fpath, j),
            ));
            let lt = b.tree_of(&lane_files);
            let msg = format!(" exp: exploration {}", j);
            let c = b.commit_tree(Some(lane_ref), 2, &msg, &lt, &[&lane_head]);
            lane_head = git2repo.find_commit(c).unwrap();
        }
    }

    // ---- tags ---------------------------------------------------------------
    let n_commits = count_commits(&git2repo);
    let tag_points = tag_points(&git2repo, n_commits);
    for (name, oid) in tag_points {
        let obj = git2repo.find_object(oid, None).unwrap();
        let _target = obj.peel(git2::ObjectType::Commit).unwrap();
        let sig = b.sig(0);
        let _tag = git2repo
            .tag(
                name.as_str(),
                &obj,
                &sig,
                &format!("release {}", name),
                false,
            )
            .map(|_| ());
        let _ = git2repo.find_commit(oid).unwrap();
    }

    // End the borrows `b` and `head` hold over the repository before the
    // handle is released. `Builder`/`Commit` have no destructors, so this is
    // purely a lifetime-ending `drop` — exactly what is needed here.
    #[allow(clippy::drop_non_drop)]
    {
        drop(b);
        drop(head);
    }
    let path = dir.clone();
    drop(git2repo);

    // The marker is only written after a complete, verified build so a killed
    // build can never poison the fixture cache.
    std::fs::write(&marker, spec.profile).map_err(|e| format!("marker: {}", e))?;

    // sanity: open through the app's own handle
    Repo::open(&path).map(|_| path)
}

fn module_source(rng: &mut Lcg, id: usize, j: usize) -> String {
    let lines = 4 + rng.below(6);
    let mut body = String::new();
    for l in 0..lines {
        body.push_str(&format!("    let v{} = {} + {};\n", l, id, j + l as usize));
    }
    format!("pub fn module_{}(x: i32) -> i32 {{\n{}}}\n", id, body)
}

/// One trunk commit: advances time, evolves a hot file, commits to main.
#[allow(clippy::too_many_arguments)]
fn trunk_commit_oid(
    b: &mut Builder,
    repo: &Repository,
    main_files: &mut Vec<(&'static str, String)>,
    hot_contents: &mut [String],
    hot_files: usize,
    i: usize,
    parent: Oid,
    _next_id: &mut usize,
) -> Oid {
    b.time += b.rng.below(43_200) as i64 + 43_200 / 2;
    let hot_idx = if hot_files > 0 { i % hot_files } else { 0 };
    if !hot_contents.is_empty() {
        hot_contents[hot_idx] = evolve_source(&mut b.rng, hot_idx, &hot_contents[hot_idx], i);
    }
    let hot_name = leak(format!("src/hot/module_{}.rs", hot_idx));
    main_files.retain(|(p, _)| *p != hot_name);
    if !hot_contents.is_empty() {
        main_files.push((hot_name, hot_contents[hot_idx].clone()));
    }
    let msg = match b.rng.below(7) {
        0 => format!(" fix(core): repair invariant {}", i),
        1 => format!(" docs: expand notes for stage {}", i),
        2 => format!(" refactor: fold stage {} helpers", i),
        3 => format!(" perf(hot): tighten loop {}", i),
        4 => format!(" chore: dependencies {}", i),
        _ => format!(" feat: extend stage {}", i),
    };
    let author = (i + b.rng.below(3) as usize) % AUTHORS.len();
    let t = b.tree_of(main_files);
    let parent_commit = repo.find_commit(parent).unwrap();
    b.commit_tree(Some("refs/heads/main"), author, &msg, &t, &[&parent_commit])
}

fn evolve_source(rng: &mut Lcg, idx: usize, prev: &str, i: usize) -> String {
    // grow the hot file slowly
    let extra = match rng.below(4) {
        0 => format!("pub fn extra_{}() -> i32 {{ {} }}\n", i, idx),
        1 => format!("pub const STAGE_{}: i32 = {};\n", i, i),
        _ => String::new(),
    };
    if extra.is_empty() {
        prev.to_string()
    } else {
        format!("{}{}", prev, extra)
    }
}

fn verb(rng: &mut Lcg, _j: usize) -> &'static str {
    match rng.below(5) {
        0 => "grow",
        1 => "extend",
        2 => "wire",
        3 => "polish",
        _ => "stage",
    }
}

/// Stable id allocator for module names.
fn alloc(next: &mut usize) -> usize {
    *next += 1;
    *next
}

/// Leak a String into a &'static str: fixture file sets are small and the
/// process is short-lived; the simplicity is worth it here (this is the only
/// place in the app that leaks, and it is bounded by the fixture size).
fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

fn union_tree(b: &Builder, main: &[(&str, String)], branch: &[(&str, String)]) -> Oid {
    let mut all: Vec<(&str, String)> = main.to_vec();
    for (p, c) in branch {
        if !all.iter().any(|(q, _)| *q == *p) {
            all.push((p, c.clone()));
        }
    }
    b.tree_of(&all)
}

fn count_commits(repo: &Repository) -> usize {
    let mut walk = repo.revwalk().unwrap();
    let _ = walk.push_head();
    walk.count()
}

fn early_commit<'r>(repo: &'r Repository, nth_from_start: usize) -> Option<git2::Commit<'r>> {
    // walk oldest-first
    let mut walk = repo.revwalk().ok()?;
    let _ = walk.push_head();
    let _ = walk.set_sorting(git2::Sort::REVERSE | git2::Sort::TOPOLOGICAL);
    for (i, item) in walk.enumerate() {
        if i == nth_from_start {
            let oid = item.ok()?;
            return repo.find_commit(oid).ok();
        }
    }
    None
}

fn tag_points(repo: &Repository, total: usize) -> Vec<(String, Oid)> {
    let mut walk = repo.revwalk().unwrap();
    let _ = walk.push_head();
    let _ = walk.set_sorting(git2::Sort::REVERSE | git2::Sort::TOPOLOGICAL);
    let oids: Vec<Oid> = walk.flatten().collect();
    let mut out = Vec::new();
    let marks = [0.25f64, 0.55, 0.9];
    for (k, frac) in marks.iter().enumerate() {
        let idx = ((total as f64) * frac) as usize;
        if let Some(oid) = oids.get(idx) {
            out.push((format!("v0.{}.0", k + 1), *oid));
        }
    }
    out
}

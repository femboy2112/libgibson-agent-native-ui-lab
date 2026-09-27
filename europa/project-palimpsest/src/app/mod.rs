//! Application state machine: views, camera, input modes, scrollback policy.

pub mod keys;
pub mod lens;
pub mod search;

use std::time::{Duration, Instant};

use gibson::focus::{FocusId, FocusRing};

use crate::git::blame::BlameData;
use crate::git::diff::CommitDiff;
use crate::git::filelog::FileEvent;
use crate::git::history::History;
use crate::git::metrics::Metrics;
use crate::git::repo::{BranchInfo, HeadInfo, Repo, TagInfo};
use crate::theme::Palette;

/// The five major surfaces of the workstation. They are lenses over the same
/// object, not unrelated screens.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum View {
    Atlas,
    Strata,
    Lens,
    Provenance,
    Health,
}

impl View {
    pub fn title(&self) -> &'static str {
        match self {
            View::Atlas => "HISTORY ATLAS",
            View::Strata => "FILE STRATA",
            View::Lens => "DIFF LENS",
            View::Provenance => "PROVENANCE",
            View::Health => "REPOSITORY HEALTH",
        }
    }

    /// Compact identity code for constrained layouts.
    pub fn short(&self) -> &'static str {
        match self {
            View::Atlas => "ATLAS",
            View::Strata => "STRATA",
            View::Lens => "LENS",
            View::Provenance => "PROV",
            View::Health => "HEALTH",
        }
    }

    pub fn key(&self) -> &'static str {
        match self {
            View::Atlas => "1",
            View::Strata => "2",
            View::Lens => "3",
            View::Provenance => "4",
            View::Health => "5",
        }
    }

    pub fn from_num(n: u8) -> Option<Self> {
        match n {
            1 => Some(View::Atlas),
            2 => Some(View::Strata),
            3 => Some(View::Lens),
            4 => Some(View::Provenance),
            5 => Some(View::Health),
            _ => None,
        }
    }
}

/// Which interactive mode owns the keyboard right now.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InputMode {
    Normal,
    Search,
    Command,
    HelpOverlay,
    FileBrowser,
}

/// Atlas camera: the multiscale time-lens position. `px_per_day` is the zoom;
/// `t_center` is the unix time at the center of the viewport.
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub px_per_day: f32,
    pub t_center: i64,
}

impl Camera {
    pub fn days_visible(&self, width: u16) -> f32 {
        width as f32 / self.px_per_day.max(0.001)
    }

    pub fn t_left(&self, width: u16) -> i64 {
        self.t_center - ((width as f32 / 2.0) / self.px_per_day * 86_400.0) as i64
    }

    pub fn t_right(&self, width: u16) -> i64 {
        self.t_center + ((width as f32 / 2.0) / self.px_per_day * 86_400.0) as i64
    }

    pub fn x_of(&self, t: i64, width: u16) -> i32 {
        let days = (t - self.t_center) as f32 / 86_400.0;
        (width as f32 / 2.0 + days * self.px_per_day).round() as i32
    }

    pub fn t_of(&self, x: i32, width: u16) -> i64 {
        let days = (x as f32 - width as f32 / 2.0) / self.px_per_day;
        self.t_center + (days * 86_400.0) as i64
    }
}

/// Named zoom presets — the lens stops on the multiscale ladder.
pub const ZOOM_PRESETS: [(&str, f32); 6] = [
    ("eon", 100.0 / 3650.0), // ~decade
    ("year", 100.0 / 365.0),
    ("quarter", 100.0 / 90.0),
    ("month", 100.0 / 30.0),
    ("week", 100.0 / 10.0),
    ("day", 100.0 / 2.5),
];

pub const ZOOM_FIT: f32 = 0.0; // sentinel: fit loaded history

/// Camera pan animation (the temporal transition feel).
pub struct PanAnim {
    pub from_t: i64,
    pub from_px: f32,
    pub to_t: i64,
    pub to_px: f32,
    pub start: Instant,
    pub dur: Duration,
}

/// Focusable panes (the ring is used for modal capture/restore; pane cycling
/// is app-owned because ring membership is static in v0.2.0 — see issue #40).
pub const FOCUS_MAIN: FocusId = FocusId(1);
pub const FOCUS_FILES: FocusId = FocusId(2);
pub const FOCUS_SEARCH: FocusId = FocusId(3);
pub const FOCUS_CMD: FocusId = FocusId(4);

/// The strata (file evolution) lens state.
pub struct StrataState {
    pub path: String,
    pub events: Vec<FileEvent>,
    pub cursor: usize,
    pub scroll: usize,
}

/// The provenance (blame) lens state.
pub struct ProvState {
    pub commit: Option<u32>,
    pub path: String,
    pub blame: Option<BlameData>,
    pub line_cursor: usize,
    pub line_scroll: usize,
    /// Whether the selected line has "unfolded backward" into its fiber.
    pub unfolded: bool,
    /// Cached fiber stations for the unfolded line (computed once per unfold).
    pub fiber: Vec<crate::git::filelog::FileEvent>,
    /// Which fiber station the `f` key cycles to.
    pub fiber_cursor: usize,
}

/// The diff lens state.
pub struct LensState {
    pub commit: Option<u32>,
    pub diff: Option<CommitDiff>,
    pub file_cursor: usize,
    pub file_scroll: usize,
    pub hunk_cursor: usize,
    pub hunk_scroll: usize,
    pub focus_files: bool,
    /// Search inside the diff (`/` inside lens).
    pub query: String,
    pub matches: Vec<(usize, usize)>, // (file idx, line idx)
    pub match_cursor: usize,
}

/// File browser overlay state.
pub struct BrowserState {
    pub at_commit: u32,
    pub entries: Vec<crate::git::filelog::TreeEntry>,
    pub filter: String,
    pub cursor: usize,
}

pub struct App {
    pub repo: Repo,
    pub hist: History,
    pub palette: Palette,
    pub branches: Vec<BranchInfo>,
    pub tags: Vec<TagInfo>,
    pub head: HeadInfo,
    pub metrics: Option<Metrics>,
    pub now: i64,
    pub scanned: usize,

    pub view: View,
    pub camera: Camera,
    pub selection: Option<u32>,
    pub search: search::SearchState,
    pub lens: LensState,
    pub prov: ProvState,
    pub strata: StrataState,
    pub browser: Option<BrowserState>,
    pub mode: InputMode,
    pub health_scroll: usize,

    pub status: String,
    pub status_err: bool,

    pub width: u16,
    pub height: u16,

    pub anim: Option<PanAnim>,
    pub dirty: bool,
    pub ring: FocusRing,

    /// Lazily-loaded full message of the selected commit (dossier body).
    pub dossier_msg: Option<(String, String)>,
    /// Emissions queued for scrollback (dump mode replays these; interactive
    /// mode pushes them through the live-region insertion immediately).
    pub scrollback_log: Vec<String>,
    /// Counters for `--profile`.
    pub profile: ProfileNote,
}

#[derive(Default)]
pub struct ProfileNote {
    pub load_ms: u128,
    pub tier_a_resolved: usize,
    pub windows_drawn: usize,
}

impl App {
    pub fn load(repo: Repo, palette: Palette, limit: usize) -> Result<Self, String> {
        let t0 = Instant::now();
        let head = repo.head()?;
        let branches = repo.branches()?;
        let tags = repo.tags()?;

        let (hist, scanned) = History::load(&repo, limit)?;
        let now = hist.rows.first().map(|r| r.time).unwrap_or(0);
        let span_days = ((hist.t_max - hist.t_min) / 86_400).max(1) as f32;
        let camera = Camera {
            px_per_day: (100.0 / span_days).clamp(0.004, 40.0),
            t_center: (hist.t_max + hist.t_min) / 2,
        };
        let selection = hist.rows.first().map(|_| 0);
        let load_ms = t0.elapsed().as_millis();

        let mut app = Self {
            repo,
            hist,
            palette,
            branches,
            tags,
            head,
            metrics: None,
            now,
            scanned,
            view: View::Atlas,
            camera,
            selection,
            search: search::SearchState::default(),
            lens: LensState {
                commit: None,
                diff: None,
                file_cursor: 0,
                file_scroll: 0,
                hunk_cursor: 0,
                hunk_scroll: 0,
                focus_files: false,
                query: String::new(),
                matches: Vec::new(),
                match_cursor: 0,
            },
            prov: ProvState {
                commit: None,
                path: String::new(),
                blame: None,
                line_cursor: 0,
                line_scroll: 0,
                unfolded: false,
                fiber: Vec::new(),
                fiber_cursor: 0,
            },
            strata: StrataState {
                path: String::new(),
                events: Vec::new(),
                cursor: 0,
                scroll: 0,
            },
            browser: None,
            mode: InputMode::Normal,
            health_scroll: 0,
            status: String::new(),
            status_err: false,
            width: 80,
            height: 24,
            anim: None,
            dirty: true,
            ring: FocusRing::new([FOCUS_MAIN, FOCUS_FILES, FOCUS_SEARCH, FOCUS_CMD]),
            dossier_msg: None,
            scrollback_log: Vec::new(),
            profile: ProfileNote {
                load_ms,
                tier_a_resolved: 0,
                windows_drawn: 0,
            },
        };
        app.hist.enrich(&app.branches, &app.tags);
        app.set_status(format!(
            "{} commits, {} branches, {} tags, span {}d",
            app.hist.len(),
            app.branches.iter().filter(|b| !b.is_remote).count(),
            app.tags.len(),
            span_days as i64
        ));
        Ok(app)
    }

    pub fn set_status(&mut self, msg: impl Into<String>) {
        self.status = msg.into();
        self.status_err = false;
    }

    pub fn set_error(&mut self, msg: impl Into<String>) {
        self.status = msg.into();
        self.status_err = true;
    }

    pub fn mark(&mut self) {
        self.dirty = true;
    }

    pub fn sel_row(&self) -> Option<&crate::git::history::CommitRow> {
        self.selection.map(|i| &self.hist.rows[i as usize])
    }

    /// Move selection by `delta` in time order (rows are newest-first, so
    /// +1 = older/right). Follows the camera with a pan animation.
    pub fn step_selection(&mut self, delta: i32) {
        let Some(cur) = self.selection else { return };
        let n = self.hist.len() as i32;
        let next = (cur as i32 + delta).clamp(0, n - 1) as u32;
        if next == cur {
            return;
        }
        self.select(next);
    }

    pub fn select(&mut self, idx: u32) {
        self.selection = Some(idx);
        self.dossier_msg = None;
        if let Some(r) = self.sel_row() {
            self.animate_camera_to(r.time, self.camera.px_per_day);
        }
        self.mark();
    }

    /// Jump the selection to an oid (keyed: survives filtering).
    pub fn select_oid(&mut self, oid: &str) -> bool {
        match self.hist.idx_of(oid) {
            Some(i) => {
                self.select(i);
                true
            }
            None => false,
        }
    }

    pub fn animate_camera_to(&mut self, t: i64, px_per_day: f32) {
        let already = self.camera.t_center == t
            && self.camera.px_per_day == px_per_day
            && self.anim.is_none();
        if already {
            return;
        }
        self.anim = Some(PanAnim {
            from_t: self.camera.t_center,
            from_px: self.camera.px_per_day,
            to_t: t,
            to_px: px_per_day,
            start: Instant::now(),
            dur: Duration::from_millis(240),
        });
        self.mark();
    }

    /// Advance the camera animation one frame. Returns true while animating.
    pub fn tick_animation(&mut self) -> bool {
        let Some(a) = &self.anim else {
            return false;
        };
        let (from_t, from_px, to_t, to_px, start, dur) =
            (a.from_t, a.from_px, a.to_t, a.to_px, a.start, a.dur);
        let e = start.elapsed().as_secs_f32() / dur.as_secs_f32();
        if e >= 1.0 {
            self.camera.t_center = to_t;
            self.camera.px_per_day = to_px;
            self.anim = None;
            self.mark();
            return false;
        }
        let eased = gibson::clock::ease_out(e.min(1.0));
        self.camera.t_center = from_t + ((to_t - from_t) as f32 * eased) as i64;
        self.camera.px_per_day = from_px + (to_px - from_px) * eased;
        self.mark();
        true
    }

    pub fn zoom(&mut self, factor: f32) {
        let new_px = (self.camera.px_per_day * factor).clamp(0.004, 40.0);
        self.animate_camera_to(self.camera.t_center, new_px);
        self.set_status(format!(
            "lens scale {:.2} px/day · {:.0} days across",
            new_px,
            self.camera.days_visible(self.width.max(40))
        ));
    }

    pub fn zoom_preset(&mut self, idx: usize) {
        let (_, px) = ZOOM_PRESETS[idx.min(ZOOM_PRESETS.len() - 1)];
        self.animate_camera_to(self.camera.t_center, px);
    }

    pub fn pan(&mut self, frac: f32) {
        let dt = (self.camera.days_visible(self.width) * frac * 86_400.0) as i64;
        self.camera.t_center += dt;
        self.mark();
    }

    /// Re-center the camera on the selection without animation (initial view).
    pub fn center_on_selection(&mut self) {
        if let Some(r) = self.sel_row() {
            self.camera.t_center = r.time;
        }
    }

    // ---- on-demand data loads ---------------------------------------------

    pub fn ensure_lens(&mut self) {
        let Some(sel) = self.selection else { return };
        if self.lens.commit == Some(sel) && self.lens.diff.is_some() {
            return;
        }
        let oid = self.hist.rows[sel as usize].oid.clone();
        match crate::git::diff::commit_diff(&self.repo, &oid, 400) {
            Ok(d) => {
                self.lens.commit = Some(sel);
                self.lens.diff = Some(d);
                self.lens.file_cursor = 0;
                self.lens.file_scroll = 0;
                self.lens.hunk_cursor = 0;
                self.lens.hunk_scroll = 0;
                self.lens.query.clear();
                self.lens.matches.clear();
                self.mark();
            }
            Err(e) => self.set_error(e),
        }
    }

    pub fn ensure_prov(&mut self, path: &str) {
        let Some(sel) = self.selection else { return };
        let oid = self.hist.rows[sel as usize].oid.clone();
        if self.prov.commit == Some(sel) && self.prov.path == path && self.prov.blame.is_some() {
            return;
        }
        match crate::git::blame::blame_file(&self.repo, &oid, path, 4000) {
            Ok(b) => {
                self.prov.commit = Some(sel);
                self.prov.path = path.to_string();
                self.prov.blame = Some(b);
                self.prov.line_cursor = 0;
                self.prov.line_scroll = 0;
                self.prov.unfolded = false;
                self.mark();
            }
            Err(e) => self.set_error(e),
        }
    }

    pub fn ensure_strata(&mut self, path: &str) {
        if self.strata.path == path && !self.strata.events.is_empty() {
            return;
        }
        let events = crate::git::filelog::file_events(&self.repo, &self.hist, path, 3000);
        self.strata.path = path.to_string();
        self.strata.events = events;
        self.strata.cursor = 0;
        self.strata.scroll = 0;
        self.mark();
    }

    pub fn ensure_metrics(&mut self) {
        if self.metrics.is_some() {
            return;
        }
        self.metrics = Some(crate::git::metrics::compute(
            &self.repo,
            &self.hist,
            &self.branches,
            self.scanned,
        ));
        self.mark();
    }

    pub fn dossier_message(&mut self) -> String {
        let Some(sel) = self.selection else {
            return String::new();
        };
        let oid = self.hist.rows[sel as usize].oid.clone();
        if let Some((cached_oid, msg)) = &self.dossier_msg {
            if *cached_oid == oid {
                return msg.clone();
            }
        }
        let msg = self
            .repo
            .inner()
            .find_commit(git2::Oid::from_str(&oid).unwrap_or(git2::Oid::zero()))
            .ok()
            .map(|c| c.message().unwrap_or("").to_string())
            .unwrap_or_default();
        let trimmed = crate::theme::truncate(msg.trim(), 400).to_string();
        self.dossier_msg = Some((oid, trimmed.clone()));
        trimmed
    }

    pub fn open_browser(&mut self, at_commit: u32) {
        let oid = self.hist.rows[at_commit as usize].oid.clone();
        match crate::git::filelog::list_tree(&self.repo, &oid, 2000) {
            Ok(entries) => {
                self.ring.capture();
                self.browser = Some(BrowserState {
                    at_commit,
                    entries,
                    filter: String::new(),
                    cursor: 0,
                });
                self.mode = InputMode::FileBrowser;
                self.mark();
            }
            Err(e) => self.set_error(e),
        }
    }

    pub fn close_overlay(&mut self) {
        match self.mode {
            InputMode::HelpOverlay | InputMode::FileBrowser => {
                self.ring.release();
            }
            _ => {}
        }
        self.browser = None;
        self.mode = InputMode::Normal;
        self.mark();
    }

    // ---- scrollback policy ---------------------------------------------------

    /// Queue a finalized artifact for native scrollback. In interactive mode
    /// this is inserted above the live region immediately (the app keeps
    /// running below); in dump mode the queue is replayed into the capture.
    pub fn note_scrollback(&mut self, text: String) {
        self.scrollback_log.push(text);
    }
}
#[cfg(test)]
mod camera_tests {
    use super::*;

    #[test]
    fn x_of_places_center_at_half_width() {
        let cam = Camera {
            px_per_day: 8.0,
            t_center: 1_000_000_000,
        };
        assert_eq!(cam.x_of(1_000_000_000, 100), 50, "center maps to middle");
        assert_eq!(cam.x_of(1_000_086_400, 100), 58, "+1 day at 8px/day");
        assert_eq!(cam.x_of(999_913_600, 100), 42, "-1 day at 8px/day");
    }

    #[test]
    fn t_of_round_trips_x_of_at_integral_scales() {
        // x/t is exact when one day maps to a whole column count; fractional
        // scales are for display only (the ruler reads t, never x->t).
        let cam = Camera {
            px_per_day: 4.0,
            t_center: 1_700_000_000,
        };
        for &t in &[
            1_700_000_000i64,
            1_700_086_400,
            1_699_913_600,
            1_700_432_000,
        ] {
            let x = cam.x_of(t, 80);
            assert_eq!(cam.t_of(x, 80), t, "round trip for {t}");
        }
    }

    #[test]
    fn window_bounds_agree_with_x_of() {
        let cam = Camera {
            px_per_day: 10.0,
            t_center: 500_000,
        };
        let w = 120u16;
        let tl = cam.t_left(w);
        let tr = cam.t_right(w);
        assert_eq!(cam.x_of(tl, w), 0, "left edge maps to column 0");
        assert_eq!(cam.x_of(tr, w), w as i32, "right edge maps to last column");
        assert!(tl < cam.t_center && tr > cam.t_center);
    }

    #[test]
    fn days_visible_is_width_over_scale() {
        let cam = Camera {
            px_per_day: 10.0,
            t_center: 0,
        };
        assert_eq!(cam.days_visible(100), 10.0);
        let zoomed = Camera {
            px_per_day: 0.1,
            t_center: 0,
        };
        assert!((zoomed.days_visible(100) - 1000.0).abs() < 1e-3);
    }

    #[test]
    fn zoom_presets_form_a_narrowing_ladder() {
        for w in ZOOM_PRESETS.windows(2) {
            assert!(
                w[0].1 < w[1].1,
                "ladder must narrow: {} ({}) then {} ({})",
                w[0].0,
                w[0].1,
                w[1].0,
                w[1].1
            );
        }
        assert_eq!(ZOOM_PRESETS[0].0, "eon");
        assert_eq!(ZOOM_PRESETS[5].0, "day");
    }

    #[test]
    fn view_short_codes_are_compact() {
        for v in [
            View::Atlas,
            View::Strata,
            View::Lens,
            View::Provenance,
            View::Health,
        ] {
            assert!(
                v.short().len() <= 6,
                "{} stays compact at hostile widths",
                v.short()
            );
        }
        assert_eq!(View::from_num(1), Some(View::Atlas));
        assert_eq!(View::from_num(5), Some(View::Health));
        assert_eq!(View::from_num(0), None);
        assert_eq!(View::from_num(6), None);
    }
}

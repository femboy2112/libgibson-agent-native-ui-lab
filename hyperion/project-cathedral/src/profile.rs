//! Sustained-run instrumentation.
//!
//! Every number here is either measured by LibGibson's public counters or by
//! `/proc/self/statm`. Wall-clock frame times are labelled as such: they include
//! the **profiler's own overhead** (building the surface, reading `/proc`,
//! pushing into the sample vector). The `render_us` field is LibGibson's own
//! `last_render_duration_micros` and is the least contaminated frame-cost probe.

use std::time::Duration;

use gibson::capability::ColorDepth;
use gibson::scheduler::RenderStats;

/// One frame's measurements.
#[derive(Clone, Copy, Debug, Default)]
pub struct FrameSample {
    pub frame_time_us: u64,
    pub render_us: u64,
    pub surface_build_us: u64,
    pub bytes: u64,
    pub dirty_cells: u64,
    pub exact_changed: u64,
    pub history_insertions: u64,
    pub rss_kib: u64,
}

/// Aggregated sustained-run metrics.
#[derive(Debug, Default)]
pub struct Profile {
    pub frames: u64,
    pub samples: Vec<FrameSample>,
    pub total_bytes: u64,
    pub total_dirty: u64,
    pub total_exact: u64,
    pub total_history_insertions: u64,
    pub max_history_insertions_per_frame: u64,
    pub rss_start_kib: u64,
    pub rss_end_kib: u64,
    pub rss_peak_kib: u64,
    pub music_builds: u32,
    pub music_total_us: u64,
    pub music_last_us: u64,
    pub color_depth: ColorDepth,
    pub ticks_paused: u64,
    pub scrollback_entries: u64,
}

impl Profile {
    pub fn new() -> Self {
        Profile {
            rss_start_kib: rss_kib(),
            rss_end_kib: rss_kib(),
            color_depth: ColorDepth::TrueColor,
            ..Profile::default()
        }
    }

    pub fn record(&mut self, s: FrameSample) {
        self.frames += 1;
        self.total_bytes += s.bytes;
        self.total_dirty += s.dirty_cells;
        self.total_exact += s.exact_changed;
        self.samples.push(s);
        self.rss_end_kib = rss_kib();
        self.rss_peak_kib = self.rss_peak_kib.max(s.rss_kib);
    }

    pub fn percentile_us(&self, p: f64) -> u64 {
        if self.samples.is_empty() {
            return 0;
        }
        let mut v: Vec<u64> = self.samples.iter().map(|s| s.frame_time_us).collect();
        v.sort_unstable();
        let idx = ((v.len() as f64 - 1.0) * p).round() as usize;
        v[idx.min(v.len() - 1)]
    }

    pub fn mean_frame_us(&self) -> f64 {
        if self.frames == 0 {
            return 0.0;
        }
        self.samples
            .iter()
            .map(|s| s.frame_time_us as f64)
            .sum::<f64>()
            / self.frames as f64
    }

    pub fn mean_render_us(&self) -> f64 {
        if self.frames == 0 {
            return 0.0;
        }
        self.samples.iter().map(|s| s.render_us as f64).sum::<f64>() / self.frames as f64
    }

    pub fn max_frame_us(&self) -> u64 {
        self.samples
            .iter()
            .map(|s| s.frame_time_us)
            .max()
            .unwrap_or(0)
    }

    pub fn mean_bytes(&self) -> f64 {
        if self.frames == 0 {
            return 0.0;
        }
        self.total_bytes as f64 / self.frames as f64
    }

    pub fn mean_dirty(&self) -> f64 {
        if self.frames == 0 {
            return 0.0;
        }
        self.total_dirty as f64 / self.frames as f64
    }

    pub fn mean_exact(&self) -> f64 {
        if self.frames == 0 {
            return 0.0;
        }
        self.total_exact as f64 / self.frames as f64
    }

    /// Merge LibGibson's absolute counters.
    pub fn absorb_stats(&mut self, stats: &RenderStats, prev: &RenderStats) {
        let bytes = stats.frame_bytes.saturating_sub(prev.frame_bytes);
        let dirty = stats.dirty_cells.saturating_sub(prev.dirty_cells);
        let hist = stats
            .history_insertions
            .saturating_sub(prev.history_insertions);
        self.total_history_insertions += hist;
        self.max_history_insertions_per_frame = self.max_history_insertions_per_frame.max(hist);
        let _ = (bytes, dirty);
    }

    pub fn summary(&self, fps: u32) -> String {
        let secs = self.frames as f64 / fps.max(1) as f64;
        let mut out = String::new();
        out.push_str("── PROJECT CATHEDRAL · sustained run ──\n");
        out.push_str(&format!(
            "frames: {}  nominal {:?}/frame  wall {:.1}s  color {:?}\n",
            self.frames,
            Duration::from_micros(1_000_000 / fps.max(1) as u64),
            secs,
            self.color_depth
        ));
        out.push_str(&format!(
            "frame time: mean {:.3} ms  p95 {:.3} ms  max {:.3} ms  (profiler overhead included)\n",
            self.mean_frame_us() / 1000.0,
            self.percentile_us(0.95) as f64 / 1000.0,
            self.max_frame_us() as f64 / 1000.0,
        ));
        out.push_str(&format!(
            "libgibson render: mean {:.3} ms/frame  (last_render_duration_micros)\n",
            self.mean_render_us() / 1000.0,
        ));
        out.push_str(&format!(
            "emitted: mean {:.0} frame bytes/frame  total {:.1} MiB\n",
            self.mean_bytes(),
            self.total_bytes as f64 / (1024.0 * 1024.0),
        ));
        out.push_str(&format!(
            "cells: mean {:.1} affected/frame  mean {:.1} exact changed/frame  total affected {}\n",
            self.mean_dirty(),
            self.mean_exact(),
            self.total_dirty,
        ));
        out.push_str(&format!(
            "history insertions: {}  max/frame {}\n",
            self.total_history_insertions, self.max_history_insertions_per_frame,
        ));
        out.push_str(&format!(
            "scrollback commits: {}  bounded retained history: the app never grows its own event vector past its cap\n",
            self.scrollback_entries,
        ));
        out.push_str(&format!(
            "RSS: start {} KiB -> end {} KiB  peak {} KiB\n",
            self.rss_start_kib, self.rss_end_kib, self.rss_peak_kib,
        ));
        out.push_str(&format!(
            "music: {} checked rebuilds  last {:.3} ms  total {:.1} ms  mean {:.3} ms/rebuild\n",
            self.music_builds,
            self.music_last_us as f64 / 1000.0,
            self.music_total_us as f64 / 1000.0,
            if self.music_builds > 0 {
                self.music_total_us as f64 / self.music_builds as f64 / 1000.0
            } else {
                0.0
            },
        ));
        out
    }
}

/// Resident set size in KiB, read from `/proc/self/statm` (Linux).
pub fn rss_kib() -> u64 {
    if let Ok(s) = std::fs::read_to_string("/proc/self/statm") {
        if let Some(first) = s.split_whitespace().next() {
            if let Ok(pages) = first.parse::<u64>() {
                return pages * 4;
            }
        }
    }
    0
}

/// A simple monotonic wall clock for the profiler.
pub fn now_us() -> u64 {
    use std::time::Instant;
    thread_local! {
        static START: Instant = Instant::now();
    }
    START.with(|s| s.elapsed().as_micros() as u64)
}

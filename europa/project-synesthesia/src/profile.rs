//! Fixed-size frame profiling; profiling state never grows with session length.

use std::time::Duration;

#[derive(Clone, Debug)]
pub struct Profile {
    pub frames: u64,
    pub frame_time_sum_us: u128,
    pub frame_time_max_us: u64,
    pub frame_histogram_ms: [u64; 128],
    pub dsp_sum_us: u128,
    pub surface_sum_us: u128,
    pub surface_max_us: u64,
    pub renderer_sum_us: u128,
    pub bytes: u128,
    pub bytes_max: u64,
    pub exact_changed_cells: u128,
    pub affected_cells: u128,
    pub nodes_sum: u128,
    pub nodes_max: usize,
    pub late_frames: u64,
    pub dropped_frames: u64,
    pub rss_start_kb: u64,
    pub rss_warm_kb: u64,
    pub rss_end_kb: u64,
    pub rss_peak_kb: u64,
    pub retained_history: usize,
}

impl Profile {
    pub fn new(retained_history: usize) -> Self {
        let (rss_start_kb, rss_peak_kb) = memory_kb();
        Self {
            frames: 0,
            frame_time_sum_us: 0,
            frame_time_max_us: 0,
            frame_histogram_ms: [0; 128],
            dsp_sum_us: 0,
            surface_sum_us: 0,
            surface_max_us: 0,
            renderer_sum_us: 0,
            bytes: 0,
            bytes_max: 0,
            exact_changed_cells: 0,
            affected_cells: 0,
            nodes_sum: 0,
            nodes_max: 0,
            late_frames: 0,
            dropped_frames: 0,
            rss_start_kb,
            rss_warm_kb: rss_start_kb,
            rss_end_kb: rss_start_kb,
            rss_peak_kb,
            retained_history,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record(
        &mut self,
        frame_time: Duration,
        dsp_time: Duration,
        surface_us: u64,
        renderer_us: u64,
        bytes: u64,
        exact_changed: usize,
        affected: u64,
        nodes: usize,
        interval: Duration,
        skipped: u64,
    ) {
        self.frames += 1;
        let elapsed = frame_time.as_micros().min(u64::MAX as u128) as u64;
        self.frame_time_sum_us += elapsed as u128;
        self.frame_time_max_us = self.frame_time_max_us.max(elapsed);
        let bucket = elapsed.div_ceil(1_000).min(127) as usize;
        self.frame_histogram_ms[bucket] += 1;
        self.dsp_sum_us += dsp_time.as_micros();
        self.surface_sum_us += surface_us as u128;
        self.surface_max_us = self.surface_max_us.max(surface_us);
        self.renderer_sum_us += renderer_us as u128;
        self.bytes += bytes as u128;
        self.bytes_max = self.bytes_max.max(bytes);
        self.exact_changed_cells += exact_changed as u128;
        self.affected_cells += affected as u128;
        self.nodes_sum += nodes as u128;
        self.nodes_max = self.nodes_max.max(nodes);
        if frame_time > interval {
            self.late_frames += 1;
        }
        self.dropped_frames += skipped;
        let (rss, peak) = memory_kb();
        if self.frames == 64 {
            self.rss_warm_kb = rss;
        }
        self.rss_end_kb = rss;
        self.rss_peak_kb = self.rss_peak_kb.max(peak);
    }

    pub fn summary(&self, fps: u32) -> String {
        let mean = self.mean_frame_us();
        let p95 = self.percentile_us(0.95);
        let mean_div = self.frames.max(1) as u128;
        format!(
            "SYNESTHESIA PERFORMANCE\n\
             frames: {}  target: {} FPS  late (> budget): {}  scheduler-skipped: {}\n\
             frame: mean {:.2} ms  p95 {:.2} ms  max {:.2} ms\n\
             DSP: {:.2} ms/frame  custom Surface: {:.2} ms/frame (max {:.2})\n\
             LibGibson render: {:.2} ms/frame  nodes: {:.1} avg / {} max\n\
             exact changed cells: {:.1}/frame  affected footprint: {:.1}/frame\n\
             emitted frame bytes: {:.1}/frame  max {}  retained spectra: {}/{}\n\
             RSS: start {} -> warm(64) {} -> end {} KiB  peak {} KiB",
            self.frames,
            fps,
            self.late_frames,
            self.dropped_frames,
            mean as f64 / 1_000.0,
            p95 as f64 / 1_000.0,
            self.frame_time_max_us as f64 / 1_000.0,
            self.dsp_sum_us as f64 / mean_div as f64 / 1_000.0,
            self.surface_sum_us as f64 / mean_div as f64 / 1_000.0,
            self.surface_max_us as f64 / 1_000.0,
            self.renderer_sum_us as f64 / mean_div as f64 / 1_000.0,
            self.nodes_sum as f64 / mean_div as f64,
            self.nodes_max,
            self.exact_changed_cells as f64 / mean_div as f64,
            self.affected_cells as f64 / mean_div as f64,
            self.bytes as f64 / mean_div as f64,
            self.bytes_max,
            self.retained_history,
            crate::visual::HISTORY_CAPACITY,
            self.rss_start_kb,
            self.rss_warm_kb,
            self.rss_end_kb,
            self.rss_peak_kb,
        )
    }

    fn mean_frame_us(&self) -> u64 {
        if self.frames == 0 {
            0
        } else {
            (self.frame_time_sum_us / self.frames as u128) as u64
        }
    }

    fn percentile_us(&self, percentile: f64) -> u64 {
        if self.frames == 0 {
            return 0;
        }
        let target = (self.frames as f64 * percentile).ceil() as u64;
        let mut seen = 0;
        for (bucket, count) in self.frame_histogram_ms.iter().enumerate() {
            seen += count;
            if seen >= target {
                return bucket as u64 * 1_000;
            }
        }
        127_000
    }
}

pub fn memory_kb() -> (u64, u64) {
    #[cfg(target_os = "linux")]
    {
        let Ok(contents) = std::fs::read_to_string("/proc/self/status") else {
            return (0, 0);
        };
        let mut rss = 0;
        let mut peak = 0;
        for line in contents.lines() {
            if let Some(value) = line.strip_prefix("VmRSS:") {
                rss = first_number(value);
            }
            if let Some(value) = line.strip_prefix("VmHWM:") {
                peak = first_number(value);
            }
        }
        (rss, peak)
    }
    #[cfg(not(target_os = "linux"))]
    {
        (0, 0)
    }
}

fn first_number(value: &str) -> u64 {
    value
        .split_whitespace()
        .next()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
}

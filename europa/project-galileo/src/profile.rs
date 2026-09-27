//! Performance and frame instrumentation for Project Galileo.
//!
//! Instruments:
//! - Frame output bytes
//! - Changed cells / terminal diff efficiency via `vt100::Parser`
//! - Frame render duration (min / max / avg)
//! - Active animations and retained presentation keys
//! - Visual generation throughput

use std::time::Duration;

#[derive(Debug, Clone)]
pub struct FrameStats {
    pub frame_index: u64,
    pub render_time: Duration,
    pub frame_bytes: usize,
    pub changed_cells: usize,
    pub active_animations: usize,
    pub retained_keys: usize,
}

pub struct Profiler {
    parser: vt100::Parser,
    prev_screen_chars: Vec<char>,
    width: u16,
    height: u16,
    total_frames: u64,
    total_bytes: usize,
    total_duration: Duration,
    min_duration: Duration,
    max_duration: Duration,
    last_frame_stats: Option<FrameStats>,
}

impl Profiler {
    pub fn new(width: u16, height: u16) -> Self {
        let size = (width as usize) * (height as usize);
        Self {
            parser: vt100::Parser::new(height, width, 0),
            prev_screen_chars: vec![' '; size],
            width,
            height,
            total_frames: 0,
            total_bytes: 0,
            total_duration: Duration::ZERO,
            min_duration: Duration::from_secs(999),
            max_duration: Duration::ZERO,
            last_frame_stats: None,
        }
    }

    pub fn record_frame(
        &mut self,
        bytes: &[u8],
        render_time: Duration,
        active_animations: usize,
        retained_keys: usize,
    ) -> FrameStats {
        self.total_frames += 1;
        self.total_bytes += bytes.len();
        self.total_duration += render_time;
        self.min_duration = self.min_duration.min(render_time);
        self.max_duration = self.max_duration.max(render_time);

        // Feed bytes into vt100 parser to inspect terminal cell changes
        self.parser.process(bytes);
        let screen = self.parser.screen();

        let mut changed_cells = 0;
        let mut curr_chars = Vec::with_capacity((self.width as usize) * (self.height as usize));

        for row in 0..self.height {
            for col in 0..self.width {
                let cell_char = screen
                    .cell(row, col)
                    .map(|c| c.contents().chars().next().unwrap_or(' '))
                    .unwrap_or(' ');
                curr_chars.push(cell_char);
            }
        }

        if !self.prev_screen_chars.is_empty() && self.prev_screen_chars.len() == curr_chars.len() {
            for (i, &curr) in curr_chars.iter().enumerate() {
                if self.prev_screen_chars[i] != curr {
                    changed_cells += 1;
                }
            }
        } else {
            changed_cells = curr_chars.len();
        }
        self.prev_screen_chars = curr_chars;

        let stats = FrameStats {
            frame_index: self.total_frames,
            render_time,
            frame_bytes: bytes.len(),
            changed_cells,
            active_animations,
            retained_keys,
        };

        self.last_frame_stats = Some(stats.clone());
        stats
    }

    pub fn summary(&self) -> String {
        let avg_time = if self.total_frames > 0 {
            self.total_duration / (self.total_frames as u32)
        } else {
            Duration::ZERO
        };
        let avg_bytes = if self.total_frames > 0 {
            self.total_bytes / (self.total_frames as usize)
        } else {
            0
        };

        format!(
            "PROFILER REPORT: [Frames: {}] | [Avg Frame Time: {:.2}ms (min: {:.2}ms, max: {:.2}ms)] | [Avg Bytes: {} B] | [Total Data: {:.2} KB]",
            self.total_frames,
            avg_time.as_secs_f64() * 1000.0,
            self.min_duration.as_secs_f64() * 1000.0,
            self.max_duration.as_secs_f64() * 1000.0,
            avg_bytes,
            self.total_bytes as f64 / 1024.0,
        )
    }

    pub fn last_stats(&self) -> Option<&FrameStats> {
        self.last_frame_stats.as_ref()
    }
}

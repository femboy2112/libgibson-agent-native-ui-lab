//! Search state: query, hits, and the keyed-selection policy that keeps the
//! selection stable while filters change underneath it.

use crate::git::history::History;

#[derive(Default)]
pub struct SearchState {
    pub query: String,
    /// Row indices matching the query, newest first.
    pub hits: Vec<u32>,
    /// Which hit `n`/`N` currently points at.
    pub hit_cursor: usize,
}

impl SearchState {
    /// Recompute hits for the current query. The selection is *not* moved:
    /// the atlas keeps its selected commit and paints hit halos around
    /// matches (stable keys across filtering).
    pub fn recompute(&mut self, hist: &History) {
        self.hits = hist.search(&self.query);
        if self.hit_cursor >= self.hits.len() {
            self.hit_cursor = 0;
        }
    }

    pub fn push(&mut self, c: char, hist: &History) {
        self.query.push(c);
        self.recompute(hist);
    }

    pub fn pop(&mut self, hist: &History) {
        self.query.pop();
        self.recompute(hist);
    }

    pub fn clear(&mut self) {
        self.query.clear();
        self.hits.clear();
        self.hit_cursor = 0;
    }

    pub fn is_active(&self) -> bool {
        !self.query.is_empty() && !self.hits.is_empty()
    }

    /// Cycle to the next/previous hit; returns the row index to jump to.
    pub fn cycle(&mut self, forward: bool) -> Option<u32> {
        if self.hits.is_empty() {
            return None;
        }
        if forward {
            self.hit_cursor = (self.hit_cursor + 1) % self.hits.len();
        } else {
            self.hit_cursor =
                (self.hit_cursor + self.hits.len().saturating_sub(1)) % self.hits.len();
        }
        Some(self.hits[self.hit_cursor])
    }
}

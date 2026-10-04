//! The observatory: raw data + cleaned streams + the causal analysis timeline,
//! all derived from a single seed.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::dsp::{self, Clean, Spectrum};
use crate::rng::fnv1a;
use crate::sim::{self, RawData, Truth, N};
use crate::track::Timeline;

pub const DEFAULT_SEED: u64 = 0x5015_A211_2112;

pub struct Observatory {
    pub seed: u64,
    pub raw: RawData,
    pub cleans: Vec<Clean>,
    pub timeline: Timeline,
    spectra: Mutex<HashMap<usize, Arc<Spectrum>>>,
}

impl Observatory {
    pub fn new(seed: u64) -> Observatory {
        Observatory::from_raw(seed, sim::generate(seed, true))
    }

    pub fn from_raw(seed: u64, raw: RawData) -> Observatory {
        let cleans: Vec<Clean> = raw.streams.iter().map(|s| dsp::preprocess(s)).collect();
        let timeline = Timeline::build(&cleans);
        Observatory {
            seed,
            raw,
            cleans,
            timeline,
            spectra: Mutex::new(HashMap::new()),
        }
    }

    pub fn truth(&self) -> &[Truth] {
        &self.raw.truth
    }

    /// Normalised periodogram of the first `ep` epochs of station 1 (cached; the
    /// cache can never change a result, it only avoids recomputing it).
    pub fn spectrum(&self, ep: usize) -> Arc<Spectrum> {
        let mut cache = self.spectra.lock().unwrap_or_else(|e| e.into_inner());
        cache
            .entry(ep)
            .or_insert_with(|| {
                let n = ep * sim::EPOCH_SAMPLES;
                Arc::new(dsp::periodogram(&self.cleans[0], n, N))
            })
            .clone()
    }

    /// Digest over the raw data and every timeline number: the determinism witness.
    pub fn digest(&self) -> u64 {
        let mut bytes: Vec<u8> = Vec::new();
        for s in &self.raw.streams {
            for v in s {
                bytes.extend_from_slice(&v.to_bits().to_le_bytes());
            }
        }
        for e in &self.timeline.epochs {
            for s in &e.slots {
                for v in [s.f, s.z, s.pulsy, s.persist, s.peak_phase, s.fold_snr] {
                    bytes.extend_from_slice(&v.to_bits().to_le_bytes());
                }
                bytes.push(s.state as u8);
            }
        }
        fnv1a(&bytes)
    }
}

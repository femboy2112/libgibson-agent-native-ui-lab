//! In-process playback through LibGibson's own realtime device backend.
//!
//! Under the `device-audio` feature this drives [`gibson::audio::device::AudioDevice`],
//! which streams an [`AudioSource`] to the default output device through cpal (→ **ALSA**
//! on Linux). Nothing is spawned: the *same* [`HumanMusicSynth`] the offline renderer uses
//! is pushed across a bounded ring by LibGibson's own render thread.
//!
//! The synth is built lazily on the first rendered block instead of up front. The device
//! negotiates its own sample rate and reports it through `AudioDevice::sample_rate`; using
//! the rate carried by the first [`RenderCtx`] keeps pitch honest when a device refuses
//! 48 kHz. That is the one bit of ceremony the library deliberately leaves to the caller.
//!
//! Without the feature the module is a stub that reports the missing backend, so a
//! device-free binary still builds and the WAV remains the artifact.

#[cfg(feature = "device-audio")]
mod imp {
    use gibson::audio::buffer::StereoBlock;
    use gibson::audio::device::AudioDevice;
    use gibson::audio::human_music::{HumanMusicSynth, MusicWorld, Score};
    use gibson::audio::render::{AudioSource, RenderCtx};
    use gibson::audio::time::{SampleRate, SampleTime};

    use crate::app::App;

    /// A synth that does not know the sample rate until the device tells it.
    struct RateHonestSynth {
        score: Score,
        world: MusicWorld,
        synth: Option<HumanMusicSynth>,
    }

    impl RateHonestSynth {
        fn new(score: Score, world: MusicWorld) -> Self {
            RateHonestSynth {
                score,
                world,
                synth: None,
            }
        }
    }

    impl AudioSource for RateHonestSynth {
        fn render(&mut self, out: &mut StereoBlock, ctx: &RenderCtx) {
            let synth = self
                .synth
                .get_or_insert_with(|| HumanMusicSynth::new(&self.score, &self.world, ctx.sr));
            synth.render(out, ctx);
        }

        fn is_finished(&self, at: SampleTime) -> bool {
            self.synth.as_ref().is_some_and(|s| s.is_finished(at))
        }
    }

    /// A live, in-process stream of the current performance.
    pub struct LiveAudio {
        device: Option<AudioDevice>,
    }

    impl LiveAudio {
        /// Stream the app's current checked take to the default output device.
        ///
        /// Returns the backend's reason if the host has no usable device (headless box, no
        /// audio server, or a device with no f32 stereo configuration); the caller can then
        /// fall back to the exported WAV.
        pub fn start(app: &App) -> Result<LiveAudio, String> {
            let take = app
                .music
                .take
                .as_ref()
                .ok_or_else(|| "no performance to play yet".to_string())?;
            let source = RateHonestSynth::new(take.score.clone(), app.music.world.clone());
            let device = AudioDevice::play(Box::new(source), SampleRate::STUDIO)
                .map_err(|e| e.to_string())?;
            Ok(LiveAudio {
                device: Some(device),
            })
        }

        /// The rate the device actually negotiated and is streaming at.
        pub fn sample_rate(&self) -> u32 {
            self.device
                .as_ref()
                .map(|d| d.sample_rate().get())
                .unwrap_or(0)
        }

        /// Zero-filled samples the callback was forced to emit (starvation meter; 0 is healthy).
        pub fn underruns(&self) -> u64 {
            self.device.as_ref().map(|d| d.underruns()).unwrap_or(0)
        }

        /// Stop and join the stream.
        pub fn stop(&mut self) {
            if let Some(device) = self.device.take() {
                device.stop();
            }
        }
    }

    impl Drop for LiveAudio {
        fn drop(&mut self) {
            self.stop();
        }
    }
}

#[cfg(not(feature = "device-audio"))]
mod silent {
    use crate::app::App;

    /// Device-free stub: the WAV is the only audible artifact.
    pub struct LiveAudio;

    impl LiveAudio {
        pub fn start(_app: &App) -> Result<LiveAudio, String> {
            Err("built without the device-audio feature (no realtime backend)".into())
        }
        pub fn sample_rate(&self) -> u32 {
            0
        }
        pub fn underruns(&self) -> u64 {
            0
        }
        pub fn stop(&mut self) {}
    }
}

#[cfg(feature = "device-audio")]
pub use imp::LiveAudio;

#[cfg(not(feature = "device-audio"))]
pub use silent::LiveAudio;

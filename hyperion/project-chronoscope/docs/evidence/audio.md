## HumanMusic as a time-travel instrument — measurements

Fixture: seed 16; root = 344 steps to Some(Meltdown); fork B = 640 steps (Some(StepCap)). 1 step = 1/4 beat; world tempo 88 BPM; 48 kHz stereo.

| world | strategy | branch | audio s | compose ms | render ms | × realtime | peak | RMS | voices | notes | PCM MiB | hash |
|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| BlackIce | FutureOnly | A | 63.2 | 3 | 1193 | 53 | 0.80 | 0.110 | 16 | 360 | 11.6 | `e2ba0b0930e03a13` |
| BlackIce | FutureOnly | B | 103.1 | 5 | 1994 | 52 | 0.86 | 0.110 | 16 | 592 | 18.9 | `58fe81da44d1a8a0` |
| BlackIce | FullTrace | A | 63.2 | 3 | 1213 | 52 | 0.80 | 0.110 | 16 | 360 | 11.6 | `e2ba0b0930e03a13` |
| BlackIce | FullTrace | B | 103.1 | 5 | 2142 | 48 | 0.83 | 0.109 | 15 | 636 | 18.9 | `fde06acdd39d1bc3` |
| Vapor95 | FutureOnly | A | 66.1 | 3 | 1849 | 36 | 0.69 | 0.091 | 19 | 360 | 12.1 | `c73f44fbc144070c` |
| Vapor95 | FutureOnly | B | 107.9 | 5 | 3069 | 35 | 0.63 | 0.094 | 20 | 592 | 19.7 | `3b50cc7ce4a01e94` |
| SwissSignal | FutureOnly | A | 47.8 | 2 | 867 | 55 | 0.72 | 0.064 | 16 | 360 | 8.7 | `ac2d47ef6cb8953e` |
| SwissSignal | FutureOnly | B | 77.5 | 4 | 1465 | 53 | 0.62 | 0.064 | 18 | 592 | 14.2 | `3ec19b6823a8dd19` |

#### Strategy FutureOnly

**Rebuild after eviction**: first hash `58fe81da44d1a8a0`, rebuilt `58fe81da44d1a8a0` → bit-identical
**Immutable past**: 409091 samples before the fork sample: parent == child → true
**Divergence after the fork**: first differing frame at +Some(103) frames (Some(2.1458333333333335) ms)
**Seam** (crossfade 0.35 s after the fork): max sample step across the seam window 4656 / 32767 (a pure-parent window of equal length: 2629); RMS before 0.0231, after 0.0631

#### Strategy FullTrace

**Rebuild after eviction**: first hash `fde06acdd39d1bc3`, rebuilt `fde06acdd39d1bc3` → bit-identical
**Immutable past**: 409091 samples before the fork sample: parent == child → true
**Divergence after the fork**: first differing frame at +Some(4) frames (Some(0.08333333333333333) ms)
**Seam** (crossfade 0.35 s after the fork): max sample step across the seam window 4621 / 32767 (a pure-parent window of equal length: 2629); RMS before 0.0231, after 0.0231
**Prefix causality (raw FullTrace performances)**: parent and child raw PCM first differ at frame Some(0) (Some(0.0) s); the fork is at frame 409091 (8.52 s)

**Harmony at the fork** (beat 12.5): parent chord Some("9Min"); child FutureOnly beat 0 Some("1Maj"); child FullTrace at the fork beat Some("9Min").
**Prefix harmony stability** (full-trace child vs parent, per beat over the shared 12 beats): 0/12 beats carry the same chord; tempo 88 vs 88 BPM; total beats 89 vs 160.

**Chord agreement at the fork over 27 fork points** (steps 40..300): FullTrace child == parent chord at 17/27, mean pitch-class overlap 0.79; FutureOnly child's first chord == parent chord at 10/27, overlap 0.59.

**Seek**: PCM offset fetch of 4096 frames = 9.2 µs. Honest reconstruction (fresh synth, render-and-discard the prefix): to 10.0 s = 153 ms; to 20.0 s = 344 ms; to 40.0 s = 715 ms; to 63.2 s = 1141 ms; 

Audio store after these builds: 2 resident, 30.4 MiB, builds 2, rebuilds 0, evictions 0, rebuild hash mismatches 0.

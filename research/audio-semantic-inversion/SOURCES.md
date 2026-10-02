# Candidate inference instruments

Checked 2026-10-02. These are candidate measurement instruments, not authorities and not
part of the semantic definition. Pin exact versions before use.

## MuScriptor

- Project: https://github.com/muscriptor/muscriptor
- Paper: "MuScriptor: An Open Model for Multi-Instrument Music Transcription" (2026),
  arXiv:2607.08168.
- Intended role: full-mix, multi-instrument note/instrument transcription.
- Repository code license: MIT.
- Published model weights are separately licensed CC BY-NC 4.0 according to the project
  README; verify the exact selected weights before use.
- Practical warning: heavyweight model; steady-tempo material is easier for its notation
  quantization. Treat its MIDI as derived evidence, not ground truth.

## Beat This!

- Project: https://github.com/CPJKU/beat_this
- Paper: "Beat This! Accurate Beat Tracking Without DBN Postprocessing", ISMIR 2024.
- Intended role: beat/downbeat evidence and metric transport.
- Project states code and published model weights are MIT licensed.
- CPU inference is supported; GPU is optional.

## Spotify Basic Pitch

- Project: https://github.com/spotify/basic-pitch
- Intended role: lightweight alternate note/pitch transcription route, especially on
  isolated/simplified material.
- Instrument-agnostic and polyphonic, but the project explicitly notes it works best on
  one instrument at a time.
- Useful as a route with different failure characteristics from a full-mix
  instrument-labelled transcriber.

## Essentia tonal analysis

- Project/docs: https://essentia.upf.edu/
- Intended role: HPCP/chroma, key and chord candidates independent of note transcription.
- \`ChordsDetection\` and \`ChordsDetectionBeats\` provide major/minor chord estimates from
  HPCP evidence.
- Essentia's own documentation labels chord detection experimental/prone to errors. This is
  a feature of the research design: it is a competing constraint surface, not an oracle.

## Demucs family

- Historical repository: https://github.com/facebookresearch/demucs
- Intended role: optional source-separation ablation before per-stem transcription.
- The original Meta repository is archived/read-only; it points to the author's fork.
- Do not make separation a prerequisite for the first full-mix claim. Separation can add
  artifacts and leakage of its own, so compare full-mix and separation-assisted routes.

## Selection rule

Choose the weakest sufficient set of instruments for each experiment. Record:

- exact code commit/package version;
- exact model/weights identifier;
- license of code and weights separately;
- configuration and command line;
- CPU/GPU backend;
- raw output artifact hash.

If a candidate cannot run on the experiment machine, preserve that as an access/compute
constraint and use cached or alternate evidence. Do not translate "tool unavailable" into
"musical information absent".

#!/usr/bin/env python3
"""Exploratory harmonic dictionary competition; never imports the frozen evaluator.

Known synthetic generators and inverse dictionary intentionally differ. The
spectral objective concerns note proposals, never source/cover PCM similarity.
"""
from __future__ import annotations
import argparse
import hashlib
import importlib.util
import json
import platform
import time
from pathlib import Path
import numpy as np
import scipy
from scipy.optimize import nnls
import librosa
import soundfile as sf

SR, HOP, FFT = 22050, 512, 8192
PITCHES = np.arange(36, 96)
FAMILIES = ('decay1', 'decay2', 'odd', 'weak_fundamental')
CONTRACT = {
    'sample_rate': SR, 'hop': HOP, 'fft': FFT, 'pitch_range_inclusive': [36, 95],
    'families': FAMILIES, 'partials': 12, 'max_atoms': 6,
    'baseline_threshold_db': -40,
    'minimum_step_energy_fraction': .025, 'minimum_total_explained_fraction': .60,
    'minimum_coefficient_fraction': .12, 'minimum_frame_rms': 1e-5,
    'minimum_event_duration_sec': .06, 'rng_seed': 2026100401,
    'tuning': 'Predeclared; no search or adjustment after outcome inspection.',
    'decision': 'Candidate supported on this fixture set only if pitched micro F1 improves, neither null emits notes, and octave pair recall is at least .8; otherwise retain the exact failure.',
}


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def dictionary():
    """Analytic centered Hann magnitude lobes, not sampled training instruments."""
    bins = np.arange(FFT // 2 + 1)
    columns, ids = [], []
    for midi in PITCHES:
        fundamental = 440 * 2 ** ((midi - 69) / 12)
        for family in FAMILIES:
            atom = np.zeros(len(bins))
            for h in range(1, 13):
                hz = h * fundamental
                if hz >= SR / 2:
                    break
                weight = 1 / h ** (2 if family == 'decay2' else 1)
                if family == 'odd' and h % 2 == 0:
                    weight = 0
                if family == 'weak_fundamental' and h == 1:
                    weight *= .08
                z = bins - hz * FFT / SR
                local = np.abs(z) <= 4
                atom[local] += weight * np.abs(.5 * np.sinc(z[local]) + .25 * np.sinc(z[local] - 1) + .25 * np.sinc(z[local] + 1))
            atom /= max(np.linalg.norm(atom), 1e-20)
            columns.append(atom)
            ids.append((int(midi), family))
    return np.asarray(columns).T, ids


def segment(active, times):
    notes = []
    for pitch in PITCHES:
        flags = np.array([int(pitch) in row for row in active])
        edges = np.diff(np.r_[False, flags, False].astype(int))
        for start, stop in zip(np.where(edges == 1)[0], np.where(edges == -1)[0]):
            end = min(float(times[-1]), stop * HOP / SR)
            if end - times[start] >= .06:
                notes.append({'onset_sec': float(times[start]), 'offset_sec': end, 'pitch_midi': int(pitch)})
    return sorted(notes, key=lambda n: (n['onset_sec'], n['pitch_midi']))


def competition(y, D, ids):
    spectrum = np.abs(librosa.stft(y, n_fft=FFT, hop_length=HOP))
    times = librosa.frames_to_time(np.arange(spectrum.shape[1]), sr=SR, hop_length=HOP)
    active, receipts = [], []
    for frame in range(spectrum.shape[1]):
        b = spectrum[:, frame]
        total = float(b @ b)
        center = int(frame * HOP)
        crop = y[max(0, center - FFT // 2):min(len(y), center + FFT // 2)]
        if len(crop) == 0 or np.sqrt(np.mean(crop ** 2)) < 1e-5 or total < 1e-12:
            active.append(set()); receipts.append({'explained': 0., 'atoms': [], 'accepted': False}); continue
        residual, selected, coef = b.copy(), [], np.array([])
        for _ in range(6):
            scores = D.T @ residual
            if selected:
                scores[selected] = -np.inf
            chosen = int(np.argmax(scores))
            if scores[chosen] <= 0:
                break
            trial = selected + [chosen]
            trial_coef, _ = nnls(D[:, trial], b)
            trial_residual = b - D[:, trial] @ trial_coef
            improvement = (residual @ residual - trial_residual @ trial_residual) / total
            if improvement < .025:
                break
            selected, coef, residual = trial, trial_coef, trial_residual
        explained = float(1 - residual @ residual / total)
        atoms = [{'pitch': ids[j][0], 'family': ids[j][1], 'coefficient': float(c)} for j, c in zip(selected, coef)]
        accepted = explained >= .60
        peaks = {row['pitch'] for row in atoms if accepted and row['coefficient'] >= .12 * max(coef, default=0.)}
        active.append(peaks)
        receipts.append({'explained': explained, 'atoms': atoms, 'accepted': accepted})
    return segment(active, times), receipts


def fixture(name, events, rng, noise=0., duration=2.8):
    y = np.zeros(round(duration * SR))
    truth = []
    for midi, onset, offset, flavor, level in events:
        start, end = round(onset * SR), round(offset * SR)
        t = np.arange(end - start) / SR
        f = 440 * 2 ** ((midi - 69) / 12)
        signal = np.zeros(len(t))
        for h in range(1, 17):
            weight = 1 / h ** 1.35
            if flavor == 'strong_overtones':
                weight = (.12 if h == 1 else (1. if h == 2 else .8 if h == 3 else 1 / h))
            elif flavor == 'missing_fundamental':
                weight = 0 if h == 1 else 1 / h
            elif flavor == 'odd':
                weight = weight if h % 2 else 0
            elif flavor == 'sine':
                weight = float(h == 1)
            freq = h * f * (1.009 if flavor == 'detuned' else np.sqrt(1 + .0006 * h * h) if flavor == 'inharmonic' else 1)
            if freq < SR / 2:
                signal += weight * np.sin(2 * np.pi * freq * t + rng.uniform(0, 2 * np.pi))
        edge = min(round(.018 * SR), len(t) // 2)
        envelope = np.ones(len(t)); envelope[:edge] = np.linspace(0, 1, edge); envelope[-edge:] = np.linspace(1, 0, edge)
        y[start:end] += level * signal * envelope
        truth.append({'onset_sec': onset, 'offset_sec': offset, 'pitch_midi': midi})
    if noise:
        y += noise * rng.normal(size=len(y))
    peak = max(np.max(np.abs(y)), 1.)
    return name, (y / peak).astype(np.float32), truth


def occupancy(notes, duration):
    times = np.arange(0, duration, HOP / SR)
    matrix = np.zeros((len(PITCHES), len(times)), dtype=bool)
    for n in notes:
        pitch = int(round(n['pitch_midi']))
        if 36 <= pitch <= 95:
            matrix[pitch - 36] |= (times >= n['onset_sec']) & (times < n['offset_sec'])
    return matrix


def measure(truth, notes, duration):
    target, predicted = occupancy(truth, duration), occupancy(notes, duration)
    tp = int((target & predicted).sum()); fp = int((~target & predicted).sum()); fn = int((target & ~predicted).sum())
    return {'tp': tp, 'fp': fp, 'fn': fn, 'precision': tp / max(1, tp + fp), 'recall': tp / max(1, tp + fn), 'f1': 2 * tp / max(1, 2 * tp + fp + fn), 'event_count': len(notes)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--mir-lib', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    if args.out.exists() and any(args.out.iterdir()):
        raise SystemExit('Refusing to overwrite an existing probe output')
    args.out.mkdir(parents=True, exist_ok=True)
    contract = dict(CONTRACT, script_sha256=sha(__file__), archived_mir_lib_sha256=sha(args.mir_lib))
    (args.out / 'PREREGISTRATION.json').write_text(json.dumps(contract, indent=2) + '\n')
    spec = importlib.util.spec_from_file_location('archived_mir', args.mir_lib)
    module = importlib.util.module_from_spec(spec); spec.loader.exec_module(module)
    rng = np.random.default_rng(CONTRACT['rng_seed'])
    E = lambda pitches, kind='regular': [(p, .3, 2.4, kind, .22) for p in pitches]
    cases = [fixture('triad_regular', E([48, 52, 55]), rng),
             fixture('triad_strong_overtones', E([48, 52, 55], 'strong_overtones'), rng),
             fixture('single_missing_fundamental', E([48], 'missing_fundamental'), rng),
             fixture('real_octave_pair', E([48, 60]), rng),
             fixture('odd_partial_triad', E([48, 51, 55], 'odd'), rng),
             fixture('detuned_triad', E([48, 52, 55], 'detuned'), rng),
             fixture('inharmonic_triad', E([48, 52, 55], 'inharmonic'), rng),
             fixture('staggered_phrase', [(48,.2,.9,'regular',.22),(52,.57,1.31,'regular',.22),(55,1.11,2.42,'regular',.22)], rng),
             fixture('quiet_triad_in_noise', [(p,.3,2.4,'regular',.04) for p in [48,52,55]], rng, noise=.05),
             fixture('silence', [], rng), fixture('noise', [], rng, noise=.07)]
    D, ids = dictionary()
    report = {'schema': 'sai.harmonic_competition_probe/v1', 'contract': contract,
              'versions': {'python': platform.python_version(), 'numpy': np.__version__, 'scipy': scipy.__version__, 'librosa': librosa.__version__},
              'boundary': 'Exploratory synthetic probe. No source replacement, no pin promotion, no holdout access. Analytic harmonic prior and synthetic additive generator share harmonic assumptions; off-model controls explicitly retained.', 'cases': []}
    for name, y, truth in cases:
        tic = time.monotonic()
        baseline, _ = module.cqt_poly_notes(y, SR, threshold_db=CONTRACT['baseline_threshold_db'])
        proposed, frames = competition(y, D, ids)
        item = {'name': name, 'truth': truth, 'pcm_sha256': hashlib.sha256(y.tobytes()).hexdigest(),
                'archived_cqt': {'notes': baseline, 'metrics': measure(truth, baseline, len(y)/SR)},
                'harmonic_competition': {'notes': proposed, 'metrics': measure(truth, proposed, len(y)/SR), 'mean_spectral_explained_energy': float(np.mean([f['explained'] for f in frames]))},
                'seconds': time.monotonic()-tic}
        sf.write(args.out / f'{name}.wav', y, SR, subtype='PCM_16')
        (args.out / f'{name}_frames.json').write_text(json.dumps(frames, indent=2)+'\n')
        report['cases'].append(item)
        print(name, json.dumps({key:item[key]['metrics'] for key in ('archived_cqt','harmonic_competition')}), flush=True)
    report['aggregate_pitched'] = {}
    for route in ('archived_cqt','harmonic_competition'):
        total = {k:sum(row[route]['metrics'][k] for row in report['cases'] if row['truth']) for k in ('tp','fp','fn')}
        total['f1'] = 2*total['tp']/max(1,2*total['tp']+total['fp']+total['fn'])
        report['aggregate_pitched'][route] = total
    null_ok = all(row['harmonic_competition']['metrics']['event_count']==0 for row in report['cases'] if not row['truth'])
    octave = next(row for row in report['cases'] if row['name']=='real_octave_pair')['harmonic_competition']['metrics']['recall']
    report['predeclared_success'] = (report['aggregate_pitched']['harmonic_competition']['f1'] > report['aggregate_pitched']['archived_cqt']['f1'] and null_ok and octave >= .8)
    report['nulls_pass'] = null_ok
    report['octave_recall'] = octave
    (args.out / 'RESULTS.json').write_text(json.dumps(report, indent=2)+'\n')
    print(json.dumps({k:report[k] for k in ('aggregate_pitched','predeclared_success','nulls_pass','octave_recall')},indent=2))


if __name__ == '__main__':
    main()

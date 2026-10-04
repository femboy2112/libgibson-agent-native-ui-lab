#!/usr/bin/env python3
"""Read-only numeric audit supplement; never unpickles or rewrites source artifacts.

Requires numpy and soundfile. Exit 2 means observed invalid/unsupported contents,
not that the audit failed to produce its JSON receipt. No decoding of MP3 sources.
"""
from __future__ import annotations
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import tempfile
import time

import numpy as np
import soundfile as sf

TIME_AXES = frozenset({'times', 'time_sec', 'time_seconds', 'timestamps', 'onset_times', 'offset_times'})
FRAME_TIME_TOLERANCE_SECONDS = .1


def sha256(path):
    h = hashlib.sha256()
    with Path(path).open('rb') as stream:
        for block in iter(lambda: stream.read(8 * 1024 * 1024), b''):
            h.update(block)
    return h.hexdigest()


def inspect_npz(path, duration=None):
    result = {'kind': 'npz', 'arrays': [], 'issues': []}
    try:
        with np.load(path, allow_pickle=False) as archive:
            for name in archive.files:
                entry = {'name': name}
                try:
                    array = archive[name]
                except ValueError as error:
                    result['issues'].append({'kind': 'unsupported_array', 'array': name, 'reason': str(error)})
                    entry['status'] = 'unsupported_without_pickle'
                    result['arrays'].append(entry)
                    continue
                entry.update(dtype=str(array.dtype), shape=list(array.shape), elements=array.size)
                if array.dtype.hasobject:
                    result['issues'].append({'kind': 'unsupported_array', 'array': name, 'reason': 'object dtype is never unpickled'})
                elif np.issubdtype(array.dtype, np.number) or np.issubdtype(array.dtype, np.bool_):
                    finite = np.isfinite(array)
                    count = int(array.size - np.count_nonzero(finite))
                    entry['nonfinite_elements'] = count
                    if count:
                        result['issues'].append({'kind': 'nonfinite_array', 'array': name, 'count': count})
                    if name in TIME_AXES:
                        entry['declared_time_axis'] = True
                        if array.ndim != 1 or np.iscomplexobj(array):
                            result['issues'].append({'kind': 'invalid_time_axis_shape_or_type', 'array': name})
                        elif not count:
                            negative = int(np.count_nonzero(array < 0))
                            # All corpus names denote sample/frame timestamps, not simultaneous event groups.
                            nonincreasing = int(np.count_nonzero(np.diff(array.astype(np.float64)) <= 0))
                            entry.update(negative_times=negative, nonincreasing_steps=nonincreasing)
                            if negative or nonincreasing:
                                result['issues'].append({'kind': 'invalid_time_axis', 'array': name, 'negative_times': negative, 'nonincreasing_steps': nonincreasing})
                            if duration is not None:
                                beyond = int(np.count_nonzero(array > duration + FRAME_TIME_TOLERANCE_SECONDS))
                                entry['beyond_source_duration'] = beyond
                                if beyond:
                                    result['issues'].append({'kind': 'time_beyond_source', 'array': name, 'count': beyond})
                elif array.dtype.kind in 'US':
                    entry['status'] = 'string_metadata_not_numeric'
                else:
                    result['issues'].append({'kind': 'unsupported_dtype', 'array': name, 'dtype': str(array.dtype)})
                result['arrays'].append(entry)
    except Exception as error:
        result['issues'].append({'kind': 'unreadable_npz', 'reason': str(error)})
    return result


def inspect_wav(path, duration=None):
    result = {'kind': 'wav', 'issues': []}
    try:
        with sf.SoundFile(path) as audio:
            result.update(format=audio.format, subtype=audio.subtype, sample_rate=audio.samplerate,
                          channels=audio.channels, frames=audio.frames, duration_sec=audio.frames/audio.samplerate)
            if audio.frames <= 0 or audio.samplerate <= 0 or audio.channels <= 0:
                result['issues'].append({'kind': 'invalid_wav_header'})
            if audio.format not in ('WAV', 'WAVEX', 'RF64'):
                result['issues'].append({'kind': 'unexpected_audio_container', 'format': audio.format})
            if duration is not None and abs(result['duration_sec'] - duration) > FRAME_TIME_TOLERANCE_SECONDS:
                result['issues'].append({'kind': 'wav_source_duration_mismatch', 'delta_sec': result['duration_sec']-duration})
            if audio.subtype in ('FLOAT', 'DOUBLE'):
                nonfinite = 0
                frames_read = 0
                for block in audio.blocks(blocksize=262144, dtype='float64', always_2d=True):
                    nonfinite += int(block.size - np.count_nonzero(np.isfinite(block)))
                    frames_read += len(block)
                result.update(validation='all_float_pcm_samples_scanned', frames_read=frames_read,
                              nonfinite_samples=nonfinite)
                if nonfinite:
                    result['issues'].append({'kind': 'nonfinite_pcm', 'count': nonfinite})
                if frames_read != audio.frames:
                    result['issues'].append({'kind': 'wav_frame_count_mismatch', 'frames_read': frames_read})
            elif audio.subtype.startswith('PCM_'):
                result.update(validation='integer_pcm_cannot_encode_nan_or_inf', nonfinite_samples=0)
                # Verify readable final frame without scanning integer samples for impossible NaN.
                if audio.frames:
                    audio.seek(audio.frames-1)
                    if len(audio.read(1, dtype='int32', always_2d=True)) != 1:
                        result['issues'].append({'kind': 'unreadable_last_frame'})
            else:
                result['issues'].append({'kind': 'unsupported_audio_subtype', 'subtype': audio.subtype})
    except Exception as error:
        result['issues'].append({'kind': 'unreadable_wav', 'reason': str(error)})
    return result


def audit(corpus, output):
    started=time.monotonic()
    manifest_path=corpus/'MANIFEST.json'
    manifest=json.loads(manifest_path.read_text())
    durations={t['track_id']:t['duration_sec'] for t in manifest['tracks']}
    files=sorted(p for p in corpus.rglob('*') if p.is_file() and p.suffix.lower() in ('.npz','.wav'))
    report={'schema':'sai.numeric_artifact_audit/v1', 'corpus':str(corpus.resolve()),
            'source_manifest_sha256':sha256(manifest_path), 'validator_sha256':sha256(__file__),
            'versions':{'numpy':np.__version__, 'soundfile':sf.__version__, 'libsndfile':sf.__libsndfile_version__},
            'policy':{'npz_allow_pickle':False, 'time_axes':sorted(TIME_AXES),
                      'strictly_monotonic_frame_times':True, 'duration_tolerance_seconds':FRAME_TIME_TOLERANCE_SECONDS,
                      'float_pcm':'scan_every_sample', 'integer_pcm':'header_and_last_frame; integer_cannot_encode_nonfinite'},
            'artifacts':[]}
    for i,path in enumerate(files):
        before=path.stat()
        relative=path.relative_to(corpus)
        duration=durations.get(relative.parts[0])
        entry=inspect_npz(path,duration) if path.suffix.lower()=='.npz' else inspect_wav(path,duration)
        entry.update(path=str(relative), size_bytes=before.st_size, sha256=sha256(path))
        after=path.stat()
        if (before.st_size,before.st_mtime_ns)!=(after.st_size,after.st_mtime_ns):
            entry['issues'].append({'kind':'artifact_changed_during_audit'})
        entry['valid']=not entry['issues']
        report['artifacts'].append(entry)
        if (i+1)%25==0 or i+1==len(files):
            print(f'Validated {i+1}/{len(files)} numeric artifacts',flush=True)
    report['summary']={'files':len(files), 'kinds':dict(Counter(e['kind'] for e in report['artifacts'])),
                       'bytes':sum(e['size_bytes'] for e in report['artifacts']),
                       'valid_files':sum(e['valid'] for e in report['artifacts']),
                       'invalid_or_unsupported_files':sum(not e['valid'] for e in report['artifacts']),
                       'issue_counts':dict(Counter(issue['kind'] for e in report['artifacts'] for issue in e['issues'])),
                       'nonfinite_array_elements':sum(a.get('nonfinite_elements',0) for e in report['artifacts'] for a in e.get('arrays',[])),
                       'nonfinite_pcm_samples':sum(e.get('nonfinite_samples',0) for e in report['artifacts']),
                       'seconds':time.monotonic()-started}
    output.parent.mkdir(parents=True,exist_ok=True)
    output.write_text(json.dumps(report,indent=2,allow_nan=False)+'\n')
    return report


def self_test():
    with tempfile.TemporaryDirectory() as tmp:
        root=Path(tmp)
        np.savez(root/'valid.npz',times=np.array([0.,.5,1.]),data=np.arange(3),labels=np.array(['a']))
        assert not inspect_npz(root/'valid.npz',1)['issues']
        np.savez(root/'bad.npz',times=np.array([0.,.5,.4]),data=np.array([np.nan,np.inf]),objects=np.array([{'x':1}],dtype=object))
        issues={x['kind'] for x in inspect_npz(root/'bad.npz',1)['issues']}
        assert {'invalid_time_axis','nonfinite_array','unsupported_array'}<=issues
        sf.write(root/'valid.wav',np.zeros(100),1000,subtype='PCM_16')
        assert not inspect_wav(root/'valid.wav',.1)['issues']
        sf.write(root/'bad.wav',np.array([0.,np.nan,np.inf]),1000,subtype='FLOAT')
        assert any(x['kind']=='nonfinite_pcm' and x['count']==2 for x in inspect_wav(root/'bad.wav')['issues'])
        (root/'broken.wav').write_bytes(b'broken')
        assert inspect_wav(root/'broken.wav')['issues'][0]['kind']=='unreadable_wav'
        assert any(x['kind']=='time_beyond_source' for x in inspect_npz(root/'valid.npz',.1)['issues'])
    print('Numeric audit synthetic positive/negative controls passed')


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--corpus',type=Path)
    parser.add_argument('--out',type=Path)
    parser.add_argument('--self-test',action='store_true')
    args=parser.parse_args()
    if args.self_test:
        self_test();return 0
    if args.corpus is None or args.out is None:
        parser.error('--corpus and --out are required unless --self-test')
    report=audit(args.corpus,args.out)
    print(json.dumps(report['summary'],indent=2))
    return 2 if report['summary']['invalid_or_unsupported_files'] else 0


if __name__=='__main__':
    raise SystemExit(main())

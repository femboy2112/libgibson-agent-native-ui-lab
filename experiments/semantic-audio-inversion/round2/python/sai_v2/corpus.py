"""Read-only DeepSeek corpus audit and additive stem-aware import.

No inferred label is ground truth. No source-specific decisions. Real musical event
content is only written to an explicitly supplied local output directory.
"""
from __future__ import annotations
import argparse
import bisect
import csv
import hashlib
import json
import math
from pathlib import Path
import statistics
import subprocess
import tarfile
from collections import Counter
from .model import StemEvidence, SCHEMA, finite_tree


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(8 * 1024 * 1024), b''):
            h.update(block)
    return h.hexdigest()


def read_json(path):
    return json.loads(Path(path).read_text())


def read_csv(path):
    with Path(path).open(newline='') as f:
        return list(csv.DictReader(f))


class BeatTransport:
    """Observed beat interpolation, retaining absolute recording-time origin."""
    def __init__(self, times, bpm):
        self.times = times
        self.period = 60 / bpm
        if not math.isfinite(bpm) or bpm <= 0 or any(not math.isfinite(t) for t in times):
            raise ValueError('invalid beat transport')
        if len(times) < 2 or any(b <= a for a, b in zip(times, times[1:])):
            raise ValueError('need strictly monotonic observed beats')
        self.origin = self._raw(0.)

    def _raw(self, t):
        i = bisect.bisect_right(self.times, t) - 1
        i = max(0, min(i, len(self.times) - 2))
        return i + (t - self.times[i]) / (self.times[i+1] - self.times[i])

    def __call__(self, t):
        return self._raw(t) - self.origin


def _prov(path, **kwargs):
    return {'artifact': str(path), 'artifact_sha256': sha256(path), 'level': 'DerivedAnalysis', 'dependence_family': 'deepseek-librosa-demucs', **kwargs}


def role_hypotheses(stem, notes):
    if not notes:
        return [{'role': 'unknown', 'support': None, 'reason': 'no note observations'}]
    order = sorted(notes, key=lambda x: x['onset_sec'])
    overlaps = sum(a['offset_sec'] > b['onset_sec'] + .025 for a, b in zip(order, order[1:])) / max(1, len(order)-1)
    median = statistics.median(n['midi'] for n in notes)
    mono = overlaps <= .1
    # Support values are descriptive heuristics, never calibrated probabilities.
    if stem == 'bass' and median < 60:
        return [{'role': 'bass', 'support': .85, 'reason': 'low register plus separator bass hypothesis'}, {'role': 'riff', 'support': .4, 'reason': 'identity-bearing bass remains possible'}]
    if mono and median >= 48:
        return [{'role': 'lead', 'support': .8 if stem == 'vocals' else .6, 'reason': 'monophonic events plus melodic register'}, {'role': 'riff', 'support': .5, 'reason': 'monophony does not establish principal melody'}]
    return [{'role': 'support', 'support': .65, 'reason': 'overlapping pitched events'}, {'role': 'riff', 'support': .35, 'reason': 'polyphonic separation may conceal a melodic line'}]


def import_track(track_dir: Path, source: dict) -> StemEvidence:
    track_dir = Path(track_dir)
    bp_path = track_dir/'reverse_humanmusic/cover_blueprint.v1.json'
    bp = read_json(bp_path)
    if bp['source_identity']['track_id'] != source['track_id'] or bp['source_identity']['sha256'] != source['sha256']:
        raise ValueError('blueprint source identity mismatch')
    g = bp['global']
    beats_path = track_dir/'events/beats.csv'
    beats = read_csv(beats_path)
    bpm = float(g['tempo_bpm'])
    transport = BeatTransport([float(b['time_sec']) for b in beats], bpm)
    duration = float(g['duration_sec'])
    notes, lanes, quarantine = [], [], []
    for path in sorted((track_dir/'events').glob('*notes.csv')):
        if path.name == 'vocal_notes.csv':
            model, stem = 'htdemucs_ft', 'vocals'
        elif path.name == 'bass_notes.csv':
            model, stem = 'htdemucs_ft', 'bass'
        else:
            model, stem = path.stem.removesuffix('_notes').rsplit('_', 1)
        provenance = _prov(path, separator='demucs', model=model, stem_label=stem, version=None)
        events = []
        for idx, row in enumerate(read_csv(path)):
            try:
                start, end, pitch, confidence = (float(row[k]) for k in ('onset_sec', 'offset_sec', 'pitch_midi', 'confidence'))
                if not all(math.isfinite(v) for v in (start,end,pitch,confidence)) or not (0 <= start < end <= duration+.1 and 0 <= pitch <= 127 and 0 <= confidence <= 1):
                    raise ValueError('invalid note numeric bounds')
                events.append({'onset_sec': start, 'offset_sec': end, 'onset_beat': transport(start), 'duration_beats': transport(end)-transport(start), 'midi': pitch, 'velocity': .75, 'confidence': confidence, 'articulation': row.get('articulation'), 'f0_route': row.get('route', 'cqt_polyphonic_peaks'), 'alternate_route_support': None if 'yin_agree' not in row else row['yin_agree']=='1', 'provenance': provenance})
            except (ValueError, KeyError) as exc:
                quarantine.append({'artifact': str(path), 'row': idx+2, 'reason': str(exc)})
        hypotheses = role_hypotheses(stem, events)
        role = hypotheses[0]['role']
        for event in events:
            event['role'] = role
        notes.extend(events)
        lanes.append({'lane_id': f'{model}:{stem}', 'stem_identity': provenance, 'role_hypotheses': hypotheses, 'note_count': len(events), 'events': events, 'separation_confidence': None})
    harmony_path = track_dir/'harmony/spans.json'
    harmony = []
    harmony_prov = _prov(harmony_path)
    bp_prov = _prov(bp_path)
    for x in read_json(harmony_path)['spans']:
        if x['end_sec'] <= x['start_sec']:
            quarantine.append({'artifact': str(harmony_path), 'reason': 'nonpositive harmonic span'})
            continue
        harmony.append({'start_beat': transport(x['start_sec']), 'end_beat': transport(x['end_sec']), 'start_sec': x['start_sec'], 'end_sec': x['end_sec'], 'root': x['root_pc'], 'quality': x['quality'], 'bass_pc': x.get('bass_pc'), 'inversion': x.get('inversion'), 'confidence': x.get('confidence'), 'rivals': x.get('rivals', []), 'routes': x.get('routes', []), 'raw_metric_indices': [x.get('start_beat'),x.get('end_beat')], 'provenance': harmony_prov})
    sections = [{'start_beat': transport(s['start_second']), 'end_beat': transport(s['end_second']), 'start_sec': s['start_second'], 'end_sec': s['end_second'], 'label': s['label_candidate'], 'confidence': s.get('confidence'), 'provenance': bp_prov} for s in bp['form']['sections']]
    drum_path = track_dir/'events/drums.csv'
    drum_prov = _prov(drum_path, model=bp['groove'].get('model'), stem_label='drums')
    drums = [{'onset_beat': transport(float(r['time_sec'])), 'onset_sec': float(r['time_sec']), 'family': r['family'], 'confidence': float(r['confidence']), 'microtiming_beats': float(r['microtiming_beats']), 'raw_metric_beat': float(r['beat']), 'fill_status': 'unknown', 'provenance': drum_prov} for r in read_csv(drum_path)]
    # Canonical pocket membership requires recurrence, not a lone rounded onset.
    # Unknown fills stay unknown. Canonical index and raw recording coordinate coexist.
    bpb = int(g['meter'].get('bpb') or 4)
    bins = Counter()
    for event in drums:
        raw = transport._raw(event['onset_sec'])
        index = round(raw)
        event['canonical_beat_index'] = index
        event['metric_onset_beat'] = index - transport.origin
        event['microtiming_observed_beats'] = raw - index
        event['is_fill'] = None
        if abs(raw-index) <= .15:
            bins[(index % bpb, event['family'])] += 1
    cycles = max(1., len(beats)/bpb)
    for event in drums:
        recurrence = bins[(event['canonical_beat_index'] % bpb, event['family'])] / cycles
        event['pocket_recurrence_fraction'] = min(1., recurrence)
        event['pocket_member'] = abs(event['microtiming_observed_beats']) <= .15 and recurrence >= .25
    # These are ceilings for observations, not declarations of faithful source identity.
    statuses = {'motif': 'metric' if sum(n['role']=='lead' for n in notes) >= 4 else 'unknown', 'bass': 'metric' if sum(n['role']=='bass' for n in notes) >= 4 else 'unknown', 'harmony': 'ordered' if harmony else 'unknown', 'groove': 'pocket_skeleton' if sum(d['pocket_member'] for d in drums) >= 2 else 'unknown', 'form': 'topology' if sections else 'unknown', 'orchestration': 'unknown'}
    production = bp.get('production', {})
    # Preserve invalid production values by reference, not as usable parameters.
    def sanitize(value, path='production'):
        if isinstance(value,dict):
            return {k: sanitize(v,path+'.'+k) for k,v in value.items()}
        if isinstance(value,list):
            return [sanitize(v,path) for v in value]
        if isinstance(value,(int,float)) and (not math.isfinite(value) or ('attack_ms' in path and value < 0)):
            quarantine.append({'artifact': str(bp_path), 'field': path, 'reason': 'invalid production measurement'})
            return None
        return value
    d = {'schema': SCHEMA, 'source_id': source['track_id'], 'source_sha256': source['sha256'], 'blueprint_sha256': sha256(bp_path), 'duration_sec': duration, 'duration_beats': transport(duration), 'tempo_bpm': bpm, 'tempo_rivals': g.get('tempo_candidates', []), 'meter_beats': g['meter'].get('bpb'), 'meter_evidence': g['meter'], 'beat_origin_offset': -transport.origin, 'downbeat_phase_index': g['meter'].get('phase'), 'key_root': g.get('tonic_pc'), 'key_rivals': g.get('key_candidates', []), 'notes': sorted(notes,key=lambda n:(n['onset_sec'],n['role'],n['midi'])), 'lanes': lanes, 'harmony': harmony, 'sections': sections, 'drums': drums, 'motif_families': bp.get('motifs', []), 'form_recurrence': bp['form'].get('recurrence_links',[]), 'orchestration': bp.get('orchestration'), 'production': sanitize(production), 'separator_disagreement': read_json(track_dir/'separation/comparison/cross_model.json'), 'evidence_status': statuses, 'unknowns': ['Principal melody identity is a rival hypothesis, not established by stem label.', 'Beat/meter phase and tempo octave remain ambiguous.', 'Harmony rivals remain unresolved; exact quality not pinned.', 'Fills, exact orchestration seats, lyrics and authorial intent unknown.', 'Separator model confidence and release version absent at event level.'], 'quarantine': quarantine, 'provenance': {'blueprint': _prov(bp_path), 'beat_grid': _prov(beats_path), 'transport': 'piecewise_linear_observed_beats_recording_origin', 'source': {'sha256':source['sha256'], 'path':source['source_path']}, 'raw_stem_observatory': _prov(track_dir/'reverse_humanmusic/stem_observatory.v1.json')}}
    return StemEvidence(d)


def verify_archive_members(path: Path, expected: dict):
    """Hash streamed members without extracting private audio or trusting tar paths."""
    result = {'checked': 0, 'mismatches': [], 'missing': [], 'unexpected': []}
    seen = set()
    process = subprocess.Popen(['zstd', '-dc', str(path)], stdout=subprocess.PIPE)
    try:
        with tarfile.open(fileobj=process.stdout, mode='r|') as archive:
            for member in archive:
                if not member.isfile():
                    continue
                name = member.name.removeprefix('./')
                seen.add(name)
                h = hashlib.sha256()
                stream = archive.extractfile(member)
                for block in iter(lambda: stream.read(8*1024*1024), b''):
                    h.update(block)
                result['checked'] += 1
                if name not in expected:
                    result['unexpected'].append(name)
                elif h.hexdigest() != expected[name]['sha256'] or member.size != expected[name]['size_bytes']:
                    result['mismatches'].append({'member':name, 'actual_sha256':h.hexdigest(), 'expected_sha256':expected[name]['sha256'], 'actual_size_bytes':member.size, 'expected_size_bytes':expected[name]['size_bytes']})
        process.stdout.close()
        if process.wait() != 0:
            raise ValueError('zstd archive decode failed')
    finally:
        if process.poll() is None:
            process.kill();process.wait()
    result['missing'] = sorted(set(expected) - seen)
    return result


def audit(corpus: Path, output: Path, hash_bundles=True):
    """Exhaustive artifact inventory, source/stem hashes, JSON/CSV structural checks."""
    corpus, output = Path(corpus), Path(output)
    output.mkdir(parents=True,exist_ok=True)
    manifest = read_json(corpus/'MANIFEST.json')
    cache = {}
    def digest(p):
        p=Path(p)
        if str(p) not in cache:
            cache[str(p)]=sha256(p)
        return cache[str(p)]
    report={'schema':'sai.corpus_audit/v2','manifest':{'path':str(corpus/'MANIFEST.json'),'sha256':digest(corpus/'MANIFEST.json')},'tracks':[], 'bundle_checks':[], 'issues':[]}
    for src in manifest['tracks']:
        tid=src['track_id']; root=corpus/tid
        result={'source_id':tid,'source_sha256':src['sha256'],'source_duration_sec':src['duration_sec'],'artifacts':[],'issues':[]}
        issues=result['issues']
        source_path=Path(src['source_path'])
        result['source_hash_matches']=source_path.is_file() and digest(source_path)==src['sha256']
        if not result['source_hash_matches']:issues.append({'kind':'source_hash_mismatch'})
        def walk(x,where,artifact):
            if isinstance(x,dict):
                if 'track_id' in x and x['track_id']!=tid:
                    issues.append({'kind':'source_id_mismatch','artifact':artifact,'field':where})
                for k,v in x.items():
                    if k=='confidence' and v is not None and isinstance(v,(int,float)) and not 0 <= v <= 1:
                        issues.append({'kind':'invalid_confidence','artifact':artifact,'field':where+'.'+k})
                    if isinstance(v,str) and v.startswith('/home/') and (k.endswith('path') or k in ('archive','semantic','source','output_dir')) and not Path(v).exists():
                        issues.append({'kind':'missing_reference','artifact':artifact,'field':where+'.'+k,'path':v})
                    if isinstance(v,(int,float)) and ('attack_ms' in k) and v < 0:
                        issues.append({'kind':'invalid_negative_attack','artifact':artifact,'field':where+'.'+k})
                    walk(v,where+'.'+k,artifact)
                pairs=[('start_sec','end_sec'),('start_second','end_second'),('onset_sec','offset_sec')]
                for a,b in pairs:
                    if isinstance(x.get(a),(int,float)) and isinstance(x.get(b),(int,float)) and not 0 <= x[a] < x[b] <= src['duration_sec']+.1:
                        issues.append({'kind':'invalid_span','artifact':artifact,'field':where})
                if isinstance(x.get('path'),str) and x.get('sha256'):
                    p=Path(x['path'])
                    if p.is_file() and digest(p)!=x['sha256']:
                        issues.append({'kind':'hash_mismatch','artifact':artifact,'path':str(p)})
            elif isinstance(x,list):
                for i,v in enumerate(x):walk(v,where+f'[{i}]',artifact)
            elif isinstance(x,float) and not math.isfinite(x):
                issues.append({'kind':'nonfinite','artifact':artifact,'field':where})
        for p in sorted(root.rglob('*')):
            if not p.is_file():continue
            rel=str(p.relative_to(corpus)); entry={'path':rel,'size_bytes':p.stat().st_size,'sha256':digest(p)}
            if p.suffix=='.json':
                try:walk(read_json(p),'$',rel);entry['json_parses']=True
                except (ValueError,TypeError) as exc:
                    entry['json_parses']=False;issues.append({'kind':'invalid_json','artifact':rel,'reason':str(exc)})
            elif p.suffix=='.csv':
                rows=read_csv(p);entry['rows']=len(rows)
                last=-math.inf
                for i,row in enumerate(rows):
                    for k,v in row.items():
                        if v and v.lower() in ('nan','inf','-inf','infinity','-infinity'):
                            issues.append({'kind':'nonfinite_csv','artifact':rel,'row':i+2,'field':k})
                    k=next((k for k in ('onset_sec','time_sec','start_sec') if k in row),None)
                    if k:
                        try:
                            t=float(row[k])
                            if t<last or t<0 or t>src['duration_sec']+.1:issues.append({'kind':'invalid_time_order','artifact':rel,'row':i+2})
                            last=t
                            if row.get('offset_sec') and not t<float(row['offset_sec'])<=src['duration_sec']+.1:issues.append({'kind':'invalid_note_span','artifact':rel,'row':i+2})
                            if row.get('pitch_midi') and not 0<=float(row['pitch_midi'])<=127:issues.append({'kind':'invalid_pitch','artifact':rel,'row':i+2})
                            if row.get('confidence') and not 0<=float(row['confidence'])<=1:issues.append({'kind':'invalid_confidence','artifact':rel,'row':i+2})
                        except ValueError:issues.append({'kind':'invalid_csv_number','artifact':rel,'row':i+2})
            result['artifacts'].append(entry)
        bp=read_json(root/'reverse_humanmusic/cover_blueprint.v1.json')
        result.update({'blueprint_sha256':digest(root/'reverse_humanmusic/cover_blueprint.v1.json'),'models':[], 'timing':bp['global'],'lead_vocal':{k:v for k,v in bp['roles'].get('vocal',{}).items() if k not in ('phrases',)},'bass':{k:v for k,v in bp['roles'].get('bass',{}).items() if k not in ('slides',)},'groove':bp['groove'],'harmony_span_count':len(bp['harmony']['spans']),'motif_family_count':len(bp['motifs']),'section_count':len(bp['form']['sections']),'section_topology':[s['label_candidate'] for s in bp['form']['sections']],'orchestration_available':bool(bp.get('orchestration')),'production_available':bool(bp.get('production')),'separator_disagreement':read_json(root/'separation/comparison/cross_model.json'),'provenance_receipts':[a for a in result['artifacts'] if '/receipts/' in a['path'] or '/scripts/' in a['path']]})
        for p in sorted((root/'receipts').glob('separation_*.json')):
            rec=read_json(p);result['models'].append({'model':rec['model'],'status':rec['status'],'stems':[s['stem'] for s in rec['stems']],'receipt_sha256':digest(p)})
        try:
            fatal = [x for x in issues if x['kind'] in ('source_hash_mismatch', 'hash_mismatch', 'source_id_mismatch', 'invalid_json')]
            if fatal:
                raise ValueError('source/artifact integrity failure; see audit issues')
            normalized=import_track(root,src)
            out=output/tid;out.mkdir(exist_ok=True)
            (out/'compiled_source_model.json').write_text(normalized.to_json())
            receipt={'source_id':tid,'source_sha256':src['sha256'],'blueprint_sha256':normalized.data['blueprint_sha256'],'manifest_sha256':report['manifest']['sha256'],'source_path':src['source_path'],'model_sha256':digest(out/'compiled_source_model.json'),'semantic_identity_sha256':normalized.identity_hash()}
            (out/'source_receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
            result['import_status']='ok';result['unknowns']=normalized.data['unknowns'];result['quarantine']=normalized.data['quarantine'];result['role_lanes']=[{k:v for k,v in l.items() if k!='events'} for l in normalized.data['lanes']]
        except (ValueError,KeyError) as exc:result['import_status']='refused';result['refusal']=str(exc)
        result['issue_counts']=dict(Counter(x['kind'] for x in issues))
        report['tracks'].append(result)
        print(tid,result['import_status'],result['issue_counts'],flush=True)
    bundle_path=corpus/'bundles/BUNDLE_MANIFEST.json'
    report['bundle_manifest']={'path':str(bundle_path),'sha256':digest(bundle_path)}
    for name,b in read_json(bundle_path)['bundles'].items():
        p=Path(b['path']);result={'name':name,'path':str(p),'expected_sha256':b['sha256'],'size_bytes':p.stat().st_size,'archive_hash_matches':digest(p)==b['sha256'] if hash_bundles else None,'members':len(b['members']),'member_mismatches':[]}
        for member,r in b['members'].items():
            f=corpus/member
            if not f.is_file() or digest(f)!=r['sha256'] or f.stat().st_size!=r['size_bytes']:result['member_mismatches'].append(member)
        result['archive_member_validation'] = verify_archive_members(p,b['members']) if hash_bundles else None
        report['bundle_checks'].append(result)
    # Root/environment JSON is audited too; archived versions may differ from live reports.
    report['global_artifacts'] = []
    for p in sorted(corpus.rglob('*.json')):
        if p.relative_to(corpus).parts[0].startswith('track-'):
            continue
        entry={'path':str(p.relative_to(corpus)), 'sha256':digest(p), 'size_bytes':p.stat().st_size}
        try:
            finite_tree(read_json(p));entry['json_valid_finite']=True
        except (ValueError,TypeError) as exc:
            entry['json_valid_finite']=False;entry['error']=str(exc)
        report['global_artifacts'].append(entry)
    report['summary']={'tracks':len(report['tracks']),'imports':sum(t['import_status']=='ok' for t in report['tracks']),'source_hash_matches':sum(t['source_hash_matches'] for t in report['tracks']),'artifact_count':sum(len(t['artifacts']) for t in report['tracks']),'issue_counts':dict(sum((Counter(t['issue_counts']) for t in report['tracks']),Counter()))}
    (output/'CORPUS_INVENTORY.json').write_text(json.dumps(report,indent=2,allow_nan=False)+'\n')
    return report


def main():
    p=argparse.ArgumentParser();p.add_argument('--corpus',type=Path,required=True);p.add_argument('--out',type=Path,required=True);p.add_argument('--skip-archive-hashes',action='store_true');a=p.parse_args()
    report=audit(a.corpus,a.out,not a.skip_archive_hashes);print(json.dumps(report['summary'],indent=2))

if __name__=='__main__':main()

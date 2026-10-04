"""Strict wire model for local, derived stem evidence. Unknown is never Free."""
from __future__ import annotations
import hashlib
import json
import math
import re
from dataclasses import dataclass
from typing import Any

SCHEMA = 'sai.stem_evidence/v2'


def canonical_hash(value: Any) -> str:
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(',', ':'), allow_nan=False).encode()).hexdigest()


def finite_tree(value: Any, path: str = '$') -> None:
    if isinstance(value, float) and not math.isfinite(value):
        raise ValueError(f'{path}: non-finite number')
    if isinstance(value, dict):
        for key, child in value.items():
            if key == 'confidence':
                probability(child, f'{path}.{key}')
            if key.endswith('sha256') and child is not None and not re.fullmatch('[0-9a-f]{64}', str(child)):
                raise ValueError(f'{path}.{key}: invalid hash')
            finite_tree(child, f'{path}.{key}')
    elif isinstance(value, list):
        for i, child in enumerate(value):
            finite_tree(child, f'{path}[{i}]')


def probability(value: Any, path: str) -> None:
    if value is not None and (isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value) or not 0 <= value <= 1):
        raise ValueError(f'{path}: confidence outside [0,1]')


@dataclass(frozen=True)
class StemEvidence:
    """Validated JSON envelope; raw alternate routes remain inside lanes and rivals."""
    data: dict[str, Any]

    def __post_init__(self) -> None:
        d = self.data
        finite_tree(d)
        if d.get('schema') != SCHEMA:
            raise ValueError('unsupported evidence schema')
        for key in ('source_sha256', 'blueprint_sha256'):
            if not re.fullmatch('[0-9a-f]{64}', d.get(key, '')):
                raise ValueError(f'invalid {key}')
        if not d.get('source_id') or not d.get('provenance'):
            raise ValueError('source identity and provenance required')
        if d['duration_sec'] <= 0 or d['duration_beats'] <= 0:
            raise ValueError('nonpositive duration')
        if d.get('tempo_bpm') is not None and d['tempo_bpm'] <= 0:
            raise ValueError('invalid tempo')
        for axis, status in d.get('evidence_status', {}).items():
            if status not in ('unknown', 'theme', 'metric', 'ordered', 'quality_family', 'pocket_skeleton', 'topology', 'observed'):
                raise ValueError(f'{axis}: unsupported evidence ceiling {status}')
        for group in ('harmony', 'sections'):
            spans = d.get(group, [])
            if any(b['start_beat'] < a['end_beat'] - 1e-6 for a, b in zip(spans, spans[1:])):
                raise ValueError(f'{group}: overlapping or unordered spans')
        for group, key in (('notes', 'onset_sec'), ('drums', 'onset_sec')):
            events = d.get(group, [])
            if any(b[key] < a[key] for a, b in zip(events, events[1:])):
                raise ValueError(f'{group}: nonmonotonic event time')
        for group in ('notes', 'harmony', 'sections', 'drums'):
            for event in d.get(group, []):
                probability(event.get('confidence'), group)
                if not event.get('provenance'):
                    raise ValueError(f'{group}: missing provenance')
                if group == 'notes':
                    if not 0 <= event['midi'] <= 127 or event['onset_sec'] < 0 or event['offset_sec'] <= event['onset_sec'] or event['offset_sec'] > d['duration_sec'] + .1 or event['duration_beats'] <= 0 or event['onset_beat'] < 0 or event['onset_beat'] + event['duration_beats'] > d['duration_beats'] + .1:
                        raise ValueError('invalid note span/pitch')
                elif group in ('harmony', 'sections'):
                    if group == 'harmony' and (not isinstance(event['root'], int) or not 0 <= event['root'] < 12):
                        raise ValueError('invalid harmony root')
                    if event['start_beat'] < 0 or event['end_beat'] <= event['start_beat'] or event['end_beat'] > d['duration_beats'] + .1:
                        raise ValueError(f'invalid {group} span')
                elif event['onset_sec'] < 0 or event['onset_sec'] > d['duration_sec'] + .1:
                    raise ValueError('invalid drum onset')

    def to_json(self) -> str:
        return json.dumps(self.data, indent=2, sort_keys=True, allow_nan=False) + '\n'

    @classmethod
    def from_json(cls, text: str) -> 'StemEvidence':
        return cls(json.loads(text))

    def identity_hash(self) -> str:
        """Production coordinates and archive locations cannot alter musical identity."""
        keys = ('tempo_bpm', 'meter_beats', 'key_root', 'duration_beats', 'notes', 'harmony', 'drums', 'sections', 'evidence_status')
        def semantic(v):
            if isinstance(v, dict):
                return {k: semantic(x) for k, x in v.items() if k not in ('provenance', 'source_path')}
            if isinstance(v, list):
                return [semantic(x) for x in v]
            return v
        return canonical_hash({k: semantic(self.data.get(k)) for k in keys})

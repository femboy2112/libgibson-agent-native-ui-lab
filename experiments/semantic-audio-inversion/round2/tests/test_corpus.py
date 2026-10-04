"""Synthetic-only schema, provenance, role and transport controls."""
import copy
import json
import math
import unittest
import tempfile
from pathlib import Path
from sai_v2.corpus import BeatTransport, role_hypotheses, import_track
from sai_v2.model import StemEvidence, SCHEMA


def fixture():
    p={'artifact':'synthetic.csv','artifact_sha256':'a'*64,'level':'DerivedAnalysis','model':'synthetic','stem_label':'vocals'}
    return {'schema':SCHEMA,'source_id':'synthetic','source_sha256':'b'*64,'blueprint_sha256':'c'*64,'duration_sec':4.,'duration_beats':8.,'tempo_bpm':120.,'meter_beats':4,'key_root':0,'provenance':p,'notes':[{'role':'lead','onset_sec':0.,'offset_sec':.4,'onset_beat':0.,'duration_beats':.8,'midi':60.,'confidence':.7,'provenance':p}],'harmony':[{'start_beat':0.,'end_beat':8.,'root':0,'quality':'maj','confidence':.6,'rivals':[{'root_pc':9,'quality':'min'}],'provenance':p}],'sections':[],'drums':[],'evidence_status':{'motif':'metric','harmony':'ordered','form':'unknown'},'production':{'gain':.5},'unknowns':['form'],'lanes':[{'role_hypotheses':[{'role':'lead'},{'role':'riff'}],'stem_identity':p}]}


class SchemaTests(unittest.TestCase):
    def test_provenance_and_rivals_roundtrip(self):
        d=fixture();self.assertEqual(StemEvidence.from_json(StemEvidence(d).to_json()).data,d)
    def test_nonfinite_rejected_anywhere(self):
        for bad in (math.nan,math.inf,-math.inf):
            d=fixture();d['production']['gain']=bad
            with self.assertRaises(ValueError):StemEvidence(d)
    def test_hash_fail_closed(self):
        d=fixture();d['source_sha256']='not a hash'
        with self.assertRaises(ValueError):StemEvidence(d)
    def test_invalid_confidence_and_spans_fail_closed(self):
        for field,value in [('confidence',1.01),('onset_sec',-1),('offset_sec',0),('midi',128),('duration_beats',0)]:
            d=fixture();d['notes'][0][field]=value
            with self.assertRaises(ValueError,msg=field):StemEvidence(d)
    def test_absent_evidence_is_unknown_not_free(self):
        d=fixture();self.assertEqual(StemEvidence(d).data['evidence_status']['form'],'unknown')
        d['evidence_status']['form']='free'
        with self.assertRaises(ValueError):StemEvidence(d)
    def test_production_does_not_change_identity(self):
        a=fixture();b=copy.deepcopy(a);b['production']={'gain':100,'width':.2}
        self.assertEqual(StemEvidence(a).identity_hash(),StemEvidence(b).identity_hash())
    def test_identity_deterministic_and_sensitive(self):
        a=StemEvidence(fixture());b=StemEvidence.from_json(a.to_json())
        self.assertEqual(a.identity_hash(),b.identity_hash())
        b.data['notes'][0]['midi']+=1
        self.assertNotEqual(a.identity_hash(),b.identity_hash())
    def test_provenance_required(self):
        d=fixture();d['notes'][0]['provenance']={}
        with self.assertRaises(ValueError):StemEvidence(d)


class LaneTests(unittest.TestCase):
    def test_vocal_label_does_not_force_lead(self):
        poly=[{'onset_sec':i*.1,'offset_sec':i*.1+1,'midi':60} for i in range(8)]
        self.assertEqual(role_hypotheses('vocals',poly)[0]['role'],'support')
    def test_melodic_guitar_is_possible_lead(self):
        mono=[{'onset_sec':i,'offset_sec':i+.4,'midi':70+i} for i in range(8)]
        self.assertEqual(role_hypotheses('guitar',mono)[0]['role'],'lead')
        self.assertEqual(len(role_hypotheses('guitar',mono)),2)
    def test_bass_label_needs_low_register(self):
        high=[{'onset_sec':i,'offset_sec':i+.4,'midi':80} for i in range(8)]
        self.assertNotEqual(role_hypotheses('bass',high)[0]['role'],'bass')
    def test_unknown_lane_stays_unknown(self):
        self.assertEqual(role_hypotheses('vocals',[])[0]['role'],'unknown')
    def test_transport_preserves_origin_and_tempo_scale(self):
        a=BeatTransport([.5,1.,1.5,2.],120);b=BeatTransport([1.,2.,3.,4.],60)
        self.assertEqual(a(0),0)
        self.assertEqual(a(1.25),b(2.5))
        self.assertGreater(a(2.)-a(1.5),0)
    def test_transport_refuses_nonmonotonic(self):
        with self.assertRaises(ValueError):BeatTransport([0,.5,.5],120)

class ImportTests(unittest.TestCase):
    def make_corpus(self, root):
        for directory in ('reverse_humanmusic','events','harmony','separation/comparison'):
            (root/directory).mkdir(parents=True)
        source={'track_id':'synthetic','sha256':'b'*64,'source_path':str(root/'synthetic.wav')}
        bp={'source_identity':source,'global':{'tempo_bpm':120,'duration_sec':2,'meter':{'bpb':4,'phase':0}},'form':{'sections':[{'start_second':0,'end_second':2,'label_candidate':'A','confidence':.5}]},'groove':{'model':'synthetic'},'production':{'full_mix':{'attack_ms_median':-10}}}
        def write(name,x): (root/name).write_text(json.dumps(x))
        write('reverse_humanmusic/cover_blueprint.v1.json',bp)
        write('reverse_humanmusic/stem_observatory.v1.json',{})
        write('harmony/spans.json',{'spans':[{'start_sec':0,'end_sec':2,'start_beat':0,'end_beat':3,'root_pc':0,'quality':'maj','confidence':.6,'rivals':[{'root_pc':9,'quality':'min','score':.2}]}]})
        write('separation/comparison/cross_model.json',{'available':{},'pairs':{}})
        (root/'events/beats.csv').write_text('time_sec,is_downbeat\n0,1\n.5,0\n1,0\n1.5,0\n2,1\n')
        (root/'events/drums.csv').write_text('time_sec,beat,family,confidence,microtiming_beats\n')
        (root/'events/vocal_notes.csv').write_text('onset_sec,offset_sec,pitch_midi,confidence,route,yin_agree\n0,.2,60.2,.8,pyin,1\n.5,.7,62.1,.8,pyin,1\n1,1.2,64.1,.8,pyin,1\n1.5,1.7,67.0,.8,pyin,1\n')
        return source

    def test_import_quarantines_invalid_and_retains_rivals(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);src=self.make_corpus(root);d=import_track(root,src).data
            self.assertIsNone(d['production']['full_mix']['attack_ms_median'])
            self.assertEqual(len(d['quarantine']),1)
            self.assertEqual(d['harmony'][0]['rivals'][0]['root_pc'],9)
            self.assertEqual(d['notes'][0]['midi'],60.2)

    def test_missing_bass_and_drums_never_become_pins(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);src=self.make_corpus(root);d=import_track(root,src).data
            self.assertEqual(d['evidence_status']['bass'],'unknown')
            self.assertEqual(d['evidence_status']['groove'],'unknown')
            self.assertEqual(d['evidence_status']['motif'],'metric')

    def test_inclusive_raw_harmony_indices_are_not_durations(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);src=self.make_corpus(root);d=import_track(root,src).data
            self.assertEqual(d['harmony'][0]['raw_metric_indices'],[0,3])
            self.assertEqual(d['harmony'][0]['end_beat'],4)
            src['sha256']='d'*64
            with self.assertRaises(ValueError):import_track(root,src)

if __name__=='__main__':unittest.main()

"""Bounded frame-length discriminator; source crop selection by declared count strata."""
import argparse,hashlib,json,sys,time
from pathlib import Path
import numpy as np
import librosa
import soundfile as sf
parser=argparse.ArgumentParser();parser.add_argument('--corpus',type=Path,required=True);parser.add_argument('--out',type=Path,required=True);args=parser.parse_args()
root=args.corpus;out=args.out;out.mkdir(parents=True,exist_ok=True)
sys.path.insert(0,str(root/'scripts'));import mir_lib as M
rows=[{'source_id':p.parent.parent.parent.name,'note_count':json.load(open(p))['n_notes']} for p in sorted(root.glob('track-*/features/stems/bass_summary.json'))]
selected=[next(x['source_id'] for x in rows if x['note_count']>0),next(x['source_id'] for x in rows if x['note_count']==0)]
sr=44100;t=np.arange(6*sr)/sr;pitch=np.repeat([36.,40.,43.],2*sr);hz=440*2**((pitch-69)/12);phase=np.cumsum(2*np.pi*hz/sr);y=(.5*np.sin(phase)+.15*np.sin(2*phase)+.05*np.sin(3*phase)).astype(np.float32)
inputs=[('synthetic_C2_E2_G2',y,sr,0.,pitch)]
pitch_low=pitch-12;hz_low=440*2**((pitch_low-69)/12);phase_low=np.cumsum(2*np.pi*hz_low/sr);y_low=(.5*np.sin(phase_low)+.15*np.sin(2*phase_low)+.05*np.sin(3*phase_low)).astype(np.float32)
inputs.insert(0,('synthetic_C1_E1_G1',y_low,sr,0.,pitch_low))
for tid in selected:
 p=root/tid/'separation/htdemucs_ft/bass.wav'
 with sf.SoundFile(p) as f:
  f.seek(30*f.samplerate);y=f.read(10*f.samplerate,dtype='float32',always_2d=True).mean(axis=1);sr=f.samplerate
 inputs.append((tid,y,sr,30.,None))
report={'schema':'sai.bass_window_probe/v1','controlled_factor':'frame_length=2048 versus4096; same sample rate, crop, hop256, fminC1/fmaxG4, pYIN and segmentation threshold0.3','selection':'first source with >0 bass notes and first source with zero bass notes; crop30-40s; synthetic6s known bass control','boundary':'More detections on real audio do not establish pitch accuracy. Source extraction unchanged; no revised source model or pins.','results':[], 'librosa_version':librosa.__version__, 'source_manifest_sha256':hashlib.sha256((root/'MANIFEST.json').read_bytes()).hexdigest(), 'segmentation_script_sha256':hashlib.sha256(Path(M.__file__).read_bytes()).hexdigest()}
for tid,y,sr,start,truth in inputs:
 for frame in [2048,4096]:
  tic=time.monotonic();f0,voiced,prob=librosa.pyin(y,sr=sr,fmin=librosa.note_to_hz('C1'),fmax=librosa.note_to_hz('G4'),frame_length=frame,hop_length=256,fill_na=np.nan)
  times=librosa.frames_to_time(np.arange(len(f0)),sr=sr,hop_length=256);midi=librosa.hz_to_midi(f0);notes=M._seg_midi(times,midi,prob,min_dur=.07,pitch_tol=.7)
  rec={'source_id':tid,'crop_start_sec':start,'crop_duration_sec':len(y)/sr,'crop_pcm_sha256':hashlib.sha256(y.tobytes()).hexdigest(),'sample_rate':sr,'frame_length':frame,'notes':len(notes),'voiced_fraction':float(voiced.mean()),'prob_gt_0p3_fraction':float((prob>.3).mean()),'median_voiced_prob':float(np.median(prob)),'seconds':time.monotonic()-tic}
  if truth is not None:
   idx=np.minimum((times*sr).astype(int),len(truth)-1);correct=np.abs(midi-truth[idx])<=.75;rec['frame_pitch_recall']=float(np.mean(correct));rec['voiced_pitch_precision']=float(correct[voiced].mean()) if voiced.any() else 0
  report['results'].append(rec);print(rec,flush=True);(out/'BASS_WINDOW_PROBE.json').write_text(json.dumps(report,indent=2)+'\n')

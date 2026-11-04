#!/usr/bin/env python3
"""Frozen-engine evaluation. Python orchestrates and scores; all signal/inference work is Rust."""
import argparse,csv,datetime,hashlib,json,math,pathlib,subprocess,time,statistics
from prepare_independent_validation import closest,rows,ROOT,EAST,NORTH
OUT=ROOT/'validation-runs/independent';DATA=ROOT/'data/independent-validation'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def invoke(cmd,path):
 start=time.monotonic();p=subprocess.run([str(x) for x in cmd]+['--output',str(path)],capture_output=True,text=True,timeout=600)
 return {'exit_code':p.returncode,'elapsed_s':round(time.monotonic()-start,3),'result':str(path.relative_to(ROOT)),'result_sha256':sha(path) if path.exists() else None,'stderr':p.stderr.strip()}
def main():
 ap=argparse.ArgumentParser();ap.add_argument('--baseline-binary',type=pathlib.Path,default=ROOT/'target/baseline/release/echo-locate');ap.add_argument('--revised-binary',type=pathlib.Path,default=ROOT/'target/release/echo-locate');args=ap.parse_args();OUT.mkdir(parents=True,exist_ok=True)
 freeze=json.loads((DATA/'freeze.json').read_text())
 def verify():
  for path,digest in freeze['frozen_files'].items():assert sha(ROOT/path)==digest,'Frozen source/protocol changed: '+path
 verify();protocol=json.loads((DATA/'pathak-protocol.json').read_text());ref=protocol['reference'];records=[]
 for engine,binary in [('baseline',args.baseline_binary),('revised',args.revised_binary)]:
  for case in protocol['cases']:
   id=case['id'];audio=ROOT/case['audio'];config=DATA/(id+'.'+engine+'.config.json');pack=DATA/(id+'.package.json');path=OUT/(id+'.'+engine+'.json')
   record={'id':id,'engine':engine,'audio_sha256':sha(audio),'config_sha256':sha(config),'package_sha256':sha(pack),'binary_sha256':sha(binary),**invoke([binary,'analyze',audio,'--config',config,'--package',pack],path)}
   if record['exit_code']==0:
    a=json.loads(path.read_text())['analysis'];record.update(status=a['status'],events=len(a['events']),candidate_count=len(a['candidates']),search=a['search'])
    if a['candidates']:
     c=a['candidates'][0];x,y=c['lon']*EAST,c['lat']*NORTH;track=rows(ROOT/'corpus/pathak-heldout'/id);t=datetime.datetime.fromisoformat(json.loads((ROOT/'corpus/pathak-heldout/manifest.json').read_text())['cases'][next(i for i,c in enumerate(protocol['cases']) if c['id']==id)]['first_ridge_utc'].replace('Z','+00:00')).timestamp();track=[r for r in track if t-90<float(r['time_unix_s'])<t+180]
     offset=closest(x,y,track)[0];record.update(estimate_xy_m=[x,y],distance_to_metadata_reference_m=math.hypot(x-ref['reference_xy_m'][0],y-ref['reference_xy_m'][1]),ground_track_offset_error_m=abs(offset-case['source_offset_m']),expected_flight_matched=any(m['flight_id'].startswith(id) for m in c['matches']))
   records.append(record);print(engine,id,record.get('status','error'),record.get('distance_to_metadata_reference_m'),flush=True)
  paths=[OUT/(c['id']+'.'+engine+'.json') for c in protocol['cases']];fused=OUT/('site.'+engine+'.json');rr=invoke([args.revised_binary,'fuse-site',*paths],fused);rr.update(engine=engine,kind='shared_site_all_five')
  if rr['exit_code']==0:
   a=json.loads(fused.read_text())['fusion'];rr['status']=a['status'];rr['cells_with_all_recordings']=len(a['heatmap'])
   if a['candidates']:
    c=a['candidates'][0];x,y=c['lon']*EAST,c['lat']*NORTH;rr.update(estimate_xy_m=[x,y],distance_to_metadata_reference_m=math.hypot(x-ref['reference_xy_m'][0],y-ref['reference_xy_m'][1]),recordings=c['recordings'],independent_clocks=c['independent_clocks'])
  records.append(rr);print('fusion',engine,rr.get('status'),rr.get('distance_to_metadata_reference_m'),flush=True)
 # The four untouched AeroSonicDB signals receive the same frozen spectral/model settings.
 aero=json.loads((ROOT/'corpus/ground-truth/aerosonic/selected-recordings.json').read_text())
 for engine,binary in [('baseline',args.baseline_binary),('revised',args.revised_binary)]:
  config=DATA/('aerosonic.'+engine+'.config.json')
  for case in aero:
   path=OUT/(case['id']+'.'+engine+'.json');audio=ROOT/'corpus/ground-truth/aerosonic'/case['audio_path'];rr={'engine':engine,'id':case['id'],'kind':'aerosonic_discovery_only','audio_sha256':sha(audio),'config_sha256':sha(config),**invoke([binary,'analyze',audio,'--config',config],path)}
   if rr['exit_code']==0:
    a=json.loads(path.read_text())['analysis'];rr.update(status=a['status'],events=len(a['events']),candidate_count=len(a['candidates']),location_error_eligible=False)
   records.append(rr);print('AeroSonic',engine,case['id'],rr.get('events'),flush=True)
 verify();report={'schema_version':1,'frozen_engine':freeze,'validation_scope':'Separate real recordings, all selected before evaluation. Pathak site reference is reconstructed from metadata, not surveyed GPS. Source-track associations and a finite town prior are supplied. One-pass locations retain along-track/clock and mirror ambiguities. Fusion uses every one of the five cases, separate clocks, equal weights, and no truth-dependent basin choice.','origin_prior_baseline_distance_m':math.hypot(*ref['reference_xy_m']),'records':records,'algorithm_or_threshold_tuned_after_evaluation':False}
 (ROOT/'docs/independent-validation.json').write_text(json.dumps(report,indent=2)+'\n')
 if any(r['exit_code'] for r in records):raise SystemExit('At least one command failed; see retained results')
if __name__=='__main__':main()

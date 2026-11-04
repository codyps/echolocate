#!/usr/bin/env python3
"""Evaluate actual Rust estimates against separately held receiver coordinates.

This evaluates real audio in a documented local frame; it is not a geographic site claim.
"""
import argparse,hashlib,json,math,pathlib,statistics,subprocess,time
ROOT=pathlib.Path(__file__).resolve().parents[1]
def main():
 p=argparse.ArgumentParser();p.add_argument('--output',type=pathlib.Path,default=ROOT/'validation-runs/locations/dlr');a=p.parse_args();a.output.mkdir(parents=True,exist_ok=True)
 protocol=json.loads((ROOT/'data/location-validation/dlr/protocol.json').read_text());records=[]
 for case in protocol['cases']:
  output=a.output/(case['id']+'.json');start=time.monotonic()
  proc=subprocess.run([str(ROOT/'target/release/echo-locate'),'analyze',str(ROOT/case['audio']),'--package',str(ROOT/case['package']),'--config',str(ROOT/case['config']),'--output',str(output)],capture_output=True,text=True,timeout=180)
  record={'id':case['id'],'protocol':case['protocol'],'source_channel':case['source_channel'],'elapsed_s':round(time.monotonic()-start,3),'exit_code':proc.returncode,'audio_sha256':hashlib.sha256((ROOT/case['audio']).read_bytes()).hexdigest(),'package_sha256':hashlib.sha256((ROOT/case['package']).read_bytes()).hexdigest(),'config_sha256':hashlib.sha256((ROOT/case['config']).read_bytes()).hexdigest()}
  if proc.returncode:record['error']=proc.stderr.strip()
  else:
   result=json.loads(output.read_text())['analysis'];record.update(status=result['status'],events=len(result['events']),candidate_count=len(result['candidates']),search=result['search'])
   if result['candidates']:
    # Receiver truth is read only AFTER the inference subprocess has exited.
    truth=json.loads((ROOT/case['truth_file']).read_text());t=next(t for t in truth['cases'] if t['channel_zero_based']==case['source_channel']);x,y,z=t['xyz_m'];embed=protocol['arbitrary_embedding']
    ranked=[]
    for c in result['candidates']:
     ex=c['lon']*embed['east_m_per_degree'];ey=c['lat']*embed['north_m_per_degree'];ranked.append({'estimated_xy_m':[ex,ey],'horizontal_error_m':math.hypot(ex-x,ey-y),'score':c['score'],'start_internal_s':c['start_unix_s'],'matched_events':c['matched_events']})
    record.update(truth_xy_m=[x,y],origin_baseline_error_m=math.hypot(x,y),top1_error_m=ranked[0]['horizontal_error_m'],best_of_reported_candidates_error_m=min(r['horizontal_error_m'] for r in ranked),candidates=ranked)
  records.append(record);print(case['id'],record.get('status','ERROR'),record.get('top1_error_m'))
 groups={}
 for name in sorted({r['protocol'] for r in records}):
  group=[r for r in records if r['protocol']==name];answered=[r for r in group if 'top1_error_m' in r]
  groups[name]={'cases':len(group),'answers':len(answered),'median_error_m_on_answered':statistics.median(r['top1_error_m'] for r in answered) if answered else None,'median_origin_baseline_m_on_answered':statistics.median(r['origin_baseline_error_m'] for r in answered) if answered else None,'beats_origin_baseline':sum(r['top1_error_m']<r['origin_baseline_error_m'] for r in answered)}
 report={'schema_version':1,'engine_binary_sha256':hashlib.sha256((ROOT/'target/release/echo-locate').read_bytes()).hexdigest(),'kind':'real measured receiver positions in source local Cartesian frame','absolute_geographic_accuracy_measured':False,'independent_sessions':1,'interpretation':'All six channels share one jet flyover. Source clock convention is explicitly assumed, not independently established. One protocol uses only supplied knots and a fixed common arrival slice; the other extrapolates a constant-velocity source model. Receiver coordinates were excluded from inference.','groups':groups,'cases':records}
 (a.output/'summary.json').write_text(json.dumps(report,indent=2)+'\n');(ROOT/'docs/local-location-validation.json').write_text(json.dumps(report,indent=2)+'\n')
 lines=['# Actual location-estimate validation: DLR RECORD139','','These are actual engine estimates from measured audio, compared after inference with source receiver coordinates. No geographic origin or absolute UTC is asserted.','','| Protocol | Channel | Status | Top-1 error | Origin-only baseline |','|---|---:|---|---:|---:|']
 for r in records:lines.append(f"| {r['protocol']} | {r['source_channel']} | {r.get('status','error')} | {r['top1_error_m']:.1f} m | {r['origin_baseline_error_m']:.1f} m |" if 'top1_error_m' in r else f"| {r['protocol']} | {r['source_channel']} | {r.get('status','error')} | No estimate | — |")
 lines+=['','## Interpretation','',report['interpretation'],'','The origin-only baseline is included because the array is compact. A small absolute error without beating this baseline would not demonstrate useful acoustic localization. A returned uncalibrated candidate is not a validated answer.','', 'Raw outputs, per-candidate errors, source fingerprints, data conventions and search budgets are retained. Missing estimates remain failures/abstentions, not zero error.']
 (ROOT/'docs/LOCAL_LOCATION_VALIDATION.md').write_text('\n'.join(lines)+'\n')
 if any(r['exit_code'] for r in records):raise SystemExit(1)
if __name__=='__main__':main()

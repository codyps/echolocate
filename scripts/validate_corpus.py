#!/usr/bin/env python3
"""Run real files through the Rust engine; orchestration only, no Python DSP.

Does not supply source locations to the algorithm or score unverified coordinates.
"""
import argparse, datetime, hashlib, json, pathlib, subprocess, time

ROOT=pathlib.Path(__file__).resolve().parents[1]
def main():
    parser=argparse.ArgumentParser();parser.add_argument('--binary',type=pathlib.Path,default=ROOT/'target/release/echo-locate');parser.add_argument('--output',type=pathlib.Path,default=ROOT/'validation-runs/corpus');args=parser.parse_args()
    args.output.mkdir(parents=True,exist_ok=True)
    manifest=json.loads((ROOT/'corpus/manifest.json').read_text())
    cases=[]
    for r in manifest['recordings']:
        cases.append({'id':r['id'],'path':ROOT/'corpus'/r['audio']['path'],'sha256':r['audio']['sha256'],'source_url':r['source_url'],'class':'news_or_public_affairs','accuracy_eligible':False})
    pathak=ROOT/'corpus/pathak'
    for directory in sorted(pathak.iterdir()):
        if directory.is_dir() and (directory/'audio.wav').exists():
            p=directory/'audio.wav';cases.append({'id':directory.name,'path':p,'sha256':hashlib.sha256(p.read_bytes()).hexdigest(),'source_url':'https://zenodo.org/records/23424278','class':'independent_aircraft_dataset','accuracy_eligible':False})
    records=[]
    for case in cases:
        p=case['path'];digest=hashlib.sha256(p.read_bytes()).hexdigest()
        if digest!=case['sha256']:raise RuntimeError(f"Hash mismatch: {case['id']}")
        output=args.output/f"{case['id']}.json";start=time.monotonic()
        proc=subprocess.run([str(args.binary),'analyze',str(p),'--output',str(output)],capture_output=True,text=True,timeout=180)
        record={k:v for k,v in case.items() if k!='path'};record.update(audio_path=str(p.relative_to(ROOT)),seconds=round(time.monotonic()-start,3),exit_code=proc.returncode,stderr=proc.stderr.strip())
        if proc.returncode==0:
            a=json.loads(output.read_text())['analysis'];record.update(status=a['status'],events=len(a['events']),duration_s=a['audio']['analyzed_duration_s'],candidates=len(a['candidates']),events_summary=[{'start_s':e['start_s'],'end_s':e['end_s'],'drop_hz':round(e['drop_hz'],2),'harmonics':e['harmonics']} for e in a['events']])
            # Without a package the engine must abstain, regardless of apparent ridge quality.
            assert a['status'] in ('needs_flight_data','no_usable_events') and not a['candidates']
        records.append(record);print(f"{case['id']}: {record.get('status','ERROR')} / {record.get('events',0)} events / {record['seconds']}s")
    report={'generated_at_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'engine_sha256':hashlib.sha256(args.binary.read_bytes()).hexdigest(),'method':'unattended Rust audio discovery + required no-package abstention','location_accuracy_measured':False,'limitations':['These reports do not have independently verified microphone coordinates and waveform start timestamps.','Detected events are proposals, not confirmed aircraft.','No archived flight packages were supplied; location matching was intentionally not attempted.'],'cases':records}
    (args.output/'summary.json').write_text(json.dumps(report,indent=2)+'\n')
    lines=['# Real-audio validation receipt','',report['method'],'','This is a discovery and abstention check. It is not location-accuracy validation.','', '| Case | Kind | Seconds of audio | Events proposed | Status | Runtime |','|---|---|---:|---:|---|---:|']
    for r in records:lines.append(f"| {r['id']} | {r['class']} | {r.get('duration_s',0):.1f} | {r.get('events',0)} | {r.get('status','ERROR')} | {r['seconds']:.2f}s |")
    lines+=['','## Limits','']+['- '+v for v in report['limitations']]
    (args.output/'README.md').write_text('\n'.join(lines)+'\n')
    if any(r['exit_code'] for r in records):raise SystemExit(1)
if __name__=='__main__':main()

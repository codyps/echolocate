#!/usr/bin/env python3
"""Check all acquired traces against their recorded bytes and parsed point counts."""
import gzip,hashlib,json,pathlib
ROOT=pathlib.Path(__file__).resolve().parent

def main():
 count=0;days=[]
 manifest=json.loads((ROOT/'manifest.json').read_text())
 cases=manifest['days']+[{'date':'2025-11-09','acquired_traces':manifest['af1']['acquired_traces']},{'date':manifest['original_creator_reference']['date'],'acquired_traces':manifest['original_creator_reference']['acquired_competitive_traces']}]
 for day in cases:
  date=day['date'];receipt=ROOT/date/'trace-receipt.json'
  if date=='2025-11-09':receipt=ROOT/date/'archive-trace-receipt.json'
  records=json.loads(receipt.read_text());valid=[r for r in records if r.get('ok',True)]
  assert len(valid)==day['acquired_traces'],date
  for r in valid:
   path=ROOT/r.get('path',f"{date}/traces/trace_full_{r['icao']}.json")
   raw=path.read_bytes();decoded=gzip.decompress(raw) if raw[:2]==b'\x1f\x8b' else raw
   expected=r.get('source_sha256',r.get('sha256'))
   assert hashlib.sha256(raw).hexdigest()==expected,path
   if r.get('decoded_sha256'):
    assert hashlib.sha256(decoded).hexdigest()==r['decoded_sha256'],path
   data=json.loads(decoded);assert len(data['trace'])==r['points'],path
   count+=1
  days.append({'date':date,'verified_traces':len(valid)})
 assert count==491
 print(json.dumps({'verified_traces':count,'days':days},indent=2))
if __name__=='__main__':main()

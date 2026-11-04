#!/usr/bin/env python3
"""Restore exact source archive members from checked byte-range receipts.
Avoids downloading complete multi-gigabyte daily archives. Public GitHub URLs only.
"""
import argparse,hashlib,json,pathlib,urllib.request
ROOT=pathlib.Path(__file__).resolve().parent

def main():
 p=argparse.ArgumentParser();p.add_argument('--restore',action='store_true');p.add_argument('--verify',action='store_true');p.add_argument('--date',action='append');args=p.parse_args()
 records=json.loads((ROOT/'range-downloads.json').read_text());selected=[r for r in records if not args.date or any(r['path'].startswith(day+'/') for day in args.date)]
 for r in selected:
  path=ROOT/r['path'];expected=r['sha256']
  if args.restore and (not path.exists() or hashlib.sha256(path.read_bytes()).hexdigest()!=expected):
   start,end=r['range'];req=urllib.request.Request(r['url'],headers={'Range':f'bytes={start}-{end}','User-Agent':'Echolocate source recovery/0.1'})
   with urllib.request.urlopen(req,timeout=60) as response:
    assert response.status==206,'Server did not honor Range'
    assert response.headers.get('Content-Range','').split('/')[0]==f'bytes {start}-{end}','Unexpected range'
    data=response.read(end-start+2)
   assert len(data)==end-start+1,'Unexpected member length';assert hashlib.sha256(data).hexdigest()==expected,'Checksum mismatch';path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(data)
  if not path.exists():raise RuntimeError('Missing '+r['path']+'; pass --restore')
  assert hashlib.sha256(path.read_bytes()).hexdigest()==expected,'Changed '+r['path']
 print(f'Verified {len(selected)} original archive members by SHA-256.')
if __name__=='__main__':main()

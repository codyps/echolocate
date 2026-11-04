#!/usr/bin/env python3
"""Acquire small public replay windows, then complete per-aircraft historical traces.
Replay points are only for ICAO discovery: their altitude is barometric.
Full source traces retain geometric altitude fields for the independent Rust importer.
No numerical audio processing or Doppler inference is performed here.
"""
import argparse,concurrent.futures,datetime,gzip,hashlib,json,pathlib,struct,time,urllib.request,urllib.error,threading
ROOT=pathlib.Path(__file__).resolve().parent
SETTINGS={
 '2025-09-12':{'recording':'fox-thunderbirds','chunks':[24,25],'bounds':[-77.3,38.7,-76.7,39.15]},
 '2024-08-21':{'recording':'fox-frecce','chunks':[40,41,42],'bounds':[-77.3,38.7,-76.7,39.15]},
 '2025-11-09':{'recording':'fox-af1','chunks':[43],'bounds':[-77.1,38.7,-76.65,39.1]},
 '2025-02-26':{'recording':'dvids-yuma','chunks':[34,38,42],'bounds':[-114.85,32.4,-114.35,32.9]},
}
LOCK=threading.Lock();WIRE_BYTES=0;LIMIT=300*1024*1024

def fetch(url,path,attempts=3):
 global WIRE_BYTES
 if path.exists():return path.read_bytes(),{'cached':True,'url':url,'bytes':path.stat().st_size}
 errors=[]
 for attempt in range(attempts):
  try:
   req=urllib.request.Request(url,headers={'User-Agent':'Echolocate historical research/0.1'})
   with urllib.request.urlopen(req,timeout=35) as r:
    output=bytearray()
    while block:=r.read(1024*1024):
     with LOCK:
      WIRE_BYTES+=len(block)
      if WIRE_BYTES>LIMIT:raise RuntimeError('Research wire-byte budget exceeded')
     output.extend(block)
     if len(output)>48*1024*1024:raise RuntimeError('Individual asset exceeds48MiB limit')
    path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(output)
    return bytes(output),{'url':url,'resolved_url':r.geturl(),'bytes':len(output),'sha256':hashlib.sha256(output).hexdigest(),'retrieved_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'http_status':r.status,'cached':False}
  except Exception as e:
   errors.append(str(e));time.sleep(.5*(attempt+1))
 raise RuntimeError('; '.join(errors))

def replay(path,bounds):
 wire=path.read_bytes();raw=gzip.decompress(wire) if wire[:2]==b'\x1f\x8b' else wire
 if len(raw)%16:raise ValueError('Replay byte length not multiple of16')
 t=None;candidates={};callsigns={};times=[]
 for a,la,lo,packed in struct.iter_unpack('<4I',raw):
  if a==0x0e7f7c9d:t=(la*(1<<32)+lo)/1000;times.append(t);continue
  if la>=1073741824:
   if la>=2147483648:continue
   name=struct.pack('<2I',lo,packed).split(b'\x00')[0].decode('ascii',errors='replace').strip()
   if name:callsigns[a&0xffffff]=name
   continue
  lat=la/1e6;lon=(lo if lo<1<<31 else lo-(1<<32))/1e6
  if t is None or not bounds[0]<=lon<=bounds[2] or not bounds[1]<=lat<=bounds[3]:continue
  non_icao=bool(a&0x1000000);hexid=('~' if non_icao else '')+f'{a&0xffffff:06x}'
  alt=packed&65535
  if alt>=32768:alt-=65536
  alt=None if alt in [-123,-124] else alt*25
  p=candidates.setdefault(hexid,{'id':hexid,'non_icao':non_icao,'points':[]})
  p['points'].append({'time_unix_s':t,'lat':lat,'lon':lon,'alt_baro_ft':alt,'source_code':(a>>27)&31})
 if not times:raise ValueError('Replay lacks timestamp markers')
 for p in candidates.values():p['callsign']=callsigns.get(int(p['id'].lstrip('~'),16),'')
 return {'source_path':str(path.relative_to(ROOT)),'source_sha256':hashlib.sha256(wire).hexdigest(),'altitude_datum':'barometric feet; not geometric height','start_unix_s':times[0],'end_unix_s':times[-1],'markers':len(times),'candidate_count':len(candidates),'candidates':list(candidates.values())}

def chunk_task(day,index,cfg):
 path=ROOT/day/'replay'/f'{index:02d}.bin.ttf';url=f"https://globe.adsb.lol/globe_history/{day.replace('-','/')}/heatmap/{index:02d}.bin.ttf"
 try:
  _,receipt=fetch(url,path);data=replay(path,cfg['bounds']);data['acquisition']=receipt
  target=ROOT/day/'discovery'/f'{index:02d}.json';target.parent.mkdir(parents=True,exist_ok=True);target.write_text(json.dumps(data,indent=2)+'\n')
  print(day,index,'replay',data['candidate_count'],'regional candidates',flush=True);return {'day':day,'chunk':index,'ok':True,'receipt':receipt}
 except Exception as e:print(day,index,'FAILED',e,flush=True);return {'day':day,'chunk':index,'ok':False,'error':str(e),'url':url}

def trace_task(day,hexid):
 rawid=hexid.lstrip('~');url=f"https://globe.adsb.lol/globe_history/{day.replace('-','/')}/traces/{rawid[-2:]}/trace_full_{hexid}.json"
 path=ROOT/day/'traces'/f'trace_full_{hexid}.json'
 try:
  wire,receipt=fetch(url,path);decoded=gzip.decompress(wire) if wire[:2]==b'\x1f\x8b' else wire;v=json.loads(decoded)
  rows=v.get('trace',[])
  if not isinstance(rows,list) or len(rows)<2:raise ValueError('No multi-point trace')
  geom=sum(1 for r in rows if isinstance(r,list) and ((len(r)>10 and isinstance(r[10],(int,float))) or (len(r)>8 and isinstance(r[8],dict) and isinstance(r[8].get('alt_geom'),(int,float))) or (len(r)>6 and isinstance(r[6],int) and r[6]&8 and isinstance(r[3],(int,float)))))
  return {'day':day,'icao':hexid,'ok':True,'path':str(path.relative_to(ROOT)),'points':len(rows),'geometric_altitude_points':geom,'registration':v.get('r'),'aircraft_type':v.get('t'),'timestamp':v.get('timestamp'),'source_sha256':hashlib.sha256(wire).hexdigest(),'decoded_sha256':hashlib.sha256(decoded).hexdigest(),'acquisition':receipt}
 except Exception as e:return {'day':day,'icao':hexid,'ok':False,'error':str(e),'url':url}

def main():
 p=argparse.ArgumentParser();p.add_argument('--stage',choices=['replay','traces','all'],default='all');p.add_argument('--days',nargs='*',default=list(SETTINGS));p.add_argument('--workers',type=int,default=4);args=p.parse_args()
 results=[]
 if args.stage in ('replay','all'):
  with concurrent.futures.ThreadPoolExecutor(max_workers=args.workers) as pool:
   tasks=[pool.submit(chunk_task,day,index,SETTINGS[day]) for day in args.days for index in SETTINGS[day]['chunks']]
   results=[t.result() for t in concurrent.futures.as_completed(tasks)]
  (ROOT/'replay-receipt.json').write_text(json.dumps(results,indent=2)+'\n')
 if args.stage in ('traces','all'):
  all_results=[]
  for day in args.days:
   ids=set()
   for path in (ROOT/day/'discovery').glob('*.json'):
    d=json.loads(path.read_text());ids.update(p['id'] for p in d['candidates'])
   with concurrent.futures.ThreadPoolExecutor(max_workers=args.workers) as pool:
    tasks=[pool.submit(trace_task,day,hexid) for hexid in sorted(ids)]
    traces=[t.result() for t in concurrent.futures.as_completed(tasks)]
   (ROOT/day/'trace-receipt.json').write_text(json.dumps(traces,indent=2)+'\n');all_results+=traces
   print(day,'traces',sum(t['ok'] for t in traces),'/',len(traces),'wire bytes',WIRE_BYTES,flush=True)
  (ROOT/'trace-receipt.json').write_text(json.dumps(all_results,indent=2)+'\n')
 print('New wire bytes:',WIRE_BYTES,flush=True)
if __name__=='__main__':main()

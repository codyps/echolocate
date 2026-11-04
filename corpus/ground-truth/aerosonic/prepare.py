#!/usr/bin/env python3
"""Restore/verify four original AeroSonicDB WAVs by HTTP range extraction.

No entire1.169GB archive download. The ZIP central directory and each member's
CRC32 and uncompressed length are verified. Full-archive MD5 is not verified.
"""
import argparse,binascii,hashlib,json,pathlib,struct,urllib.request,zlib
ROOT=pathlib.Path(__file__).resolve().parent
URL='https://zenodo.org/api/records/10215080/files/audio.zip/content'
def get(a,b):
 with urllib.request.urlopen(urllib.request.Request(URL,headers={'Range':f'bytes={a}-{b}'}),timeout=60) as response:
  data=response.read()
  if response.status!=206 or len(data)!=b-a+1:raise RuntimeError('Server did not honor byte range')
  return data
def main():
 p=argparse.ArgumentParser();p.add_argument('--download',action='store_true');p.add_argument('--verify',action='store_true');args=p.parse_args()
 manifest=json.loads((ROOT/'manifest.json').read_text())
 for case in manifest['cases']:
  path=ROOT/case['audio_path']
  if args.download:
   e=case['zip_entry'];header=get(e['offset'],e['offset']+29);h=struct.unpack('<4s5H3L2H',header)
   if h[0]!=b'PK\x03\x04':raise RuntimeError('Invalid member header')
   offset=e['offset']+30+h[9]+h[10];compressed=get(offset,offset+e['compressed_size']-1)
   data=zlib.decompress(compressed,-15) if e['method']==8 else compressed
   if len(data)!=e['size'] or binascii.crc32(data)!=e['crc32']:raise RuntimeError('ZIP integrity failure')
   path.parent.mkdir(exist_ok=True);path.write_bytes(data)
  digest=hashlib.sha256(path.read_bytes()).hexdigest()
  if digest!=case['audio_sha256']:raise RuntimeError('Hash mismatch: '+path.name)
  print(case['id']+': verified '+digest)
if __name__=='__main__':main()

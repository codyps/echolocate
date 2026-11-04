#!/usr/bin/env python3
"""Restore and verify selected DLR REC139 channels; no whole HDF5 download.

Only standard-library file-format conversion. No acoustic processing/inference.
Source-pressure rows are unchanged; normalization is one recorded float32 gain.
"""
import argparse,array,base64,hashlib,json,pathlib,struct,sys,urllib.request
ROOT=pathlib.Path(__file__).resolve().parent
URL='https://zenodo.org/api/records/20494053/files/REC139.h5/content'
def main():
 parser=argparse.ArgumentParser();parser.add_argument('--download',action='store_true');parser.add_argument('--verify',action='store_true');args=parser.parse_args()
 manifest=json.loads((ROOT/'manifest.json').read_text())
 for case in manifest['cases']:
  audio=ROOT/case['audio_path'];pressure=ROOT/case['original_pressure_path']
  if args.download:
   r=case['original_data_range'];req=urllib.request.Request(URL,headers={'Range':f"bytes={r['start']}-{r['end']}"})
   with urllib.request.urlopen(req,timeout=60) as response:
    raw=response.read()
    if response.status!=206 or len(raw)!=r['bytes']:raise RuntimeError('Source ignored byte range')
   if hashlib.sha256(raw).hexdigest()!=r['sha256']:raise RuntimeError('Source-pressure row hash mismatch')
   pressure.write_bytes(base64.b64decode(case['npy_header_base64'])+raw)
   values=array.array('f');values.frombytes(raw)
   if sys.byteorder!='little':values.byteswap()
   gain=struct.unpack('<f',struct.pack('<f',case['normalization_gain']))[0]
   scaled=array.array('f',(v*gain for v in values))
   if sys.byteorder!='little':scaled.byteswap()
   pcm=scaled.tobytes();rate=int(case['sample_rate_hz']);fmt=struct.pack('<HHIIHH',3,1,rate,rate*4,4,32)
   wav=b'RIFF'+struct.pack('<I',36+len(pcm))+b'WAVEfmt '+struct.pack('<I',16)+fmt+b'data'+struct.pack('<I',len(pcm))+pcm;audio.write_bytes(wav)
  for path,expected in [(audio,case['audio_sha256']),(pressure,case['original_pressure_sha256'])]:
   actual=hashlib.sha256(path.read_bytes()).hexdigest()
   if actual!=expected:raise RuntimeError('Mismatch '+path.name)
  print(case['id']+': WAV and original pressure hashes verified')
if __name__=='__main__':main()

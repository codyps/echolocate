#!/usr/bin/env python3
"""Restore/verify the creator-linked contiguous ambient-audio excerpt.

Requires ffmpeg and yt-dlp for --download. No third-party implementation assets
or code are used. Copyright/redistribution permission has not been established.
"""
import argparse,hashlib,json,pathlib,subprocess,sys,tempfile
ROOT=pathlib.Path(__file__).resolve().parent
URL='https://www.youtube.com/watch?v=4CxVxuMVkMc'
def main():
 p=argparse.ArgumentParser();p.add_argument('--download',action='store_true');p.add_argument('--verify',action='store_true');p.add_argument('--source-m4a',type=pathlib.Path);args=p.parse_args()
 manifest=json.loads((ROOT/'manifest.json').read_text());target=ROOT/manifest['audio']['path']
 with tempfile.TemporaryDirectory(prefix='echo-colsto-') as temp:
  source=args.source_m4a
  if args.download:
   source=pathlib.Path(temp)/'original.m4a'
   subprocess.run([sys.executable,'-m','yt_dlp','--no-warnings','--no-part','-f','140','-o',str(source),URL],check=True)
  if source:
   subprocess.run(['ffmpeg','-v','error','-i',str(source),'-ss','2100','-t','1080','-map','0:a:0','-ac','1','-ar','16000','-c:a','pcm_s16le','-map_metadata','-1','-fflags','+bitexact','-flags:a','+bitexact','-y',str(target)],check=True)
 actual=hashlib.sha256(target.read_bytes()).hexdigest()
 if actual!=manifest['audio']['sha256']:raise RuntimeError('Excerpt SHA256 differs; source format or decoder may have changed')
 print('Verified contiguous18-minute excerpt:',actual)
if __name__=='__main__':main()

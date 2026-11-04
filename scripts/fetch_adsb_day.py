#!/usr/bin/env python3
"""Acquire a public daily archive, then call Rust to produce a regional package.

Network/archive orchestration only. No Python audio analysis or trajectory filtering.
Downloads are bounded and checksummed; no credentials required. Public GitHub rate limits apply.
"""
import argparse, datetime, hashlib, io, json, pathlib, subprocess, tarfile, urllib.request

ROOT=pathlib.Path(__file__).resolve().parents[1]
def get(url):
    return urllib.request.urlopen(urllib.request.Request(url,headers={'User-Agent':'echolocate-research/0.1','Accept':'application/vnd.github+json'}),timeout=60)
class Joined(io.RawIOBase):
    def __init__(self,paths):self.paths=iter(paths);self.current=None
    def readable(self):return True
    def readinto(self,buffer):
        while True:
            if self.current is None:
                try:self.current=open(next(self.paths),'rb')
                except StopIteration:return 0
            n=self.current.readinto(buffer)
            if n:return n
            self.current.close();self.current=None
    def close(self):
        if self.current:self.current.close()
        super().close()
def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('date',help='UTC archive day YYYY-MM-DD');p.add_argument('--bounds',required=True,help='west,south,east,north (use --bounds=-123,37,-122,38)');p.add_argument('--output',type=pathlib.Path,required=True);p.add_argument('--cache',type=pathlib.Path,default=ROOT/'validation-runs/adsb-cache');p.add_argument('--max-download-mib',type=int,default=512);p.add_argument('--max-extract-mib',type=int,default=16384);p.add_argument('--instance',default='prod-0',choices=['prod-0','prod-1','staging-0']);p.add_argument('--dry-run',action='store_true');p.add_argument('--binary',type=pathlib.Path,default=ROOT/'target/release/echo-locate');args=p.parse_args()
    day=datetime.date.fromisoformat(args.date);tag=f"v{day.strftime('%Y.%m.%d')}-planes-readsb-{args.instance}"
    metadata=None;errors=[]
    for repo in [f'globe_history_{day.year}','globe_history']:
        url=f'https://api.github.com/repos/adsblol/{repo}/releases/tags/{tag}'
        try:
            with get(url) as r:metadata=json.load(r)
            break
        except Exception as e:errors.append(f'{url}: {e}')
    if metadata is None:raise SystemExit('Archive unavailable.\n'+'\n'.join(errors))
    assets=sorted([a for a in metadata.get('assets',[]) if '.tar.' in a['name'] and a['name'].split('.tar.')[-1].isalpha()],key=lambda a:a['name'])
    if not assets:raise SystemExit('No supported split tar assets in release; inspect '+metadata.get('html_url',''))
    total=sum(a['size'] for a in assets);print(json.dumps({'release':metadata['html_url'],'parts':len(assets),'bytes':total,'license':'ODbL-1.0','download_budget_bytes':args.max_download_mib*1024*1024},indent=2))
    if args.dry_run:return
    if total>args.max_download_mib*1024*1024:raise SystemExit('Archive exceeds download budget. Set --max-download-mib explicitly to a sufficient budget; no files downloaded.')
    cache=args.cache/tag;cache.mkdir(parents=True,exist_ok=True);receipt={'release_url':metadata['html_url'],'license':'ODbL-1.0','assets':[]};paths=[]
    for asset in assets:
        path=cache/pathlib.Path(asset['name']).name;partial=path.with_suffix(path.suffix+'.partial');hasher=hashlib.sha256()
        if not path.exists() or path.stat().st_size!=asset['size']:
            with get(asset['browser_download_url']) as r,partial.open('wb') as out:
                n=0
                while block:=r.read(1024*1024):
                    n+=len(block)
                    if n>asset['size']:raise RuntimeError('Download exceeds declared asset size')
                    out.write(block)
            if partial.stat().st_size!=asset['size']:raise RuntimeError('Incomplete download')
            partial.replace(path)
        with path.open('rb') as f:
            while block:=f.read(1024*1024):hasher.update(block)
        digest=hasher.hexdigest();declared=asset.get('digest')
        if declared and declared.startswith('sha256:') and digest!=declared[7:]:raise RuntimeError('Published checksum mismatch: '+path.name)
        receipt['assets'].append({'name':path.name,'bytes':path.stat().st_size,'sha256':digest,'publisher_digest':declared});paths.append(path)
    traces=cache/'traces';traces.mkdir(exist_ok=True);extracted=0;count=0
    with io.BufferedReader(Joined(paths)) as stream,tarfile.open(fileobj=stream,mode='r|*') as archive:
        for entry in archive:
            if not entry.isfile() or not entry.name.endswith(('.json','.json.gz')) or 'trace' not in entry.name.lower():continue
            extracted+=entry.size
            if extracted>args.max_extract_mib*1024*1024:raise RuntimeError('Extracted-byte budget exceeded')
            # Content-addressed flat names avoid archive path traversal and name collisions.
            name=hashlib.sha256(entry.name.encode()).hexdigest()+('.json.gz' if entry.name.endswith('.gz') else '.json')
            target=traces/name;source=archive.extractfile(entry)
            if source is None:continue
            with target.open('wb') as out:
                while block:=source.read(1024*1024):out.write(block)
            count+=1
    receipt.update(trace_files=count,extracted_bytes=extracted);(cache/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
    subprocess.run([str(args.binary),'import-traces',str(traces),'--bounds',args.bounds,'--output',str(args.output)],check=True)
    print('Package ready:',args.output)
if __name__=='__main__':main()

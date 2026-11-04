#!/usr/bin/env python3
"""Package public application artifacts; retain restricted media only in local corpus."""
import hashlib,json,pathlib,shutil,zipfile
ROOT=pathlib.Path(__file__).resolve().parents[1]
OUT=ROOT.parent

def archive(name,paths):
 path=OUT/name
 with zipfile.ZipFile(path,'w',zipfile.ZIP_DEFLATED,compresslevel=6) as z:
  for source,destination in paths:z.write(source,destination)
 print(name,path.stat().st_size)
 return path

def main():
 shipped=[]
 shipped.append(archive('echolocate-static.zip',[(p,str(p.relative_to(ROOT/'dist'))) for p in sorted((ROOT/'dist').rglob('*')) if p.is_file()]))
 data=[(p,str(p.relative_to(ROOT/'web'))) for p in sorted((ROOT/'web/data').rglob('*')) if p.is_file()]
 data.append((ROOT/'docs/STATIC_DATA.md','STATIC_DATA.md'))
 shipped.append(archive('echolocate-flight-data.zip',data))
 public=[]
 for p in sorted(ROOT.rglob('*')):
  rel=p.relative_to(ROOT);parts=rel.parts
  if any(x in ('target','node_modules','dist','.git','__pycache__') for x in parts):continue
  if not p.is_file() or p.is_symlink():continue
  if parts[:2]==('validation-runs','jobs'):continue
  if parts[:2]==('corpus','audio'):continue
  if parts[:3] in (('corpus','ground-truth','colsto'),('corpus','ground-truth','aerosonic')) and p.suffix.lower() in ('.wav','.mp3','.m4a','.mp4','.png','.jpg'):continue
  if parts[:2]==('corpus','news-tracks') and 'replay' in parts:continue
  public.append((p,str(pathlib.Path('doppler-locate')/rel)))
 shipped.append(archive('echolocate-project.zip',public))
 binary=OUT/'echo-locate-linux-x86_64';shutil.copy2(ROOT/'bin/echo-locate-linux-x86_64',binary);shipped.append(binary)
 manifest={p.name:{'bytes':p.stat().st_size,'sha256':hashlib.sha256(p.read_bytes()).hexdigest()} for p in shipped}
 (OUT/'artifact-manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
 print(json.dumps(manifest,indent=2))
if __name__=='__main__':main()

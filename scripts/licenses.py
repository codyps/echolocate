#!/usr/bin/env python3
"""Assemble dependency notices from Cargo's resolved source packages."""
import json,pathlib,subprocess
root=pathlib.Path(__file__).resolve().parents[1]
metadata=json.loads(subprocess.check_output(['cargo','metadata','--locked','--format-version','1'],cwd=root))
parts=['Echolocate — third-party dependency notices','', (root/'LICENSE').read_text()]
for package in sorted(metadata['packages'],key=lambda p:(p['name'],p['version'])):
    if package['source'] is None:continue
    directory=pathlib.Path(package['manifest_path']).parent
    parts+=['\n'+'='*72,f"{package['name']} {package['version']} — {package.get('license') or 'see upstream'}",package.get('repository') or '']
    files=sorted({p for pattern in ['LICENSE*','LICENCE*','COPYING*'] for p in directory.glob(pattern) if p.is_file()})
    if files:
        for path in files:parts += [path.name,path.read_text(errors='replace')]
    else:parts+=['License text: consult the upstream repository and packaged source.']
(root/'docs/THIRD_PARTY_NOTICES.txt').write_text('\n'.join(parts)+'\n')

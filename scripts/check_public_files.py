#!/usr/bin/env python3
"""Keep restricted research media and oversized artifacts out of the public source tree."""
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]

def main():
    tracked = subprocess.check_output(['git', 'ls-files', '-z'], cwd=ROOT).decode().split('\0')
    failures = []
    count = total = 0
    for name in filter(None, tracked):
        path = ROOT / name
        if not path.is_file():
            failures.append(f'Non-file or missing tracked path: {name}')
            continue
        forbidden = (
            name.startswith('corpus/audio/')
            or name.startswith('corpus/ground-truth/aerosonic/audio/')
            or (name.startswith('corpus/ground-truth/colsto/') and path.suffix.lower() in {'.png', '.jpg', '.jpeg', '.wav', '.mp3', '.mp4', '.m4a'})
        )
        if forbidden:
            failures.append(f'Restricted research media: {name}')
        if path.is_symlink() or path.stat().st_size >= 100 * 1024 * 1024:
            failures.append(f'Unsupported public artifact: {name}')
        count += 1
        total += path.stat().st_size
    if not count:
        raise SystemExit('No tracked source files; stage the proposed publication first.')
    if failures:
        raise SystemExit('\n'.join(failures))
    print(f'Checked {count} public source files ({total:,} bytes); restricted media excluded, each file below 100 MiB.')

if __name__ == '__main__':
    main()

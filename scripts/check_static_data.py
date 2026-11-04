#!/usr/bin/env python3
"""Validate actual static asset hashes, completeness, licenses, and hosting budgets."""
import argparse,hashlib,json,pathlib
def main():
 p=argparse.ArgumentParser();p.add_argument('directory',type=pathlib.Path);a=p.parse_args();root=a.directory.resolve();data=root/'data';catalog=json.loads((data/'catalog.json').read_text());total=0
 assert catalog['schema_version']==1
 for entry in catalog['packages']:
  assert entry['license'] in ('ODbL-1.0','CC-BY-4.0','CC0-1.0','MIT')
  count=0
  for item in [entry['metadata']]+entry['shards']:
   path=(data/item['url']).resolve();assert path.is_relative_to(data.resolve());b=path.read_bytes();assert len(b)==item['bytes'];assert hashlib.sha256(b).hexdigest()==item['sha256'];assert len(b)<100*1024*1024
   if item is not entry['metadata']:count+=len(json.loads(b))
   total+=len(b)
  assert count==entry['flight_count'];assert entry['transfer_bytes']<=16*1024*1024
 assert (data/'ATTRIBUTION.md').exists()
 assert not list(root.rglob('fox-*.wav'))
 site_size=sum(p.stat().st_size for p in root.rglob('*') if p.is_file());assert site_size<1024**3
 print(f"Verified {len(catalog['packages'])} packages, {total:,} flight bytes; site {site_size:,} bytes <1 GiB, every data file <100 MiB; SHA-256 and attribution present")
if __name__=='__main__':main()

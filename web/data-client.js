const LIMIT=16*1024*1024;
export async function loadCatalog(url,fetcher=fetch){
  const absolute=new URL(url,globalThis.location?.href||'https://example.invalid/').href;
  const response=await fetcher(absolute);if(!response.ok)throw new Error(`Flight catalog unavailable (${response.status}).`);
  const catalog=await response.json();if(catalog.schema_version!==1||catalog.format!=='echolocate-flight-catalog-v1'||!Array.isArray(catalog.packages)||catalog.packages.length>10000)throw new Error('Unsupported flight catalog.');
  return {catalog,url:absolute};
}
async function verified(asset,base,fetcher){
  if(!Number.isInteger(asset.bytes)||asset.bytes<1||asset.bytes>LIMIT||!/^[a-f0-9]{64}$/.test(asset.sha256))throw new Error('Invalid catalog asset size or checksum.');
  const url=new URL(asset.url,base);if(url.origin!==new URL(base).origin||!['https:','http:'].includes(url.protocol))throw new Error('Catalog shards must share their catalog origin.');
  const r=await fetcher(url.href);if(!r.ok)throw new Error(`Flight asset unavailable (${r.status}).`);
  const chunks=[];let size=0;
  if(r.body){const reader=r.body.getReader();for(;;){const {done,value}=await reader.read();if(done)break;size+=value.length;if(size>asset.bytes){await reader.cancel();throw new Error('Flight asset exceeds its declared size.');}chunks.push(value);}}
  else{const b=new Uint8Array(await r.arrayBuffer());size=b.length;chunks.push(b);}
  if(size!==asset.bytes)throw new Error('Incomplete flight asset.');
  const bytes=new Uint8Array(size);let at=0;for(const c of chunks){bytes.set(c,at);at+=c.length;}
  if(!globalThis.crypto?.subtle)throw new Error('Verified flight downloads require HTTPS or localhost.');
  const digest=Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',bytes)),b=>b.toString(16).padStart(2,'0')).join('');
  if(digest!==asset.sha256)throw new Error('Flight asset SHA-256 mismatch.');
  return JSON.parse(new TextDecoder().decode(bytes));
}
export async function loadFlightPackage(entry,catalogURL,onProgress=()=>{},fetcher=fetch){
  if(!Array.isArray(entry.shards)||entry.shards.length>1024)throw new Error('Invalid flight shard list.');
  const assets=[entry.metadata,...entry.shards];const total=assets.reduce((n,a)=>n+(a?.bytes||0),0);
  if(total>LIMIT)throw new Error('This data window exceeds the 16 MiB browser budget. Choose a smaller region or time window.');
  const pack=await verified(entry.metadata,catalogURL,fetcher);if(!Array.isArray(pack.flights)||pack.flights.length)throw new Error('Metadata must contain an empty flights list.');
  let loaded=entry.metadata.bytes;onProgress(loaded,total);
  for(const shard of entry.shards){const flights=await verified(shard,catalogURL,fetcher);if(!Array.isArray(flights)||flights.length!==shard.flights)throw new Error('Invalid flight shard count.');pack.flights.push(...flights);loaded+=shard.bytes;onProgress(loaded,total);}
  if(pack.flights.length!==entry.flight_count||pack.license!==entry.license||pack.schema_version!==1)throw new Error('Package metadata disagrees with catalog.');
  return pack;
}

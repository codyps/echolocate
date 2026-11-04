import {readFile} from 'node:fs/promises';
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {test} from 'node:test';
const root=new URL('../',import.meta.url);
const json=async path=>JSON.parse(await readFile(new URL(path,root),'utf8'));
const wasm=await WebAssembly.instantiate(await readFile(new URL('web/echo_core.wasm',root)),{});
const e=wasm.instance.exports;
function run(request,bytes){
  const meta=new TextEncoder().encode(JSON.stringify(request));const rp=e.echo_alloc(meta.length);new Uint8Array(e.memory.buffer,rp,meta.length).set(meta);
  const ap=e.echo_alloc(bytes.length);new Uint8Array(e.memory.buffer,ap,bytes.length).set(bytes);
  const packed=e.echo_run(rp,meta.length,ap,bytes.length);const ptr=Number(packed>>32n),len=Number(packed&0xffffffffn);
  const result=JSON.parse(new TextDecoder().decode(new Uint8Array(e.memory.buffer,ptr,len)));
  e.echo_free(ptr,len);e.echo_free(rp,meta.length);e.echo_free(ap,bytes.length);return result;
}
test('native and browser Wasm recover the same automatically detected synthetic solution',async()=>{
  const request={config:await json('web/demo.config.json'),package:await json('web/demo.package.json')};
  const native=JSON.parse(execFileSync(new URL('target/release/echo-locate',root).pathname,['analyze','web/demo.wav','--config','web/demo.config.json','--package','web/demo.package.json'],{cwd:root,maxBuffer:32*1024*1024,stdio:['ignore','pipe','pipe']}));
  const browser=run(request,await readFile(new URL('web/demo.wav',root)));
  assert.equal(browser.ok,true);assert.equal(browser.analysis.status,native.analysis.status);
  assert.equal(browser.analysis.events.length,native.analysis.events.length);
  const a=native.analysis.candidates[0],b=browser.analysis.candidates[0];
  assert.equal(a.matched_events,b.matched_events);assert.ok(Math.abs(a.lat-b.lat)<0.0001);assert.ok(Math.abs(a.lon-b.lon)<0.0001);assert.ok(Math.abs(a.start_unix_s-b.start_unix_s)<0.1);assert.ok(Math.abs(a.score-b.score)<0.05);
  assert.equal(browser.analysis.score_kind,'profiled_ridge_shape_contrast_not_probability');
});
test('Wasm rejects malformed audio without claiming a location',()=>{const r=run({},new TextEncoder().encode('not audio'));assert.equal(r.ok,false);assert.equal(typeof r.error,'string');});
test('Wasm validates resource settings',()=>{const r=run({config:{max_hypotheses:0}},new Uint8Array([1]));assert.equal(r.ok,false);assert.match(r.error,/Resource/);});
test('coherent ridge paths and robust scoring agree between native Rust and Wasm',async()=>{
  const config={...await json('web/demo.config.json'),coherent_ridges:true,robust_shape:true,multiresolution:false,max_events:16,frequency_sigma_hz:2,hop_s:.5};
  const request={config,package:await json('web/demo.package.json')};
  const {mkdtemp,writeFile,rm}=await import('node:fs/promises');const os=await import('node:os');const path=await import('node:path');const dir=await mkdtemp(path.join(os.tmpdir(),'echo-coherent-'));
  try{
    await writeFile(path.join(dir,'config.json'),JSON.stringify(config));
    const native=JSON.parse(execFileSync(new URL('target/release/echo-locate',root).pathname,['analyze','web/demo.wav','--config',path.join(dir,'config.json'),'--package','web/demo.package.json'],{cwd:root,maxBuffer:32*1024*1024,stdio:['ignore','pipe','pipe']}));
    const browser=run(request,await readFile(new URL('web/demo.wav',root)));assert.equal(browser.ok,true);assert.equal(native.ok,true);
    assert.equal(browser.analysis.status,native.analysis.status);assert.equal(browser.analysis.events.length,native.analysis.events.length);assert.ok(browser.analysis.events.length>0);assert.equal(browser.analysis.score_kind,'profiled_robust_doppler_vs_linear_chirp_not_probability');
    assert.equal(browser.analysis.candidates.length,native.analysis.candidates.length);
    if(native.analysis.candidates.length){const a=native.analysis.candidates[0],b=browser.analysis.candidates[0];assert.ok(Math.abs(a.lat-b.lat)<.0001);assert.ok(Math.abs(a.lon-b.lon)<.0001);assert.ok(Math.abs(a.score-b.score)<.05);}
  }finally{await rm(dir,{recursive:true,force:true});}
});

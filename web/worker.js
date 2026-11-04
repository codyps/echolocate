let instance;
async function engine() {
  if (!instance) {
    const response = await fetch('echo_core.wasm');
    if (!response.ok) throw new Error('Rust WebAssembly bundle missing. Run scripts/build.sh.');
    const loaded = await WebAssembly.instantiate(await response.arrayBuffer(), {});
    instance = loaded.instance.exports;
  }
  return instance;
}
self.onmessage = async ({data}) => {
  const buffers=[];
  try {
    postMessage({type:'stage',message:'Loading the Rust engine…'});
    const e=await engine();
    const metadata=new TextEncoder().encode(JSON.stringify(data.request));
    const audio=new Uint8Array(data.audio);
    if(metadata.byteLength>16*1024*1024 || audio.byteLength>64*1024*1024) throw new Error('Input exceeds metadata/audio size limits.');
    const rp=e.echo_alloc(metadata.length);buffers.push([rp,metadata.length]);
    new Uint8Array(e.memory.buffer,rp,metadata.length).set(metadata);
    const ap=e.echo_alloc(audio.length);buffers.push([ap,audio.length]);
    new Uint8Array(e.memory.buffer,ap,audio.length).set(audio);
    postMessage({type:'stage',message:'Rust: decoding audio → detecting ridges → matching trajectories…'});
    const packed=e.echo_run(rp,metadata.length,ap,audio.length);
    const ptr=Number(packed>>32n),len=Number(packed&0xffffffffn);buffers.push([ptr,len]);
    const text=new TextDecoder().decode(new Uint8Array(e.memory.buffer,ptr,len));
    postMessage({type:'result',result:JSON.parse(text)});
  } catch(error) {postMessage({type:'error',error:error.message});}
  finally {if(instance)for(const [ptr,len] of buffers)instance.echo_free(ptr,len);}
};

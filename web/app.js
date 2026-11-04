import {loadCatalog,loadFlightPackage} from './data-client.js';
const $=id=>document.getElementById(id);
let audioFile=null,flightPackage=null,result=null,selected=0,worker=null,job=null,timer=null,start=0,polling=false;
let previewURL=null,processing=false;
let hostedCatalog=null;
const activity=(message,error=false)=>{$('activity').textContent=message;$('activity').classList.toggle('error',error);};
function chooseAudio(file){
  if(processing){activity('Cancel or finish the current analysis before changing audio.');return;}
  if(!file)return;
  if(file.size>64*1024*1024){activity('Audio exceeds 64 MiB. Extract a shorter lossless excerpt.',true);return;}
  clearResults();audioFile=file;$('audio-name').textContent=file.name;$('source-state').textContent='LOADED';$('analyze').disabled=false;
  if(previewURL)URL.revokeObjectURL(previewURL);previewURL=URL.createObjectURL(file);$('player').src=previewURL;$('player').hidden=false;
  activity('Ready. Analysis runs on this device.');
}
function clearResults(){result=null;selected=0;$('candidates').hidden=true;$('candidates').querySelector('tbody').replaceChildren();$('candidate-empty').hidden=false;$('candidate-empty').textContent='Run analysis to inspect this recording.';$('map-empty').hidden=false;$('spectrum-empty').hidden=false;$('event-chips').replaceChildren();$('event-count').textContent='0 events';$('status-badge').textContent='READY TO LISTEN';$('status-badge').classList.remove('warn');$('export-json').disabled=true;$('export-geo').disabled=true;$('coverage').textContent='';$('warnings').replaceChildren();$('warning-count').textContent='';drawAll();}
$('audio-file').addEventListener('change',e=>chooseAudio(e.target.files[0]));
for(const type of ['dragover','dragleave','drop'])$('drop').addEventListener(type,e=>{e.preventDefault();$('drop').classList.toggle('drag',type==='dragover');if(type==='drop')chooseAudio(e.dataTransfer.files[0]);});
$('package-file').addEventListener('change',async e=>{try{const file=e.target.files[0];if(!file)return;if(file.size>16*1024*1024)throw new Error('Package exceeds 16 MiB; split it into smaller tiles.');const p=JSON.parse(await file.text());if(p.schema_version!==1 || !Array.isArray(p.flights))throw new Error('Expected a version 1 flight package.');flightPackage=p;$('data-attribution').textContent='';$('package-name').textContent=`${p.name} · ${p.flights.length} flights`;}catch(e){flightPackage=null;$('package-name').textContent='No valid package loaded.';activity(e.message,true);}});
async function hostedData(){
  try{hostedCatalog=await loadCatalog(new URL('data/catalog.json',location.href));const select=$('catalog-package');select.replaceChildren();
    for(const [i,entry] of hostedCatalog.catalog.packages.entries()){const o=document.createElement('option');o.value=String(i);o.textContent=`${entry.name} · ${(entry.transfer_bytes/1024).toFixed(0)} KiB`;select.append(o);}
    select.disabled=processing||!select.options.length;$('load-data').disabled=processing||!select.options.length;
  }catch{ $('catalog-package').options[0].textContent='Import a package file above'; }
}
$('load-data').addEventListener('click',async()=>{try{if(processing)return;const entry=hostedCatalog.catalog.packages[Number($('catalog-package').value)];$('load-data').disabled=true;
  const pack=await loadFlightPackage(entry,hostedCatalog.url,(n,total)=>activity(`Downloading flight data · ${(100*n/total).toFixed(0)}% verified bytes`));
  clearResults();flightPackage=pack;$('package-name').textContent=`${pack.name} · ${pack.flights.length} flights`;$('data-attribution').textContent=entry.attribution;
  activity('Flight data verified and loaded. Choose audio and its time uncertainty.');
}catch(e){activity(e.message,true);}finally{$('load-data').disabled=processing||!hostedCatalog;}});
hostedData();
function timeFields(){const mode=$('time-mode').value;$('time-fields').hidden=mode==='none';$('sigma-field').hidden=mode!=='estimate';$('end-field').hidden=mode!=='range';$('start-label').textContent=mode==='range'?'Earliest start · UTC':'Estimated start · UTC';}
$('time-mode').addEventListener('change',timeFields);
const utcInput=unix=>new Date(unix*1000).toISOString().slice(0,19);
$('demo').addEventListener('click',async()=>{try{
  activity('Loading the generated example…');
  const responses=await Promise.all(['demo.wav','demo.package.json','demo.config.json'].map(p=>fetch(p)));
  if(responses.some(r=>!r.ok))throw new Error('Demo fixtures missing. Run scripts/build.sh.');
  const [blob,p,c]=await Promise.all([responses[0].blob(),responses[1].json(),responses[2].json()]);
  $('tracking').value='legacy';flightPackage=p;$('data-attribution').textContent='';chooseAudio(new File([blob],'synthetic-two-flyovers.wav',{type:'audio/wav'}));
  $('package-name').textContent=`SYNTHETIC · ${p.flights.length} simulated flight tracks`;
  $('grid').value=c.grid_spacing_m;$('height').value=c.observer_height_ellipsoid_m;
  $('time-mode').value='range';$('time-start').value=utcInput(c.time_prior.windows[0].start_unix_s);$('time-end').value=utcInput(c.time_prior.windows[0].end_unix_s);timeFields();
  $('overrides').value=JSON.stringify({max_hz:c.max_hz,max_hypotheses:c.max_hypotheses},null,2);
  activity('Synthetic example ready. Click Analyze recording; this example is not field validation.');
}catch(e){activity(e.message,true);}});
function request(){
  const override=JSON.parse($('overrides').value||'{}');if(!override || typeof override!=='object' || Array.isArray(override))throw new Error('Config overrides must be a JSON object.');
  const profile=$('tracking').value==='coherent'?{max_audio_s:1080,coherent_ridges:true,robust_shape:true,multiresolution:false,max_events:16,frequency_sigma_hz:2,hop_s:.5}:{};
  const config={grid_spacing_m:Number($('grid').value),observer_height_ellipsoid_m:Number($('height').value),...profile,...override};
  const mode=$('time-mode').value;
  if(mode!=='none'){
    const a=Date.parse($('time-start').value+'Z')/1000;if(!Number.isFinite(a))throw new Error('Supply a valid recording start in UTC.');
    if(mode==='estimate'){const sigma=Number($('time-sigma').value);if(!(sigma>0))throw new Error('Time uncertainty must be positive.');config.time_prior={estimate_unix_s:a,sigma_s:sigma};}
    else{const b=Date.parse($('time-end').value+'Z')/1000;if(!Number.isFinite(b)||b<a)throw new Error('Latest start must follow earliest start.');config.time_prior={windows:[{start_unix_s:a,end_unix_s:b,weight:1}]};}
  }
  return {config,package:flightPackage};
}
function setBusy(busy){processing=busy;for(const input of document.querySelectorAll('.controls input,.controls select,.controls textarea'))input.disabled=busy;$('analyze').disabled=busy||!audioFile;$('demo').disabled=busy;$('cancel').hidden=!busy;if(busy){start=Date.now();$('status-badge').textContent='ANALYZING';timer=setInterval(()=>{$('source-state').textContent=`${((Date.now()-start)/1000).toFixed(0)}s ELAPSED`;},1000);}else{clearInterval(timer);$('source-state').textContent=audioFile?'LOADED':'WAITING';}}
$('analyze').addEventListener('click',async()=>{
  try{
    const req=request();if(!audioFile)throw new Error('Choose an audio file.');
    clearResults();setBusy(true);const bytes=await audioFile.arrayBuffer();
    if($('execution').value==='native'){
      const health=await fetch('/api/health');if(!health.ok || !(health.headers.get('content-type')||'').includes('application/json'))throw new Error('Native execution needs echo-locate serve on this origin. Select browser execution on a static host.');
      const meta=new TextEncoder().encode(JSON.stringify(req));const envelope=new Uint8Array(4+meta.length+bytes.byteLength);new DataView(envelope.buffer).setUint32(0,meta.length,true);envelope.set(meta,4);envelope.set(new Uint8Array(bytes),4+meta.length);
      activity('Sending audio to your local Rust service…');const response=await fetch('/api/jobs',{method:'POST',headers:{'Content-Type':'application/octet-stream'},body:envelope});
      const data=await response.json();if(!response.ok)throw new Error(data.error||'Local service unavailable. Run echo-locate serve, or select browser execution.');
      job=data.id;localStorage.setItem('echo-native-job',job);await pollJob();
    }else{
      worker=new Worker('worker.js',{type:'module'});
      worker.onmessage=({data})=>{if(data.type==='stage')activity(data.message);else if(data.type==='error')fail(data.error);else if(data.type==='result')accept(data.result);};
      worker.onerror=e=>fail(e.message||'Browser worker failed.');worker.postMessage({request:req,audio:bytes},[bytes]);
    }
  }catch(e){fail(e.message);}
});
async function pollJob(){
  polling=true;
  while(polling&&job){
    const response=await fetch(`/api/jobs/${job}`);if(!response.ok)throw new Error('Saved native job unavailable.');const state=await response.json();
    if(['complete','failed','cancelled'].includes(state.state)){const r=await fetch(`/api/jobs/${job}/result`);const data=await r.json();job=null;polling=false;localStorage.removeItem('echo-native-job');accept(data);break;}
    if(state.state==='interrupted')throw new Error(`Native job interrupted. Run echo-locate resume validation-runs/jobs/${job} to rerun the saved inputs.`);
    activity(`${state.stage||state.state}. Job ${job.slice(-6)} · saved locally.`);await new Promise(r=>setTimeout(r,500));
  }
}
function fail(message){worker?.terminate();worker=null;polling=false;setBusy(false);$('status-badge').textContent='NEEDS ATTENTION';$('status-badge').classList.add('warn');activity(message,true);}
function accept(envelope){
  worker?.terminate();worker=null;setBusy(false);if(!envelope.ok){fail(envelope.error);return;}
  result=envelope.analysis;selected=0;render();activity(`Finished in ${((Date.now()-start)/1000).toFixed(1)}s. ${result.events.length} events; ${result.candidates.length} candidate regions. Scores are uncalibrated.`);
}
$('cancel').addEventListener('click',async()=>{
  if(worker){worker.terminate();worker=null;setBusy(false);activity('Browser analysis cancelled.');$('status-badge').textContent='CANCELLED';}
  if(job){try{await fetch(`/api/jobs/${job}/cancel`,{method:'POST'});activity('Cancellation requested; Rust stops at its next search checkpoint.');}catch(e){activity(e.message,true);}}
});
function render(){
  document.querySelector('.map-label span').textContent=result.coordinate_frame==='local_cartesian_embedded'?'SOURCE LOCAL FRAME / ARBITRARY MAP ORIGIN':'WGS84 / OFFLINE COORDINATE MAP';
  document.querySelector('#candidates thead th:nth-child(3)').textContent=result.clock_domain==='utc'?'START · UTC':'START · INTERNAL CLOCK';
  const labels={no_usable_events:'NO USABLE EVENTS',needs_flight_data:'NEEDS FLIGHT DATA',no_match:'NO MATCH',ambiguous:'AMBIGUOUS',search_incomplete:'SEARCH INCOMPLETE',candidates_uncalibrated:'UNCALIBRATED CANDIDATES'};
  $('status-badge').textContent=result.synthetic?'SYNTHETIC / '+(result.candidates.length?'CANDIDATES':'DISCOVERY'):labels[result.status]||result.status;
  $('status-badge').classList.toggle('warn',!result.synthetic);
  $('signal-meta').textContent=`${result.audio.analyzed_duration_s.toFixed(1)}s · CH ${result.audio.analyzed_channel+1}/${result.audio.channels} · ${(result.audio.sample_rate/1000).toFixed(0)} kHz`;
  $('event-count').textContent=`${result.events.length} events`;
  $('spectrum-empty').hidden=true;$('event-chips').replaceChildren();
  for(const e of result.events){const b=document.createElement('button');b.className='event-chip';b.textContent=`E${e.id+1} / ${e.start_s.toFixed(1)}–${e.end_s.toFixed(1)}s / ${e.drop_hz.toFixed(1)} Hz drop`;b.onclick=()=>{$('player').currentTime=e.start_s;};$('event-chips').append(b);}
  const tbody=$('candidates').querySelector('tbody');tbody.replaceChildren();
  result.candidates.forEach((c,i)=>{
    const row=document.createElement('tr');row.tabIndex=0;row.className=i===selected?'selected':'';row.setAttribute('aria-label',`Select candidate ${i+1}`);
    for(const s of [`${String(i+1).padStart(2,'0')} / Region`,`${c.lat.toFixed(5)}, ${c.lon.toFixed(5)}`,result.clock_domain==='utc'?new Date(c.start_unix_s*1000).toISOString().replace('T',' ').slice(0,19):`${c.start_unix_s.toFixed(3)} s`,`${c.matched_events}/${result.events.length}`,c.score.toFixed(2)]){const td=document.createElement('td');td.textContent=s;row.append(td);}
    row.onclick=()=>{selected=i;render();};row.onkeydown=e=>{if(e.key==='Enter'||e.key===' '){e.preventDefault();row.click();}};tbody.append(row);
  });
  $('candidates').hidden=!result.candidates.length;$('candidate-empty').hidden=!!result.candidates.length;
  if(!result.candidates.length)$('candidate-empty').textContent=result.status==='needs_flight_data'?'Acoustic events found. Add a flight-data package to search locations.':result.status==='no_usable_events'?'No suitable descending tonal events detected. This does not prove there were no aircraft.':'No location survived the current model and search. Check time, coverage and model assumptions.';
  $('warnings').replaceChildren();for(const text of result.warnings){const li=document.createElement('li');li.textContent=text;$('warnings').append(li);}
  $('warning-count').textContent=`${result.warnings.length} notes`;
  if(result.config.robust_shape){const li=document.createElement('li');li.textContent='Coherent research mode compares flight curves with a linear chirp. Some matches use a declared part of a detected curve. Separate-recording tests still leave distant alternatives; scores are not confidence.';$('warnings').append(li);$('warning-count').textContent=`${result.warnings.length+1} notes`;}
  $('coverage').textContent=result.search?`${result.search.cells_evaluated} cells · ${result.search.hypotheses_evaluated.toLocaleString()} hypotheses · ${result.search.budget_exhausted?'BUDGET EXHAUSTED':'configured search finished'} · ${result.data_source}`:'Acoustic analysis only. No geographic search performed.';
  $('map-empty').hidden=!!result.search;$('export-json').disabled=false;$('export-geo').disabled=!result.candidates.length||result.coordinate_frame!=='wgs84';
  $('map-footnote').textContent=result.synthetic?'Generated scenario · independent analytic audio renderer · no real-world accuracy claim.':'Profiled match strength; unscored cells have unknown support.';
  drawAll();
}
function context(id){const canvas=$(id);const rect=canvas.getBoundingClientRect();const ratio=Math.min(devicePixelRatio||1,2);canvas.width=Math.max(1,Math.round(rect.width*ratio));canvas.height=Math.max(1,Math.round(rect.height*ratio));const ctx=canvas.getContext('2d');ctx.scale(ratio,ratio);return [ctx,rect.width,rect.height];}
function drawMap(){
  const [ctx,w,h]=context('map');ctx.fillStyle='#e5ecdf';ctx.fillRect(0,0,w,h);
  ctx.strokeStyle='#ccdac8';ctx.lineWidth=.5;
  for(let x=0;x<w;x+=45){ctx.beginPath();ctx.moveTo(x,0);ctx.lineTo(x,h);ctx.stroke();}for(let y=0;y<h;y+=45){ctx.beginPath();ctx.moveTo(0,y);ctx.lineTo(w,y);ctx.stroke();}
  if(!result?.search)return;const b=result.search.bounds;const pad=35;
  const xy=(lat,lon)=>[pad+(lon-b.west)/(b.east-b.west)*(w-2*pad),h-pad-(lat-b.south)/(b.north-b.south)*(h-2*pad)];
  const max=result.heatmap[0]?.score||1;
  const cells=[...result.heatmap].sort((a,b)=>b.cell_size_m-a.cell_size_m);
  for(const c of cells){const [x,y]=xy(c.lat,c.lon);const v=Math.exp(Math.min(0,(c.score-max)/4));const size=c.cell_size_m/111320/(b.north-b.south)*(h-2*pad);ctx.fillStyle=`rgba(${Math.round(36+180*v)},${Math.round(105+65*v)},${Math.round(85-35*v)},${.12+.57*v})`;ctx.fillRect(x-size/2,y-size/2,Math.max(size,3),Math.max(size,3));}
  const chosen=result.candidates[selected];const ids=new Set(chosen?.matches.map(m=>m.flight_id)||[]);
  ctx.save();ctx.beginPath();ctx.rect(pad,pad,w-pad*2,h-pad*2);ctx.clip();
  for(const f of result.flights){ctx.beginPath();f.points.forEach((p,i)=>{const [x,y]=xy(p.lat,p.lon);if(i===0)ctx.moveTo(x,y);else ctx.lineTo(x,y);});ctx.strokeStyle=ids.has(f.id)?'#2c706e':'#8ba795';ctx.lineWidth=ids.has(f.id)?1.8:1;ctx.setLineDash(ids.has(f.id)?[]:[4,5]);ctx.stroke();}ctx.restore();ctx.setLineDash([]);
  result.candidates.slice(0,5).forEach((c,i)=>{const [x,y]=xy(c.lat,c.lon);ctx.beginPath();ctx.arc(x,y,i===selected?12:9,0,Math.PI*2);ctx.fillStyle=i===selected?'#153f3e':'#fbfcf7';ctx.fill();ctx.strokeStyle='#346b59';ctx.lineWidth=1.5;ctx.stroke();ctx.fillStyle=i===selected?'#fff':'#346b59';ctx.textAlign='center';ctx.textBaseline='middle';ctx.font='10px monospace';ctx.fillText(String(i+1),x,y);});
  ctx.fillStyle='#53725f';ctx.font='9px monospace';ctx.textBaseline='alphabetic';ctx.textAlign='left';ctx.fillText(`${b.north.toFixed(3)}°N`,8,35);ctx.fillText(`${b.south.toFixed(3)}°N`,8,h-45);ctx.textAlign='right';ctx.fillText(`${b.east.toFixed(3)}°`,w-10,h-8);ctx.fillText(`${b.west.toFixed(3)}°`,w-10,30);
  const span=(b.east-b.west)*111320*Math.cos((b.north+b.south)/2*Math.PI/180);const scale=1000/span*(w-pad*2);if(scale<w/2){ctx.strokeStyle='#355f50';ctx.beginPath();ctx.moveTo(w-25-scale,h-50);ctx.lineTo(w-25,h-50);ctx.stroke();ctx.fillText('1 km',w-25,h-57);}
}
function drawSpectrum(){
  const [ctx,w,h]=context('spectrum');ctx.fillStyle='#102a30';ctx.fillRect(0,0,w,h);if(!result)return;
  const s=result.spectrogram;const width=s.times_s.length;const bins=s.bins;const off=document.createElement('canvas');off.width=width;off.height=bins;const oc=off.getContext('2d');const image=oc.createImageData(width,bins);
  for(let t=0;t<width;t++)for(let k=0;k<bins;k++){const v=Math.max(0,Math.min(1,(s.power_db[t*bins+k]+3)/42));const i=((bins-1-k)*width+t)*4;image.data[i]=Math.round(12+v*v*235);image.data[i+1]=Math.round(36+v*175);image.data[i+2]=Math.round(44+Math.sin(v*Math.PI)*90);image.data[i+3]=255;}oc.putImageData(image,0,0);
  const pad={l:45,r:12,t:15,b:28};ctx.imageSmoothingEnabled=false;ctx.drawImage(off,pad.l,pad.t,w-pad.l-pad.r,h-pad.t-pad.b);
  const minT=s.times_s[0]||0,maxT=s.times_s.at(-1)||1,maxF=s.min_hz+s.bin_hz*bins;
  const xy=p=>[pad.l+(p.time_s-minT)/(maxT-minT)*(w-pad.l-pad.r),h-pad.b-(p.frequency_hz-s.min_hz)/(maxF-s.min_hz)*(h-pad.t-pad.b)];
  function line(points,color,dashed=false){ctx.beginPath();points.forEach((p,i)=>{const [x,y]=xy(p);i?ctx.lineTo(x,y):ctx.moveTo(x,y);});ctx.strokeStyle=color;ctx.lineWidth=1.5;ctx.setLineDash(dashed?[5,3]:[]);ctx.stroke();ctx.setLineDash([]);}
  for(const e of result.events)line(e.points,'#f1c454');for(const m of [...(result.candidates[selected]?.matches||[]),...(result.candidates[selected]?.spectral_matches||[])])line(m.predicted,'#8ce8d4',true);
  ctx.font='9px monospace';ctx.fillStyle='#a1b7b4';ctx.textAlign='right';for(const f of [s.min_hz,Math.round((maxF+s.min_hz)/2),Math.round(maxF)]){const [,y]=xy({time_s:0,frequency_hz:f});ctx.fillText(`${Math.round(f)}`,38,y+3);}ctx.textAlign='left';for(let i=0;i<=4;i++){const t=minT+(maxT-minT)*i/4;const [x]=xy({time_s:t,frequency_hz:0});ctx.fillText(`${t.toFixed(0)}s`,x-3,h-9);}
}
function drawAll(){drawMap();drawSpectrum();}
new ResizeObserver(()=>requestAnimationFrame(drawAll)).observe($('map'));
function download(name,data,type){const a=document.createElement('a');const url=URL.createObjectURL(new Blob([JSON.stringify(data,null,2)],{type}));a.href=url;a.download=name;a.click();setTimeout(()=>URL.revokeObjectURL(url),5000);}
$('export-json').onclick=()=>download('echolocate-result.json',{ok:true,analysis:result},'application/json');
$('export-geo').onclick=()=>download('echolocate-regions.geojson',{type:'FeatureCollection',features:result.candidates.map((c,i)=>({type:'Feature',geometry:{type:'Point',coordinates:[c.lon,c.lat]},properties:{rank:i+1,score:c.score,score_kind:result.score_kind,start_unix_s:c.start_unix_s,matched_events:c.matched_events,synthetic:result.synthetic,coordinate_frame:result.coordinate_frame,clock_domain:result.clock_domain}}))},'application/geo+json');
drawAll();
// Reconnect to a job this browser previously started. No automatic uploads or new work.
const saved=localStorage.getItem('echo-native-job');if(saved){job=saved;$('execution').value='native';setBusy(true);pollJob().catch(e=>{localStorage.removeItem('echo-native-job');fail(e.message);});}

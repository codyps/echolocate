const {chromium}=require('playwright');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
async function main(){
  const root=path.resolve(__dirname,'..');
  const browser=await chromium.launch({headless:true,...(process.env.ECHO_BROWSER_PATH?{executablePath:process.env.ECHO_BROWSER_PATH}:{}),args:process.env.ECHO_BROWSER_ARGS?.split(' ')||[]});
  try{
    const page=await browser.newPage({viewport:{width:1440,height:1100}});const errors=[];const requests=[];page.on('pageerror',e=>errors.push(String(e)));page.on('request',r=>requests.push(r.url()));
    const base=process.env.ECHO_BASE_URL||'http://127.0.0.1:8765';
    await page.goto(base);assert.equal(await page.locator('#tracking').inputValue(),'coherent');
    await page.waitForFunction(()=>document.querySelector('#catalog-package').options.length===7);
    await page.locator('#load-data').click();
    await page.waitForFunction(()=>document.querySelector('#package-name').textContent.includes('153 flights'));
    assert.match(await page.locator('#data-attribution').textContent(),/ODbL/);
    // Repeat catalog reconstruction behind a project-style Pages prefix.
    await page.route('**/echo-project/**',route=>{const u=new URL(route.request().url());u.pathname=u.pathname.replace('/echo-project/','/');return route.continue({url:u.href});});
    await page.goto(base+'/echo-project/');
    await page.waitForFunction(()=>document.querySelector('#catalog-package').options.length===7);
    await page.locator('#load-data').click();
    await page.waitForFunction(()=>document.querySelector('#package-name').textContent.includes('153 flights'));
    await page.goto(base);await page.locator('#demo').click();await page.waitForFunction(()=>!document.querySelector('#analyze').disabled);
    await page.locator('#analyze').click();await page.waitForFunction(()=>document.querySelector('#activity').textContent.startsWith('Finished'),{},{timeout:90000});
    assert.equal(await page.locator('tbody tr').count(),10);assert.match(await page.locator('#status-badge').textContent(),/SYNTHETIC/);
    const download=page.waitForEvent('download');await page.locator('#export-json').click();const d=await download;const downloadPath=await d.path();const exported=JSON.parse(fs.readFileSync(downloadPath,'utf8'));assert.equal(exported.ok,true);assert.equal(await page.locator('#data-attribution').textContent(),'');assert.equal(exported.analysis.candidates[0].matched_events,2);
    assert.equal(exported.analysis.input_audio_sha256.length,64);
    await page.locator('tbody tr').nth(1).click();assert.ok(await page.locator('tbody tr').nth(1).evaluate(e=>e.classList.contains('selected')));
    await page.screenshot({path:path.join(root,'docs/preview.png'),fullPage:true});
    await page.setViewportSize({width:390,height:844});assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);
    await page.locator('#analyze').click();await page.locator('#cancel').click();assert.match(await page.locator('#activity').textContent(),/cancelled/i);
    // Verify normal audio without a package abstains through the actual browser ABI.
    await page.reload();await page.locator('#audio-file').setInputFiles(path.join(root,'web/demo.wav'));await page.locator('#analyze').click();await page.waitForFunction(()=>document.querySelector('#activity').textContent.startsWith('Finished'),{},{timeout:90000});assert.equal(await page.locator('#status-badge').textContent(),'NEEDS FLIGHT DATA');assert.equal(await page.locator('tbody tr').count(),0);
    const researchDownload=page.waitForEvent('download');await page.locator('#export-json').click();const research=await researchDownload;const researchOutput=JSON.parse(fs.readFileSync(await research.path(),'utf8'));assert.equal(researchOutput.analysis.config.coherent_ridges,true);assert.equal(researchOutput.analysis.config.robust_shape,true);assert.equal(researchOutput.analysis.score_kind,'profiled_robust_doppler_vs_linear_chirp_not_probability');
    assert.deepEqual(errors,[]);assert.ok(requests.every(url=>url.startsWith(base)||url.startsWith('blob:')),'Unexpected remote request');
    console.log('PASS: real Chromium Wasm analysis, export, selection, responsive layout, cancellation, no-package abstention, verified hosted flight catalog at root and Pages subpath, no external network');
  }finally{await browser.close();}
}
main().catch(e=>{console.error(e);process.exitCode=1;});

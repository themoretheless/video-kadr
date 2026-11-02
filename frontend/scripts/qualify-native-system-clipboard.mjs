import { chromium } from 'playwright';
import assert from 'node:assert/strict';
import { readFile,writeFile,unlink,mkdtemp,rm } from 'node:fs/promises';
import { spawn } from 'node:child_process';
import { createServer } from 'node:net';
import { randomUUID } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
const live=process.argv.includes('--live');
const root=fileURLToPath(new URL('../',import.meta.url));
const fixture=JSON.parse(await readFile(new URL('../../backend/tests/fixtures/concat-merge-preview.json',import.meta.url),'utf8'));
const source=fixture.nativeCaptionPastes[0].reference,saved=fixture.nativeCaptionPastes[1].clipboard;
const escaped=value=>JSON.stringify(value).replaceAll('<','\u003c');
const name=`.clipboard-qualification-${randomUUID()}.html`,path=new URL(`../${name}`,import.meta.url);
await writeFile(path,`<!doctype html><html><body><main id="app" data-clipboard-qualification="${name}"></main><script type="module">
import { mount } from 'svelte';
import Panel from '/src/lib/components/composition/NestedClipTransfer.svelte';
import { nestedComposition,nestedClipboard,nestedHistory } from '/src/lib/state/nestedComposition.svelte.ts';
const source=${escaped(source)},saved=${escaped(saved)};
nestedComposition.document=source;nestedComposition.activeCompositionId='new-timeline';nestedClipboard.value=saved;
window.clipboardQualification={nestedComposition,nestedClipboard,nestedHistory,expected:JSON.stringify(saved),original:JSON.stringify(source)};
mount(Panel,{target:document.querySelector('#app')});
</script></body></html>`,{flag:'wx'});
const reservation=createServer();await new Promise(resolve=>reservation.listen(0,'127.0.0.1',resolve));
const port=reservation.address().port;await new Promise(resolve=>reservation.close(resolve));
const url=`http://127.0.0.1:${port}/${name}`;
const server=spawn(process.execPath,[fileURLToPath(new URL('../node_modules/vite/bin/vite.js',import.meta.url)),'--host','127.0.0.1','--port',String(port),'--strictPort'],{cwd:root,stdio:'ignore'});
let browser,backend,storage,backendUrl,backendExit,backendSpawnError,backendLogs='';
try {
  let ready=false;
  for(let attempt=0;attempt<100;attempt++){
    if(server.exitCode!==null)throw new Error('Qualification server failed to start');
    try {const response=await fetch(url);if(response.ok && (await response.text()).includes(`data-clipboard-qualification="${name}"`)){ready=true;break}}catch{}
    await new Promise(resolve=>setTimeout(resolve,100));
  }
  if(!ready)throw new Error('Qualification server readiness deadline exceeded');
  if(live){
    const reservation=createServer();await new Promise(resolve=>reservation.listen(0,'127.0.0.1',resolve));
    const backendPort=reservation.address().port;await new Promise(resolve=>reservation.close(resolve));
    backendUrl=`http://127.0.0.1:${backendPort}`;storage=await mkdtemp(join(tmpdir(),'video-kadr-native-clipboard-'));
    const backendEnv={...process.env,STORAGE_DIR:storage,PORT:String(backendPort),BIND_ADDR:'127.0.0.1',CORS_ALLOW_ORIGINS:`http://127.0.0.1:${port}`};delete backendEnv.VIDEO_KADR_MCP_TOKEN;delete backendEnv.OBJECT_STORE_URL;
    backend=spawn(process.env.NATIVE_CLIPBOARD_BACKEND_BINARY ?? fileURLToPath(new URL('../../backend/target/debug/video-kadr-backend',import.meta.url)),[],{cwd:root,env:backendEnv,stdio:['ignore','pipe','pipe']});
    backend.stdout.on('data',data=>{backendLogs=(backendLogs+data).slice(-8000)});backend.stderr.on('data',data=>{backendLogs=(backendLogs+data).slice(-8000)});
    backendExit=new Promise(resolve=>{backend.once('exit',resolve);backend.once('error',error=>{backendSpawnError=error;resolve()})});
    let ready=false;
    for(let attempt=0;attempt<100;attempt++){
      if(backendSpawnError || backend.exitCode!==null)throw new Error(`Native backend failed to start: ${backendSpawnError?.message ?? backendLogs}`);
      try{if((await fetch(`${backendUrl}/api/health`)).ok){ready=true;break}}catch{}
      await new Promise(resolve=>setTimeout(resolve,100));
    }
    if(!ready)throw new Error('Native backend readiness deadline exceeded');
  }
  browser=await chromium.launch({headless:true,...(process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH?{executablePath:process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH}:{})});
const context=await browser.newContext({permissions:['clipboard-read','clipboard-write']});
let previous;
const first=await context.newPage();
const errors=[];let validations=0;
context.on('page',page=>page.on('pageerror',error=>errors.push(error.message)));
first.on('pageerror',error=>errors.push(error.message));
await context.route('**/api/compositions/nested-frame',async route=>{
  const payload=route.request().postDataJSON();
  assert.ok(['clipboard','new-timeline'].includes(payload.project.root));validations++;
  if(live){const response=await route.fetch({url:`${backendUrl}/api/compositions/nested-frame`});await route.fulfill({response})}
  else await route.fulfill({contentType:'application/json',body:'{"entries":[]}'});
});
try {
  await first.goto(url);await first.getByRole('button',{name:'Передать буфер другому окну',exact:true}).waitFor();
  previous=await first.evaluate(()=>navigator.clipboard.readText());
  await first.getByRole('button',{name:'Передать буфер другому окну',exact:true}).click();
  await first.getByText('Клипы скопированы для другого окна Video Kadr.',{exact:true}).waitFor();
  const second=await context.newPage();await second.goto(url);
  await second.getByRole('button',{name:'Прочитать системный буфер',exact:true}).waitFor();
  await second.evaluate(()=>{window.clipboardQualification.nestedClipboard.value=null});
  await second.getByRole('button',{name:'Прочитать системный буфер',exact:true}).click();
  await second.getByText('Клипы получены. Выберите место и вставьте их из буфера.',{exact:true}).waitFor();
  const result=await second.evaluate(()=>{const q=window.clipboardQualification;return{same:JSON.stringify(q.nestedClipboard.value)===q.expected,unchanged:JSON.stringify(q.nestedComposition.document)===q.original,history:q.nestedHistory.past.length}});
  assert.deepEqual(result,{same:true,unchanged:true,history:0});assert.equal(validations,1);
  assert.deepEqual(await second.evaluate(()=>JSON.parse(JSON.stringify(window.clipboardQualification.nestedClipboard.value))),saved);
  await second.evaluate(()=>navigator.clipboard.writeText('unrelated clipboard text'));
  await second.getByRole('button',{name:'Прочитать системный буфер',exact:true}).click();
  await second.waitForFunction(()=>window.clipboardQualification.nestedHistory.error.includes('Video Kadr'));
  assert.equal(await second.evaluate(()=>JSON.stringify(window.clipboardQualification.nestedClipboard.value)===window.clipboardQualification.expected),true);
  assert.equal(validations,1);
  if(live){
    const previousError=await second.evaluate(()=>window.clipboardQualification.nestedHistory.error);
    await second.evaluate(()=>{const q=window.clipboardQualification,value=JSON.parse(JSON.stringify(q.nestedClipboard.value));value.compositions.clipboard.content.captionTracks[0].style.size=0;return navigator.clipboard.writeText(JSON.stringify({format:'video-kadr/clip-selection',version:1,project:value}))});
    const rejected=second.waitForResponse(response=>response.url().endsWith('/api/compositions/nested-frame') && response.status()===400);
    await second.getByRole('button',{name:'Прочитать системный буфер',exact:true}).click();await rejected;
    await second.waitForFunction(previous=>window.clipboardQualification.nestedHistory.error!==previous,previousError);
    assert.deepEqual(await second.evaluate(()=>JSON.parse(JSON.stringify(window.clipboardQualification.nestedClipboard.value))),saved);
    assert.equal(validations,2);
    await second.getByLabel('Начало переноса клипов',{exact:true}).fill('8');
    await second.getByRole('button',{name:'Вставить из буфера',exact:true}).click();
    await second.waitForFunction(()=>window.clipboardQualification.nestedHistory.past.length===1);
    const pasted=await second.evaluate(()=>JSON.parse(JSON.stringify(window.clipboardQualification.nestedComposition.document)));
    const cue=pasted.compositions['new-timeline'].content.captionTracks[1].clips[0];
    assert.equal(cue.timelineStartTick,String(8000000n*254016n+1n));assert.equal(cue.text,saved.compositions.clipboard.content.captionTracks[0].clips[0].text);
    assert.equal(validations,3);
    const registration=await fetch(`${backendUrl}/api/auth/register`,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({username:`clipboard-${randomUUID().slice(0,8)}`,password:randomUUID()})});
    assert.equal(registration.status,201);const token=(await registration.json()).token;assert.ok(token);
    const frame=async(project,compositionId,timeUs)=>{
      const response=await fetch(`${backendUrl}/api/compositions/nested-preview`,{method:'POST',headers:{'Content-Type':'application/json',Authorization:`Bearer ${token}`},body:JSON.stringify({project,compositionId,timeUs,renderedOutputs:{}})});
      assert.equal(response.status,200);const bytes=Buffer.from(await response.arrayBuffer());assert.equal(bytes.length,16*16*4);return bytes;
    };
    const original=await frame(saved,'clipboard',2250000),copy=await frame(pasted,'new-timeline',8250000);
    assert.deepEqual(copy,original);assert.ok(copy.some((value,index)=>index%4!==3 && value>0));
    await second.locator('[data-native-clipboard]').focus();await second.keyboard.press('Control+z');
    await second.waitForFunction(()=>window.clipboardQualification.nestedHistory.past.length===0);
    assert.deepEqual(await second.evaluate(()=>JSON.parse(JSON.stringify(window.clipboardQualification.nestedComposition.document))),source);
    assert.deepEqual(await second.evaluate(()=>JSON.parse(JSON.stringify(window.clipboardQualification.nestedClipboard.value))),saved);
    await second.locator('[data-native-clipboard]').focus();await second.keyboard.press('Control+Shift+z');
    await second.waitForFunction(()=>window.clipboardQualification.nestedHistory.past.length===1);
    assert.deepEqual(await second.evaluate(()=>JSON.parse(JSON.stringify(window.clipboardQualification.nestedComposition.document))),pasted);
  }
  assert.deepEqual(errors,[]);
  console.log(live?'PASS: real Chromium system clipboard + native Rust HTTP validation/rejection, paste/one Undo/Redo, exact fractional caption clock and visible pixel equality; no HTTP mocks':
'PASS: actual Chromium clipboard between two pages, exact caption payload, native-validation contract, no authoring, unrelated text refusal; HTTP validation mocked');
} finally {
  try {if(previous!==undefined)await first.evaluate(value=>navigator.clipboard.writeText(value),previous)}finally{await browser.close()}
}

} finally {
  try {if(browser?.isConnected())await browser.close()} finally {
    if(server.exitCode===null){server.kill('SIGTERM');await new Promise(resolve=>server.once('exit',resolve))}
    if(backend && !backendSpawnError && backend.exitCode===null){backend.kill('SIGTERM');await backendExit}
    if(storage)await rm(storage,{recursive:true,force:true});
    await unlink(path);
  }
}

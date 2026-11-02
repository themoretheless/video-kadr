import {execFileSync} from 'node:child_process'
import {expect,test} from '@playwright/test'
if(process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE)test.use({launchOptions:{executablePath:process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE}})
test('private PNG masks preserve edits through save reload preview and lossless export',async({page,request})=>{
 const registered=await request.post('/api/auth/register',{data:{username:'mask-browser-user',password:'scope-browser-test-password'}});expect(registered.status()).toBe(201)
 const {token}=await registered.json() as {token:string}
 const png=execFileSync('ffmpeg',['-v','error','-f','lavfi','-i','color=blue:s=16x16','-frames:v','1','-threads','1','-f','image2pipe','-c:v','png','pipe:1'])
 const uploaded=await request.post('/api/upload',{headers:{Authorization:`Bearer ${token}`},multipart:{file:{name:'mask-fixture.png',mimeType:'image/png',buffer:png}}});expect(uploaded.ok()).toBe(true)
 const {id}=await uploaded.json() as {id:string}
 const constant=(value:number)=>({mode:'constant',value})
 const content={schemaVersion:1,timeBase:1000000,canvas:{width:16,height:16,fpsMilli:10000,background:{red:0,green:0,blue:0,alpha:1}},sources:{[id]:{id,kind:'image',durationTicks:0,width:16,height:16,hasAudio:false}},tracks:[{kind:'image',id:'images',name:'Images',hidden:false,locked:false,clips:[{id:'photo',sourceId:id,timelineStartTick:1000000,durationTicks:1000000,transform:{x:constant(0),y:constant(0),scaleX:constant(1),scaleY:constant(1),rotationDegrees:constant(0),anchorX:0.5,anchorY:0.5},opacity:constant(1),blendMode:'normal',effects:[{kind:'gamma_correction',parameters:{gamma:10}}],enabled:true}]}]}
 await page.addInitScript(props=>{if(!localStorage.getItem('video-kadr.live-mask-test'))localStorage.setItem('video-kadr.live-mask-test',JSON.stringify(props))},{token,project:{schemaVersion:1,root:'root',compositions:{root:{durationUs:2000000,content,instances:[]}}}})
 const errors:string[]=[];page.on('pageerror',error=>errors.push(error.message))
 await page.goto('/e2e/fixtures/live-mask.html')
 await page.getByRole('button',{name:'Прямоугольник',exact:true}).click()
 const history=page.getByLabel('Mask history');await expect(history).toHaveText('1')
 for(const [channel,value,count] of [['x','0.25','2'],['width','0.5','3']] as const){const input=page.getByLabel(`photo mask 1 ${channel}`,{exact:true});await input.fill(value);await input.dispatchEvent('change');await expect(history).toHaveText(count)}
 await page.getByLabel('photo mask 1 width key',{exact:true}).click();await expect(history).toHaveText('4')
 const document=JSON.parse(await page.getByLabel('Mask document').innerText())
 expect(document.compositions.root.content.tracks[0].clips[0].effects[1].width.track.keyframes).toEqual([{tick:500000,value:0.5}])

 await page.getByText('Ширина · 1 ключей',{exact:true}).click()
 const interpolation=page.getByLabel('photo mask 1 width interpolation',{exact:true})
 await interpolation.selectOption('bezier');await expect(history).toHaveText('5')
 const point=page.getByLabel('photo mask 1 width p1 0',{exact:true})
 await point.fill('0.4');await point.dispatchEvent('change');await expect(history).toHaveText('6')
 let curve=JSON.parse(await page.getByLabel('Mask document').innerText()).compositions.root.content.tracks[0].clips[0].effects[1].width.track.interpolation
 expect(curve).toEqual({kind:'bezier',p1:[0.4,0.1],p2:[0.25,1]})
 await interpolation.selectOption('elastic');await expect(history).toHaveText('7')
 const period=page.getByLabel('photo mask 1 width period',{exact:true})
 await period.fill('0.5');await period.dispatchEvent('change');await expect(history).toHaveText('8')
 curve=JSON.parse(await page.getByLabel('Mask document').innerText()).compositions.root.content.tracks[0].clips[0].effects[1].width.track.interpolation
 expect(curve).toEqual({kind:'elastic',dir:'out',amplitude:1,period:0.5})
 for(const count of ['7','6','5','4']){await page.getByRole('button',{name:'Undo',exact:true}).click();await expect(history).toHaveText(count)}
 await expect(interpolation).toHaveValue('linear')
 const segment=page.getByLabel('photo mask 1 width 500000 interpolation',{exact:true})
 await segment.selectOption('bezier');await expect(history).toHaveText('5')
 const segmentPoint=page.getByLabel('photo mask 1 width 500000 curve p2 1',{exact:true})
 await segmentPoint.fill('0.8');await segmentPoint.dispatchEvent('change');await expect(history).toHaveText('6')
 let segmentTrack=JSON.parse(await page.getByLabel('Mask document').innerText()).compositions.root.content.tracks[0].clips[0].effects[1].width.track
 expect(segmentTrack.interpolation).toBe('linear')
 expect(segmentTrack.keyframes[0].interpolation).toEqual({kind:'bezier',p1:[0.25,0.1],p2:[0.25,0.8]})
 await segment.selectOption('elastic');await expect(history).toHaveText('7')
 const segmentPeriod=page.getByLabel('photo mask 1 width 500000 curve period',{exact:true})
 await segmentPeriod.fill('0.5');await segmentPeriod.dispatchEvent('change');await expect(history).toHaveText('8')
 segmentTrack=JSON.parse(await page.getByLabel('Mask document').innerText()).compositions.root.content.tracks[0].clips[0].effects[1].width.track
 expect(segmentTrack.interpolation).toBe('linear')
 expect(segmentTrack.keyframes[0].interpolation).toEqual({kind:'elastic',dir:'out',amplitude:1,period:0.5})
 for(const count of ['7','6','5','4']){await page.getByRole('button',{name:'Undo',exact:true}).click();await expect(history).toHaveText(count)}
 await expect(segment).toHaveValue('inherit')
 expect(JSON.parse(await page.getByLabel('Mask document').innerText())).toEqual(document)
 await page.getByLabel('photo mask 1 width 500000 tangents add',{exact:true}).click();await expect(history).toHaveText('5')
 const withTangents=JSON.parse(await page.getByLabel('Mask document').innerText())
 const outgoing=page.getByLabel('photo mask 1 width 500000 outgoing x',{exact:true})
 await outgoing.fill('0.4');await outgoing.dispatchEvent('change');await expect(history).toHaveText('6')
 const freeTangents=JSON.parse(await page.getByLabel('Mask document').innerText())
 expect(freeTangents.compositions.root.content.tracks[0].clips[0].effects[1].width.track.keyframes[0].tangents.outgoing).toMatchObject({mode:'free',x:0.4})
 await page.getByLabel('photo mask 1 width 500000 outgoing mode',{exact:true}).selectOption('auto');await expect(history).toHaveText('7')
 const automatic=JSON.parse(await page.getByLabel('Mask document').innerText()).compositions.root.content.tracks[0].clips[0].effects[1].width.track.keyframes[0].tangents.outgoing
 expect(automatic).toEqual({...freeTangents.compositions.root.content.tracks[0].clips[0].effects[1].width.track.keyframes[0].tangents.outgoing,mode:'auto'})
 await page.getByRole('button',{name:'Undo',exact:true}).click();await expect(history).toHaveText('6')
 expect(JSON.parse(await page.getByLabel('Mask document').innerText())).toEqual(freeTangents)
 await page.getByRole('button',{name:'Undo',exact:true}).click();await expect(history).toHaveText('5')
 expect(JSON.parse(await page.getByLabel('Mask document').innerText())).toEqual(withTangents)
 await page.getByRole('button',{name:'Undo',exact:true}).click();await expect(history).toHaveText('4')
 expect(JSON.parse(await page.getByLabel('Mask document').innerText())).toEqual(document)
 await expect(page.getByLabel('Mask error')).toHaveText('')
 const keyTick=page.getByLabel('photo mask 1 width 500000 time',{exact:true})
 await keyTick.fill('0');await keyTick.dispatchEvent('change');await expect(history).toHaveText('5')
 await page.getByLabel('photo mask 1 width key',{exact:true}).click();await expect(history).toHaveText('6')
 const secondValue=page.getByLabel('photo mask 1 width 500000 value',{exact:true})
 await secondValue.fill('0.75');await secondValue.dispatchEvent('change');await expect(history).toHaveText('7')
 await interpolation.selectOption('bezier');await expect(history).toHaveText('8')
 await page.getByLabel('photo mask 1 width 0 tangents add',{exact:true}).click();await expect(history).toHaveText('9')
 await page.getByLabel('photo mask 1 width 0 outgoing mode',{exact:true}).selectOption('auto');await expect(history).toHaveText('10')
 await page.getByLabel('photo mask 1 width 500000 tangents add',{exact:true}).click();await expect(history).toHaveText('11')
 await page.getByLabel('photo mask 1 width 500000 incoming mode',{exact:true}).selectOption('auto');await expect(history).toHaveText('12')
 const resolved=JSON.parse(await page.getByLabel('Mask document').innerText()).compositions.root.content.tracks[0].clips[0].effects[1].width.track.keyframes
 expect(resolved[0].tangents.outgoing).toEqual({x:1/3,y:0,mode:'auto'})
 expect(resolved[1].tangents.incoming).toEqual({x:2/3,y:1,mode:'auto'})
 await expect(page.getByLabel('Mask error')).toHaveText('')
 for(const count of ['11','10','9','8','7','6','5','4']){await page.getByRole('button',{name:'Undo',exact:true}).click();await expect(history).toHaveText(count)}
 expect(JSON.parse(await page.getByLabel('Mask document').innerText())).toEqual(document)
 await page.getByText('Images · photo · порядок эффектов',{exact:true}).click()
 await page.getByLabel('photo effect 1 up',{exact:true}).click();await expect(history).toHaveText('5')
 await page.getByRole('button',{name:'Undo',exact:true}).click();await expect(history).toHaveText('4')
 await page.getByRole('button',{name:'Redo',exact:true}).click();await expect(history).toHaveText('5')
 const response=page.waitForResponse(response=>new URL(response.url()).pathname==='/api/compositions/nested-preview')
 await page.getByRole('button',{name:'Native Preview',exact:true}).click();expect((await response).status()).toBe(200)
 await expect(page.getByLabel('Mask preview error')).toHaveText('')
 await expect.poll(()=>page.getByLabel('Mask preview',{exact:true}).evaluate(element=>{
  const canvas=element as HTMLCanvasElement;if(canvas.width!==16||canvas.height!==16)return false
  const pixels=canvas.getContext('2d')!.getImageData(0,0,16,16).data
  for(let i=0;i<256;i++){const expected=i%16<8?[0,0,254,255]:[0,0,0,255];for(let c=0;c<4;c++)if(Math.abs(pixels[i*4+c]!-expected[c]!)>1)return false}return true
 })).toBe(true)

 const edited=JSON.parse(await page.getByLabel('Mask document').innerText())
 const headers={Authorization:`Bearer ${token}`}
 const spaceResponse=await request.post('/api/spaces',{headers,data:{name:'Private image effects'}});expect(spaceResponse.status()).toBe(201)
 const space=await spaceResponse.json()
 const savedResponse=await request.post('/api/composition-projects',{headers,data:{schemaVersion:2,mode:'composition',spaceId:space.id,name:'Image masks',document:edited}});expect(savedResponse.status()).toBe(201)
 const saved=await savedResponse.json()
 const loadedResponse=await request.get(`/api/composition-projects/${saved.id}`,{headers});expect(loadedResponse.status()).toBe(200)
 const loaded=await loadedResponse.json();expect(loaded.document).toEqual(edited)
 const outsiderResponse=await request.post('/api/auth/register',{data:{username:'mask-browser-outsider',password:'scope-browser-test-password'}});expect(outsiderResponse.status()).toBe(201)
 const outsider=await outsiderResponse.json()
 expect((await request.get(`/api/composition-projects/${saved.id}`,{headers:{Authorization:`Bearer ${outsider.token}`}})).status()).toBe(404)
 const beforePixels=await page.getByLabel('Mask preview',{exact:true}).evaluate(element=>Array.from((element as HTMLCanvasElement).getContext('2d')!.getImageData(0,0,16,16).data))
 // Keep reloaded fixture data deterministic without competing init scripts.
 await page.evaluate(props=>localStorage.setItem('video-kadr.live-mask-test',JSON.stringify(props)),{token,project:loaded.document})
 await page.reload()
 await expect(page.getByLabel('Mask history')).toHaveText('0')
 expect(JSON.parse(await page.getByLabel('Mask document').innerText())).toEqual(edited)
 const restoredPreview=page.waitForResponse(response=>new URL(response.url()).pathname==='/api/compositions/nested-preview')
 await page.getByRole('button',{name:'Native Preview',exact:true}).click();expect((await restoredPreview).status()).toBe(200)
 await expect.poll(()=>page.getByLabel('Mask preview',{exact:true}).evaluate(element=>Array.from((element as HTMLCanvasElement).getContext('2d')!.getImageData(0,0,16,16).data))).toEqual(beforePixels)

 const exportBody={schemaVersion:1,mode:'video_only',project:loaded.document,compositionId:'root',startUs:1500000,endUs:2000000}
 expect((await request.post('/api/compositions/nested-export',{headers:{Authorization:`Bearer ${outsider.token}`},data:exportBody})).status()).toBe(403)
 const exported=await request.post('/api/compositions/nested-export',{headers,data:exportBody});expect(exported.status()).toBe(202)
 const {jobId}=await exported.json()
 let terminal:{status:string;result?:{url:string;videoCodec:string};error?:string}|undefined
 await expect.poll(async()=>{
  const response=await request.get(`/api/jobs/${jobId}`,{headers});expect(response.status()).toBe(200)
  terminal=await response.json();return terminal!.status
 },{timeout:30000}).toBe('done')
 expect(terminal!.result!.videoCodec).toBe('ffv1')
 const movieResponse=await request.get(terminal!.result!.url,{headers});expect(movieResponse.status()).toBe(200)
 const decoded=execFileSync('ffmpeg',['-v','error','-i','pipe:0','-map','0:v:0','-f','rawvideo','-pix_fmt','rgba','pipe:1'],{input:await movieResponse.body()})
 expect(decoded.length).toBe(16*16*4*5)
 for(let offset=0;offset<decoded.length;offset+=1024)expect(Array.from(decoded.subarray(offset,offset+1024))).toEqual(beforePixels)
 const repeated=await request.post('/api/compositions/nested-export',{headers,data:exportBody});expect(repeated.status()).toBe(202)
 expect((await repeated.json()).jobId).toBe(jobId)

 for(const kind of ['bezier','elastic','tangents','auto_tangents'] as const){
  const animated=JSON.parse(JSON.stringify(loaded.document))
  const width=animated.compositions.root.content.tracks[0].clips[0].effects[0].width
  width.track.keyframes=[{tick:0,value:0.25},{tick:1000000,value:0.75}]
  width.track.interpolation=kind==='bezier'?{kind:'bezier',p1:[1/3,0],p2:[2/3,1]}:{kind:'elastic',dir:'out',amplitude:1,period:0.5}
  if(kind==='tangents'||kind==='auto_tangents'){
   width.track.interpolation={kind:'bezier',p1:[1/3,1/3],p2:[2/3,2/3]}
   width.track.keyframes[0].tangents={continuity:'broken',incoming:{x:2/3,y:2/3,mode:'free'},outgoing:{x:1/3,y:0,mode:'free'}}
   width.track.keyframes[1].tangents={continuity:'broken',incoming:{x:2/3,y:1,mode:'free'},outgoing:{x:1/3,y:1/3,mode:'free'}}
   if(kind==='auto_tangents'){width.track.keyframes[0].tangents.outgoing={x:0.25,y:0.25,mode:'auto'};width.track.keyframes[1].tangents.incoming={x:0.75,y:0.75,mode:'auto'}}
  }
  const expectedFrame=(progress:number)=>{
   const easing=kind==='auto_tangents'?progress:kind!=='elastic'?3*progress*progress-2*progress*progress*progress:1-2**(-10*progress)*Math.cos(4*Math.PI*progress)
   const size=0.25+0.5*easing
   return Array.from({length:256},(_,i)=>Math.abs((i%16+0.5)/16-0.25)<=size/2?[0,0,255,255]:[0,0,0,255]).flat()
  }
  const preview=await request.post('/api/compositions/nested-preview',{headers,data:{project:animated,timeUs:1500000,renderedOutputs:{}}});expect(preview.status()).toBe(200)
  expect(Array.from(await preview.body())).toEqual(expectedFrame(0.5))
  const jobResponse=await request.post('/api/compositions/nested-export',{headers,data:{...exportBody,project:animated}});expect(jobResponse.status()).toBe(202)
  const job=await jobResponse.json()
  let result:{url:string}|undefined
  await expect.poll(async()=>{const response=await request.get(`/api/jobs/${job.jobId}`,{headers});const terminal=await response.json();result=terminal.result;return terminal.status},{timeout:30000}).toBe('done')
  const movie=await request.get(result!.url,{headers});expect(movie.status()).toBe(200)
  const pixels=execFileSync('ffmpeg',['-v','error','-i','pipe:0','-map','0:v:0','-f','rawvideo','-pix_fmt','rgba','pipe:1'],{input:await movie.body()})
  expect(pixels.length).toBe(5120)
  for(let frame=0;frame<5;frame++)expect(Array.from(pixels.subarray(frame*1024,(frame+1)*1024))).toEqual(expectedFrame(0.5+frame/10))
 }

 for(const mode of ['hold','continue','loop','ping_pong','offset'] as const){
  const animated=JSON.parse(JSON.stringify(loaded.document))
  const width=animated.compositions.root.content.tracks[0].clips[0].effects[0].width
  width.track={timeBase:1000000,interpolation:'linear',extrapolation:{before:'hold',after:mode},keyframes:[{tick:0,value:0.25},{tick:250000,value:0.5}]}
  const expectedFrame=(local:number)=>{
   const phase=local/250000
   const folded=mode==='ping_pong'?(Math.floor(phase)%2?1-(phase%1):phase%1):phase%1
   const size=mode==='hold'?0.5:mode==='continue'||mode==='offset'?0.25+0.25*phase:0.25+0.25*folded
   return Array.from({length:256},(_,i)=>Math.abs((i%16+0.5)/16-0.25)<=size/2?[0,0,255,255]:[0,0,0,255]).flat()
  }
  const preview=await request.post('/api/compositions/nested-preview',{headers,data:{project:animated,timeUs:1500000,renderedOutputs:{}}});expect(preview.status()).toBe(200)
  expect(Array.from(await preview.body())).toEqual(expectedFrame(500000))
  const exported=await request.post('/api/compositions/nested-export',{headers,data:{...exportBody,project:animated}});expect(exported.status()).toBe(202)
  const job=await exported.json();let result:{url:string}|undefined
  await expect.poll(async()=>{const response=await request.get(`/api/jobs/${job.jobId}`,{headers});const terminal=await response.json();result=terminal.result;return terminal.status},{timeout:30000}).toBe('done')
  const movie=await request.get(result!.url,{headers});expect(movie.status()).toBe(200)
  const pixels=execFileSync('ffmpeg',['-v','error','-i','pipe:0','-map','0:v:0','-f','rawvideo','-pix_fmt','rgba','pipe:1'],{input:await movie.body()})
  expect(pixels.length).toBe(5120)
  for(let frame=0;frame<5;frame++)expect(Array.from(pixels.subarray(frame*1024,(frame+1)*1024))).toEqual(expectedFrame(500000+frame*100000))
 }

 for(const mode of ['hold','continue','loop','ping_pong','offset'] as const){
  const animated=JSON.parse(JSON.stringify(loaded.document))
  const width=animated.compositions.root.content.tracks[0].clips[0].effects[0].width
  width.track={timeBase:1000000,interpolation:'linear',extrapolation:{before:mode,after:'hold'},keyframes:[{tick:500000,value:1},{tick:750000,value:1.25}]}
  const expectedFrame=(local:number)=>{
   const phase=(local-500000)/250000,cycle=Math.floor(phase),fraction=phase-cycle
   const folded=mode==='ping_pong'?(Math.abs(cycle)%2?1-fraction:fraction):fraction
   const size=mode==='hold'?1:mode==='continue'||mode==='offset'?1+0.25*phase:1+0.25*folded
   return Array.from({length:256},(_,i)=>Math.abs((i%16+0.5)/16-0.25)<=size/2?[0,0,255,255]:[0,0,0,255]).flat()
  }
  const preview=await request.post('/api/compositions/nested-preview',{headers,data:{project:animated,timeUs:1000000,renderedOutputs:{}}});expect(preview.status()).toBe(200)
  expect(Array.from(await preview.body())).toEqual(expectedFrame(0))
  const exported=await request.post('/api/compositions/nested-export',{headers,data:{...exportBody,project:animated,startUs:1000000,endUs:1500000}});expect(exported.status()).toBe(202)
  const job=await exported.json();let result:{url:string}|undefined
  await expect.poll(async()=>{const response=await request.get(`/api/jobs/${job.jobId}`,{headers});const terminal=await response.json();result=terminal.result;return terminal.status},{timeout:30000}).toBe('done')
  const movie=await request.get(result!.url,{headers});expect(movie.status()).toBe(200)
  const pixels=execFileSync('ffmpeg',['-v','error','-i','pipe:0','-map','0:v:0','-f','rawvideo','-pix_fmt','rgba','pipe:1'],{input:await movie.body()})
  expect(pixels.length).toBe(5120)
  for(let frame=0;frame<5;frame++)expect(Array.from(pixels.subarray(frame*1024,(frame+1)*1024))).toEqual(expectedFrame(frame*100000))
 }

 {
  const animated=JSON.parse(JSON.stringify(loaded.document))
  const width=animated.compositions.root.content.tracks[0].clips[0].effects[0].width
  width.track={timeBase:1000000,interpolation:'linear',keyframes:[{tick:0,value:0.25,interpolation:'hold'},{tick:500000,value:0.5,interpolation:'ease_in'},{tick:1000000,value:0.75}]}
  const expectedFrame=(local:number)=>{
   const size=local<500000?0.25:0.5+0.25*((local-500000)/500000)**3
   return Array.from({length:256},(_,i)=>Math.abs((i%16+0.5)/16-0.25)<=size/2?[0,0,255,255]:[0,0,0,255]).flat()
  }
  for(const local of [250000,750000]){
   const preview=await request.post('/api/compositions/nested-preview',{headers,data:{project:animated,timeUs:1000000+local,renderedOutputs:{}}});expect(preview.status()).toBe(200)
   expect(Array.from(await preview.body())).toEqual(expectedFrame(local))
  }
  const exported=await request.post('/api/compositions/nested-export',{headers,data:{...exportBody,project:animated,startUs:1000000,endUs:2000000}});expect(exported.status()).toBe(202)
  const job=await exported.json();let result:{url:string}|undefined
  await expect.poll(async()=>{const response=await request.get(`/api/jobs/${job.jobId}`,{headers});const terminal=await response.json();result=terminal.result;return terminal.status},{timeout:30000}).toBe('done')
  const movie=await request.get(result!.url,{headers});expect(movie.status()).toBe(200)
  const pixels=execFileSync('ffmpeg',['-v','error','-i','pipe:0','-map','0:v:0','-f','rawvideo','-pix_fmt','rgba','pipe:1'],{input:await movie.body()})
  expect(pixels.length).toBe(10240)
  for(let frame=0;frame<10;frame++)expect(Array.from(pixels.subarray(frame*1024,(frame+1)*1024))).toEqual(expectedFrame(frame*100000))
 }
 expect(errors).toEqual([])
 if(process.env.MASK_SCREENSHOT_PATH)await page.screenshot({path:process.env.MASK_SCREENSHOT_PATH,fullPage:true})
})

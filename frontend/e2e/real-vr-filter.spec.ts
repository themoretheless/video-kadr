import {readFile} from 'node:fs/promises'
import {spawnSync} from 'node:child_process'
import {expect,test} from '@playwright/test'
if(process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE)test.use({launchOptions:{executablePath:process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE}})
for(let profile=0;profile<24;profile++)test(`real VR Filter profile${profile} intensity stereo undo and original preview`,async({page,request})=>{
 const fixture=JSON.parse(await readFile('../backend/tests/fixtures/filmcraft-vr-filter.json','utf8'))
 const cases=fixture.cases.filter((c:any)=>c.width===18&&c.rgba[3]>0&&c.pxScale===1&&[50,500].includes(c.strength))
 expect(cases).toHaveLength(24)
 const original=cases[profile];const sharpen=original.effect==='vr_sharpen';const label=sharpen?'VR Sharpen':'VR Blur';const channel=sharpen?'amount':'blurriness';const slug=original.effect.replaceAll('_','-');const parameters={frameLayout:original.layout,[channel]:original.strength}
 const encoded=spawnSync('ffmpeg',['-v','error','-f','rawvideo','-pixel_format','rgba','-video_size',`${original.width}x${original.height}`,'-framerate','1','-i','pipe:0','-c:v','ffv1','-pix_fmt','bgra','-f','matroska','pipe:1'],{input:Buffer.from(original.rgba),maxBuffer:16*1024*1024})
 expect(encoded.error).toBeUndefined();expect(encoded.status,encoded.stderr.toString()).toBe(0)
 const registered=await request.post('/api/auth/register',{data:{username:`vr-filter-browser-${profile}`,password:'scope-browser-test-password'}});expect(registered.status()).toBe(201)
 const {token}=await registered.json() as {token:string}
 const uploaded=await request.post('/api/upload',{headers:{Authorization:`Bearer ${token}`},multipart:{file:{name:'vr-filter-fixture.mkv',mimeType:'video/x-matroska',buffer:encoded.stdout}}});expect(uploaded.ok()).toBe(true)
 const {id}=await uploaded.json() as {id:string}
 const constant=(value:number)=>({mode:'constant',value})
 const content={schemaVersion:1,timeBase:1000000,canvas:{width:original.width,height:original.height,fpsMilli:1000,background:{red:0,green:0,blue:0,alpha:1}},sources:{[id]:{id,kind:'video',durationTicks:1000000,width:original.width,height:original.height,hasAudio:false}},tracks:[{kind:'video',id:'main',name:'Image',hidden:false,locked:false,clips:[{id:'clip',sourceId:id,placement:{timelineStartTick:0,sourceInTick:0,sourceOutTick:1000000,speed:1},transform:{x:constant(0),y:constant(0),scaleX:constant(1),scaleY:constant(1),rotationDegrees:constant(0),anchorX:0.5,anchorY:0.5},opacity:constant(1),blendMode:'normal',effects:[{kind:original.effect,parameters}],enabled:true}]}]}
 const props={token,project:{schemaVersion:1,root:'root',compositions:{root:{durationUs:1000000,content,instances:[]}}}}
 await page.addInitScript(value=>localStorage.setItem('video-kadr.live-'+value.slug+'-test',JSON.stringify(value.props)),{slug,props})
 const errors:string[]=[];page.on('pageerror',error=>errors.push(error.message));await page.goto(`/e2e/fixtures/live-${slug}.html`)
 const history=page.getByLabel(`${label} history`);await expect(history).toHaveText('0')
 await page.getByLabel(`clip ${label} ${channel} ключ`,{exact:true}).click();const count=1;await expect(history).toHaveText('1');const saved=JSON.parse(await page.getByLabel(`${label} document`).innerText());const effect=saved.compositions.root.content.tracks[0].clips[0].effects[0];expect(effect.animation.track.keyframes).toEqual([{tick:500000,value:original.strength}])
 const set=async(channel:string,value:number)=>page.getByLabel(`clip ${label} ${channel}`,{exact:true}).evaluate((element,value)=>{(element as HTMLInputElement).value=String(value);element.dispatchEvent(new Event('change',{bubbles:true}))},value)
 await set(channel,42.125);await expect(history).toHaveText(String(count+1));const updated=JSON.parse(await page.getByLabel(`${label} document`).innerText()).compositions.root.content.tracks[0].clips[0].effects[0];expect(updated).toEqual({...effect,parameters:{...parameters,[channel]:42.125}});await page.getByRole('button',{name:'Undo',exact:true}).click();await expect(history).toHaveText(String(count));expect(JSON.parse(await page.getByLabel(`${label} document`).innerText())).toEqual(saved)
 const key=page.getByLabel(`clip ${label} 500000 value`,{exact:true});await key.locator('xpath=ancestor::details[1]').locator('summary').click();await key.evaluate(element=>{(element as HTMLInputElement).value='12.5';element.dispatchEvent(new Event('change',{bubbles:true}))});await expect(history).toHaveText(String(count+1));await page.getByRole('button',{name:'Undo',exact:true}).click();await expect(history).toHaveText(String(count));expect(JSON.parse(await page.getByLabel(`${label} document`).innerText())).toEqual(saved)
 await page.getByLabel(`clip ${label} frameLayout`,{exact:true}).selectOption(String((parameters.frameLayout+1)%3));await expect(history).toHaveText(String(count+1));await page.getByRole('button',{name:'Undo',exact:true}).click();await expect(history).toHaveText(String(count));expect(JSON.parse(await page.getByLabel(`${label} document`).innerText())).toEqual(saved);await expect(page.getByLabel(`clip ${label} frameLayout`,{exact:true})).toHaveValue(String(parameters.frameLayout))
 const f32=(bits:number)=>new Float32Array(new Uint32Array([bits]).buffer)[0]!
 const oracle:number[]=[]
 for(let i=0;i<original.bits.length;i+=4){const alpha=f32(original.bits[i+3]);const a=Math.max(0,Math.min(255,Math.round(alpha*255+1e-4)));for(let c=0;c<3;c++){const v=alpha<=1e-6?0:Math.max(0,f32(original.bits[i+c])/alpha);const d=v<=0.0031308?v*12.92:1.055*Math.pow(v,1/2.4)-0.055;oracle.push(Math.round(Math.max(0,Math.min(255,Math.round(d*255+1e-4)))*a/255))}oracle.push(255)}
 const response=page.waitForResponse(r=>new URL(r.url()).pathname==='/api/compositions/nested-preview');await page.getByRole('button',{name:'Native Preview',exact:true}).click();expect((await response).status()).toBe(200)
 await expect.poll(()=>page.getByLabel(`${label} preview`,{exact:true}).evaluate((element,args)=>{const canvas=element as HTMLCanvasElement;if(canvas.width!==args.width||canvas.height!==args.height)return false;const pixels=canvas.getContext('2d')!.getImageData(0,0,canvas.width,canvas.height).data;return pixels.every((v,i)=>Math.abs(v-args.oracle[i]!)<=1)},{width:original.width,height:original.height,oracle})).toBe(true)
 await expect(page.getByLabel(`${label} error`,{exact:true})).toHaveText('');await expect(page.getByLabel(`${label} preview error`,{exact:true})).toHaveText('');expect(errors).toEqual([])
 if(profile===0||cases[profile-1]?.effect!==original.effect)await page.screenshot({path:`/tmp/video-kadr-stage1286-${slug}.png`,fullPage:true})
})

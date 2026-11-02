import {readFile} from 'node:fs/promises'
import {spawnSync} from 'node:child_process'
import {expect,test} from '@playwright/test'
if(process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE)test.use({launchOptions:{executablePath:process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE}})
for(let profile=0;profile<30;profile++)test(`real VR Fractal profile${profile} channels stereo undo and original preview`,async({page,request})=>{
 const fixture=JSON.parse(await readFile('../backend/tests/fixtures/filmcraft-vr-fractal-noise.json','utf8'))
 const cases=fixture.cases.filter((c:any)=>c.width===18&&c.rgba[3]===128&&c.parameters.contrast===137.5&&c.parameters.fractalType===0)
 expect(cases).toHaveLength(30)
 const original=cases[profile];const label='VR Fractal Noise';const slug='vr-fractal-noise';const parameters=original.parameters
 const encoded=spawnSync('ffmpeg',['-v','error','-f','rawvideo','-pixel_format','rgba','-video_size',`${original.width}x${original.height}`,'-framerate','1','-i','pipe:0','-c:v','ffv1','-pix_fmt','bgra','-f','matroska','pipe:1'],{input:Buffer.from(original.rgba),maxBuffer:16*1024*1024})
 expect(encoded.error).toBeUndefined();expect(encoded.status,encoded.stderr.toString()).toBe(0)
 const registered=await request.post('/api/auth/register',{data:{username:`vr-fractal-noise-basic-browser-${profile}`,password:'scope-browser-test-password'}});expect(registered.status()).toBe(201)
 const {token}=await registered.json() as {token:string}
 const uploaded=await request.post('/api/upload',{headers:{Authorization:`Bearer ${token}`},multipart:{file:{name:'vr-fractal-noise-fixture.mkv',mimeType:'video/x-matroska',buffer:encoded.stdout}}});expect(uploaded.ok()).toBe(true)
 const {id}=await uploaded.json() as {id:string}
 const constant=(value:number)=>({mode:'constant',value})
 const content={schemaVersion:1,timeBase:1000000,canvas:{width:original.width,height:original.height,fpsMilli:1000,background:{red:0,green:0,blue:0,alpha:1}},sources:{[id]:{id,kind:'video',durationTicks:1000000,width:original.width,height:original.height,hasAudio:false}},tracks:[{kind:'video',id:'main',name:'Image',hidden:false,locked:false,clips:[{id:'clip',sourceId:id,placement:{timelineStartTick:0,sourceInTick:0,sourceOutTick:1000000,speed:1},transform:{x:constant(0),y:constant(0),scaleX:constant(1),scaleY:constant(1),rotationDegrees:constant(0),anchorX:0.5,anchorY:0.5},opacity:constant(1),blendMode:'normal',effects:[{kind:'vr_fractal_noise',parameters}],enabled:true}]}]}
 const props={token,project:{schemaVersion:1,root:'root',compositions:{root:{durationUs:1000000,content,instances:[]}}}}
 await page.addInitScript(value=>localStorage.setItem('video-kadr.live-'+value.slug+'-test',JSON.stringify(value.props)),{slug,props})
 const errors:string[]=[];page.on('pageerror',error=>errors.push(error.message));await page.goto(`/e2e/fixtures/live-${slug}.html`)
 const history=page.getByLabel(`${label} history`);await expect(history).toHaveText('0')
 const channels:Record<string,number>=Object.fromEntries(['contrast','brightness','scale','complexity','evolution','opacity'].map(c=>[c,parameters[c]]))
 let count=0;for(const channel of Object.keys(channels)){await page.getByLabel(`clip ${label} ${channel} key`,{exact:true}).click();await expect(history).toHaveText(String(++count))}
 const saved=JSON.parse(await page.getByLabel(`${label} document`).innerText());const effect=saved.compositions.root.content.tracks[0].clips[0].effects[0]
 for(const[channel,value]of Object.entries(channels))expect(effect.animation[channel].track.keyframes).toEqual([{tick:500000,value}])
 const set=async(channel:string,value:number)=>page.getByLabel(`clip ${label} ${channel}`,{exact:true}).evaluate((element,value)=>{(element as HTMLInputElement).value=String(value);element.dispatchEvent(new Event('change',{bubbles:true}))},value)
 await set('evolution',42.125);await expect(history).toHaveText(String(count+1));const updated=JSON.parse(await page.getByLabel(`${label} document`).innerText()).compositions.root.content.tracks[0].clips[0].effects[0];expect(updated).toEqual({...effect,parameters:{...parameters,evolution:42.125}});await page.getByRole('button',{name:'Undo',exact:true}).click();await expect(history).toHaveText(String(count));expect(JSON.parse(await page.getByLabel(`${label} document`).innerText())).toEqual(saved)
 const key=page.getByLabel(`clip ${label} brightness 500000 value`,{exact:true});await key.locator('xpath=ancestor::details[1]').locator('summary').click();await key.evaluate(element=>{(element as HTMLInputElement).value='-12.5';element.dispatchEvent(new Event('change',{bubbles:true}))});await expect(history).toHaveText(String(count+1));await page.getByRole('button',{name:'Undo',exact:true}).click();await expect(history).toHaveText(String(count));expect(JSON.parse(await page.getByLabel(`${label} document`).innerText())).toEqual(saved)
 for(const[channel,modulus]of [['frameLayout',3],['fractalType',3],['mode',5]]as const){ await page.getByLabel(`clip ${label} ${channel}`,{exact:true}).selectOption(String((parameters[channel]+1)%modulus));await expect(history).toHaveText(String(count+1));await page.getByRole('button',{name:'Undo',exact:true}).click();await expect(history).toHaveText(String(count));expect(JSON.parse(await page.getByLabel(`${label} document`).innerText())).toEqual(saved);await expect(page.getByLabel(`clip ${label} ${channel}`,{exact:true})).toHaveValue(String(parameters[channel]))}
 const checkbox=page.getByLabel(`clip ${label} invert`,{exact:true});await checkbox.setChecked(!parameters.invert);await expect(history).toHaveText(String(count+1));await page.getByRole('button',{name:'Undo',exact:true}).click();await expect(history).toHaveText(String(count));expect(JSON.parse(await page.getByLabel(`${label} document`).innerText())).toEqual(saved);if(parameters.invert)await expect(checkbox).toBeChecked();else await expect(checkbox).not.toBeChecked()
 const f32=(bits:number)=>new Float32Array(new Uint32Array([bits]).buffer)[0]!
 const oracle:number[]=[]
 for(let i=0;i<original.bits.length;i+=4){const alpha=f32(original.bits[i+3]);const a=Math.max(0,Math.min(255,Math.round(alpha*255+1e-4)));for(let c=0;c<3;c++){const v=alpha<=1e-6?0:Math.max(0,f32(original.bits[i+c])/alpha);const d=v<=0.0031308?v*12.92:1.055*Math.pow(v,1/2.4)-0.055;oracle.push(Math.round(Math.max(0,Math.min(255,Math.round(d*255+1e-4)))*a/255))}oracle.push(255)}
 const response=page.waitForResponse(r=>new URL(r.url()).pathname==='/api/compositions/nested-preview');await page.getByRole('button',{name:'Native Preview',exact:true}).click();expect((await response).status()).toBe(200)
 await expect.poll(()=>page.getByLabel(`${label} preview`,{exact:true}).evaluate((element,args)=>{const canvas=element as HTMLCanvasElement;if(canvas.width!==args.width||canvas.height!==args.height)return false;const pixels=canvas.getContext('2d')!.getImageData(0,0,canvas.width,canvas.height).data;return pixels.every((v,i)=>Math.abs(v-args.oracle[i]!)<=1)},{width:original.width,height:original.height,oracle})).toBe(true)
 await expect(page.getByLabel(`${label} error`,{exact:true})).toHaveText('');await expect(page.getByLabel(`${label} preview error`,{exact:true})).toHaveText('');expect(errors).toEqual([])
 if(profile===0)await page.screenshot({path:`/tmp/video-kadr-stage1321-basic-${slug}.png`,fullPage:true})
})

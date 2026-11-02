import {readFile} from 'node:fs/promises'
import {spawnSync} from 'node:child_process'
import {expect,test} from '@playwright/test'
if(process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE)test.use({launchOptions:{executablePath:process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE}})
for(const profile of Array.from({length:15},(_,i)=>i)) test(`real Auto Reframe profile${profile} three keys undo and original preview`,async({page,request})=>{
 const fixture=JSON.parse(await readFile('../backend/tests/fixtures/filmcraft-auto-reframe.json','utf8'))
 const original=fixture.moving.filter((c:any)=>c.tick===500000)[profile]
 expect(original).toBeTruthy()
 const rgba=Buffer.from(Array.from({length:100},(_,k)=>Array.from({length:180},(_,i)=>{const x=Math.floor(k/10)%12+2;return Math.abs(i%18-x)<=1&&Math.abs(Math.floor(i/18)-4)<=1?[255,192,64,128]:[(i*7)%32,(i*3)%24,(i*11)%16,128]}).flat()).flat())
 const parameters=original.parameters
 const png=spawnSync('ffmpeg',['-v','error','-f','rawvideo','-pixel_format','rgba','-video_size','18x10','-framerate','100','-i','pipe:0','-c:v','ffv1','-pix_fmt','bgra','-f','matroska','pipe:1'],{input:rgba})
 expect(png.status).toBe(0)
 const registered=await request.post('/api/auth/register',{data:{username:`auto-reframe-browser-${profile}`,password:'scope-browser-test-password'}});expect(registered.status()).toBe(201)
 const {token}=await registered.json() as {token:string}
 const uploaded=await request.post('/api/upload',{headers:{Authorization:`Bearer ${token}`},multipart:{file:{name:'auto-reframe-fixture.mkv',mimeType:'video/x-matroska',buffer:png.stdout}}});expect(uploaded.ok()).toBe(true)
 const {id}=await uploaded.json() as {id:string};expect(id).toBeTruthy()
 const constant=(value:number)=>({mode:'constant',value})
 const content={schemaVersion:1,timeBase:1000000,canvas:{width:10,height:16,fpsMilli:29970,background:{red:0,green:0,blue:0,alpha:1}},sources:{[id]:{id,kind:'video',durationTicks:1000000,width:18,height:10,hasAudio:false}},tracks:[{kind:'video',id:'main',name:'Image',hidden:false,locked:false,clips:[{id:'clip',sourceId:id,placement:{timelineStartTick:0,sourceInTick:0,sourceOutTick:1000000,speed:1},transform:{x:constant(0),y:constant(0),scaleX:constant(1),scaleY:constant(1),rotationDegrees:constant(0),anchorX:0.5,anchorY:0.5},opacity:constant(1),blendMode:'normal',effects:[{kind:'auto_reframe',parameters}],enabled:true}]}]}

 const props={token,project:{schemaVersion:1,root:'root',compositions:{root:{durationUs:1000000,content,instances:[]}}}}
 await page.addInitScript(value=>localStorage.setItem('video-kadr.live-auto-reframe-test',JSON.stringify(value)),props)
 const errors:string[]=[];page.on('pageerror',error=>errors.push(error.message))
 await page.goto('/e2e/fixtures/live-auto-reframe.html')
 const history=page.getByLabel('Auto Reframe history');await expect(history).toHaveText('0')
 const set=async(channel:string,value:string)=>page.getByLabel(`clip Auto Reframe ${channel}`,{exact:true}).evaluate((element,value)=>{(element as HTMLInputElement).value=value;element.dispatchEvent(new Event('change',{bubbles:true}))},value)
 let count=0
 for(const channel of ['zoom','offsetX','offsetY']) {await page.getByLabel(`clip Auto Reframe ${channel} key`,{exact:true}).click();await expect(history).toHaveText(String(++count))}
 const saved=JSON.parse(await page.getByLabel('Auto Reframe document').innerText());const effect=saved.compositions.root.content.tracks[0].clips[0].effects[0]
 expect(effect.parameters).toEqual(parameters)
 for(const[channel,value]of Object.entries({zoom:parameters.zoom,offsetX:parameters.offset[0],offsetY:parameters.offset[1]}))expect(effect.animation[channel].track.keyframes).toEqual([{tick:500000,value}])
 const key=page.getByLabel('clip Auto Reframe zoom 500000 value',{exact:true});await key.locator('xpath=ancestor::details[1]').locator('summary').click();await key.evaluate(element=>{(element as HTMLInputElement).value='200';element.dispatchEvent(new Event('change',{bubbles:true}))});await expect(history).toHaveText(String(count+1));await page.getByRole('button',{name:'Undo',exact:true}).click();await expect(history).toHaveText(String(count));expect(JSON.parse(await page.getByLabel('Auto Reframe document').innerText())).toEqual(saved)
 await set('offsetX','2.123456789012');await expect(history).toHaveText(String(count+1));const changed=JSON.parse(await page.getByLabel('Auto Reframe document').innerText()).compositions.root.content.tracks[0].clips[0].effects[0];expect(changed).toEqual({...effect,parameters:{...parameters,offset:[2.123456789012,parameters.offset[1]]}});await page.getByRole('button',{name:'Undo',exact:true}).click();await expect(history).toHaveText(String(count));expect(JSON.parse(await page.getByLabel('Auto Reframe document').innerText())).toEqual(saved)
 for(const[channel,value,restored]of [['preset',(parameters.preset+1)%3,parameters.preset],['aspect',(parameters.aspect+1)%5,parameters.aspect]]as const){await page.getByLabel(`clip Auto Reframe ${channel}`,{exact:true}).selectOption(String(value));await expect(history).toHaveText(String(count+1));await page.getByRole('button',{name:'Undo',exact:true}).click();await expect(history).toHaveText(String(count));expect(JSON.parse(await page.getByLabel('Auto Reframe document').innerText())).toEqual(saved);await expect(page.getByLabel(`clip Auto Reframe ${channel}`,{exact:true})).toHaveValue(String(restored))}
 const f32=(bits:number)=>new Float32Array(new Uint32Array([bits]).buffer)[0]!
 const oracle:number[]=[]
 for(let y=0;y<16;y++)for(let x=0;x<10;x++){
  if(y<3||y>=13){oracle.push(0,0,0,255);continue}
  const i=((y-3)*18+x+4)*4;const alpha=f32(original.bits[i+3]);const a=Math.max(0,Math.min(255,Math.round(alpha*255+1e-4)))
  for(let c=0;c<3;c++){const v=alpha<=1e-6?0:Math.max(0,f32(original.bits[i+c])/alpha);const d=v<=0.0031308?v*12.92:1.055*Math.pow(v,1/2.4)-0.055;oracle.push(Math.round(Math.max(0,Math.min(255,Math.round(d*255+1e-4)))*a/255))}oracle.push(255)
 }

 const preview=page.waitForResponse(response=>new URL(response.url()).pathname==='/api/compositions/nested-preview')
 await page.getByRole('button',{name:'Native Preview',exact:true}).click();expect((await preview).status()).toBe(200)
 await expect.poll(()=>page.getByLabel('Auto Reframe preview',{exact:true}).evaluate((element,expected)=>{
  const canvas=element as HTMLCanvasElement;if(canvas.width!==10||canvas.height!==16)return false
  const pixels=canvas.getContext('2d')!.getImageData(0,0,10,16).data
  for(let i=0;i<pixels.length;i++)if(Math.abs(pixels[i]! - expected[i]!)>1)return false
  return true
 },oracle)).toBe(true)
 await expect(page.getByLabel('Auto Reframe error')).toHaveText('')
 expect(errors).toEqual([])
 if(profile===0)await page.screenshot({path:'/tmp/video-kadr-stage1240-auto-reframe.png',fullPage:true})
})

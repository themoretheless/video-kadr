import {readFile} from 'node:fs/promises'
import {spawnSync} from 'node:child_process'
import {expect,test} from '@playwright/test'
if(process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE)test.use({launchOptions:{executablePath:process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE}})
for(const profile of Array.from({length:32},(_,i)=>i)) test(`real Warp Stabilizer profile${profile} five keys undo and original preview`,async({page,request})=>{
 const fixture=JSON.parse(await readFile('../backend/tests/fixtures/filmcraft-warp-stabilizer.json','utf8'))
 const original=fixture.cases[profile]
 expect(original).toBeTruthy()
 const rgba=Buffer.from(Array.from({length:100},(_,frame)=>{const k=Math.round(frame/100*6);return Array.from({length:96*64},(_,i)=>{const x=((i%96-2*k)%96+96)%96;const y=((Math.floor(i/96)-k)%64+64)%64;return [(x*17+y*31+Math.floor(x/7)*Math.floor(y/5)*53)%256,(x*3+y*11)%256,(x*29+y*7)%256,original.alpha]}).flat()}).flat())
 const parameters=original.parameters
 const png=spawnSync('ffmpeg',['-v','error','-f','rawvideo','-pixel_format','rgba','-video_size','96x64','-framerate','100','-i','pipe:0','-c:v','ffv1','-pix_fmt','bgra','-f','matroska','pipe:1'],{input:rgba,maxBuffer:16*1024*1024})
 expect(png.error).toBeUndefined()
 expect(png.status,png.stderr.toString()).toBe(0)
 const registered=await request.post('/api/auth/register',{data:{username:`warp-stabilizer-browser-${profile}`,password:'scope-browser-test-password'}});expect(registered.status()).toBe(201)
 const {token}=await registered.json() as {token:string}
 const uploaded=await request.post('/api/upload',{headers:{Authorization:`Bearer ${token}`},multipart:{file:{name:'warp-stabilizer-fixture.mkv',mimeType:'video/x-matroska',buffer:png.stdout}}});expect(uploaded.ok()).toBe(true)
 const {id}=await uploaded.json() as {id:string};expect(id).toBeTruthy()
 const constant=(value:number)=>({mode:'constant',value})
 const content={schemaVersion:1,timeBase:1000000,canvas:{width:96,height:64,fpsMilli:6000,background:{red:0,green:0,blue:0,alpha:1}},sources:{[id]:{id,kind:'video',durationTicks:1000000,width:96,height:64,hasAudio:false}},tracks:[{kind:'video',id:'main',name:'Image',hidden:false,locked:false,clips:[{id:'clip',sourceId:id,placement:{timelineStartTick:0,sourceInTick:0,sourceOutTick:1000000,speed:1},transform:{x:constant(0),y:constant(0),scaleX:constant(1),scaleY:constant(1),rotationDegrees:constant(0),anchorX:0.5,anchorY:0.5},opacity:constant(1),blendMode:'normal',effects:[{kind:'warp_stabilizer',parameters}],enabled:true}]}]}

 const props={token,project:{schemaVersion:1,root:'root',compositions:{root:{durationUs:1000000,content,instances:[]}}}}
 await page.addInitScript(value=>localStorage.setItem('video-kadr.live-warp-stabilizer-test',JSON.stringify(value)),props)
 const errors:string[]=[];page.on('pageerror',error=>errors.push(error.message))
 await page.goto('/e2e/fixtures/live-warp-stabilizer.html')
 const history=page.getByLabel('Warp Stabilizer history');await expect(history).toHaveText('0')
 const set=async(channel:string,value:string)=>page.getByLabel(`clip Warp Stabilizer ${channel}`,{exact:true}).evaluate((element,value)=>{(element as HTMLInputElement).value=value;element.dispatchEvent(new Event('change',{bubbles:true}))},value)
 let count=0
 for(const channel of ['smoothness','maxScale','actionSafe','additionalScale','cropLess']) {await page.getByLabel(`clip Warp Stabilizer ${channel} key`,{exact:true}).click();await expect(history).toHaveText(String(++count))}
 const saved=JSON.parse(await page.getByLabel('Warp Stabilizer document').innerText());const effect=saved.compositions.root.content.tracks[0].clips[0].effects[0]
 expect(effect.parameters).toEqual(parameters)
 for(const[channel,value]of Object.entries({smoothness:50,maxScale:150,actionSafe:0,additionalScale:100,cropLess:50}))expect(effect.animation[channel].track.keyframes).toEqual([{tick:500000,value}])
 const key=page.getByLabel('clip Warp Stabilizer smoothness 500000 value',{exact:true});await key.locator('xpath=ancestor::details[1]').locator('summary').click();await key.evaluate(element=>{(element as HTMLInputElement).value='200';element.dispatchEvent(new Event('change',{bubbles:true}))});await expect(history).toHaveText(String(count+1));await page.getByRole('button',{name:'Undo',exact:true}).click();await expect(history).toHaveText(String(count));expect(JSON.parse(await page.getByLabel('Warp Stabilizer document').innerText())).toEqual(saved)
 await set('smoothness','75');await expect(history).toHaveText(String(count+1));const changed=JSON.parse(await page.getByLabel('Warp Stabilizer document').innerText()).compositions.root.content.tracks[0].clips[0].effects[0];expect(changed).toEqual({...effect,parameters:{...parameters,smoothness:75}});await page.getByRole('button',{name:'Undo',exact:true}).click();await expect(history).toHaveText(String(count));expect(JSON.parse(await page.getByLabel('Warp Stabilizer document').innerText())).toEqual(saved)
 for(const[channel,value,restored]of [['result',(parameters.result+1)%2,parameters.result],['method',(parameters.method+1)%4,parameters.method],['framing',(parameters.framing+1)%4,parameters.framing]]as const){await page.getByLabel(`clip Warp Stabilizer ${channel}`,{exact:true}).selectOption(String(value));await expect(history).toHaveText(String(count+1));await page.getByRole('button',{name:'Undo',exact:true}).click();await expect(history).toHaveText(String(count));expect(JSON.parse(await page.getByLabel('Warp Stabilizer document').innerText())).toEqual(saved);await expect(page.getByLabel(`clip Warp Stabilizer ${channel}`,{exact:true})).toHaveValue(String(restored))}
 const f32=(bits:number)=>new Float32Array(new Uint32Array([bits]).buffer)[0]!
 const oracle:number[]=[]
 for(let i=0;i<96*64*4;i+=4){
  const alpha=f32(original.bits[i+3]);const a=Math.max(0,Math.min(255,Math.round(alpha*255+1e-4)))
  for(let c=0;c<3;c++){const v=alpha<=1e-6?0:Math.max(0,f32(original.bits[i+c])/alpha);const d=v<=0.0031308?v*12.92:1.055*Math.pow(v,1/2.4)-0.055;oracle.push(Math.round(Math.max(0,Math.min(255,Math.round(d*255+1e-4)))*a/255))}oracle.push(255)
 }

 const preview=page.waitForResponse(response=>new URL(response.url()).pathname==='/api/compositions/nested-preview')
 await page.getByRole('button',{name:'Native Preview',exact:true}).click();expect((await preview).status()).toBe(200)
 await expect.poll(()=>page.getByLabel('Warp Stabilizer preview',{exact:true}).evaluate((element,expected)=>{
  const canvas=element as HTMLCanvasElement;if(canvas.width!==96||canvas.height!==64)return false
  const pixels=canvas.getContext('2d')!.getImageData(0,0,96,64).data
  for(let i=0;i<pixels.length;i++)if(Math.abs(pixels[i]! - expected[i]!)>1)return false
  return true
 },oracle)).toBe(true)
 await expect(page.getByLabel('Warp Stabilizer error')).toHaveText('')
 expect(errors).toEqual([])
 if(profile===0)await page.screenshot({path:'/tmp/video-kadr-stage1256-warp-stabilizer.png',fullPage:true})
})

import {readFile} from 'node:fs/promises'
import {spawnSync} from 'node:child_process'
import {expect,test} from '@playwright/test'
if(process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE)test.use({launchOptions:{executablePath:process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE}})
const profiles=Array.from({length:24},(_,index)=>index)
for(const profileIndex of profiles)test(`real TrackMatte effected external track ${profileIndex} controls undo original preview`,async({page,request})=>{
 const fixture=JSON.parse(await readFile('../backend/tests/fixtures/filmcraft-track-matte-composition.json','utf8'))
 const original=fixture.cases[profileIndex]
 expect(original).toBeTruthy()
 const png=spawnSync('ffmpeg',['-v','error','-f','rawvideo','-pixel_format','rgba','-video_size','18x10','-i','pipe:0','-frames:v','1','-f','image2pipe','-vcodec','png','pipe:1'],{input:Buffer.from(original.rgba)})
 expect(png.status).toBe(0)
 const registered=await request.post('/api/auth/register',{data:{username:`track-matte-external-browser-${profileIndex}`,password:'scope-browser-test-password'}});expect(registered.status()).toBe(201)
 const {token}=await registered.json() as {token:string}
 const uploaded=await request.post('/api/upload',{headers:{Authorization:`Bearer ${token}`},multipart:{file:{name:'track-matte-fixture.png',mimeType:'image/png',buffer:png.stdout}}});expect(uploaded.ok()).toBe(true)
 const {id}=await uploaded.json() as {id:string};expect(id).toBeTruthy()
 const controlPng=spawnSync('ffmpeg',['-v','error','-f','rawvideo','-pixel_format','rgba','-video_size','6x4','-i','pipe:0','-frames:v','1','-f','image2pipe','-vcodec','png','pipe:1'],{input:Buffer.from(original.controlRgba)});expect(controlPng.status).toBe(0)
 const controlUpload=await request.post('/api/upload',{headers:{Authorization:`Bearer ${token}`},multipart:{file:{name:'track-matte-control.png',mimeType:'image/png',buffer:controlPng.stdout}}});expect(controlUpload.ok()).toBe(true)
 const {id:controlId}=await controlUpload.json() as {id:string}
 const constant=(value:number)=>({mode:'constant',value})
 const content={schemaVersion:1,timeBase:1000000,canvas:{width:18,height:10,fpsMilli:10000,background:{red:0,green:0,blue:0,alpha:1}},sources:{[id]:{id,kind:'image',durationTicks:0,width:18,height:10,hasAudio:false}},tracks:[{kind:'image',id:'main',name:'Image',hidden:false,locked:false,clips:[{id:'clip',sourceId:id,timelineStartTick:1000000,durationTicks:1000000,transform:{x:constant(0),y:constant(0),scaleX:constant(1),scaleY:constant(1),rotationDegrees:constant(0),anchorX:0.5,anchorY:0.5},opacity:constant(1),blendMode:'normal',effects:[],enabled:true}]}]}

 content.sources[controlId]={id:controlId,kind:'image',durationTicks:0,width:6,height:4,hasAudio:false}
 content.tracks.push({kind:'image',id:'control',name:'Control',hidden:true,locked:false,clips:[{...content.tracks[0]!.clips[0]!,id:'control',sourceId:controlId,opacity:constant(0.7),transform:{x:constant(original.transform.x),y:constant(original.transform.y),scaleX:constant(original.transform.scaleX),scaleY:constant(original.transform.scaleY),rotationDegrees:constant(original.transform.rotation),anchorX:0.5,anchorY:0.5},effects:[{kind:'compound_blur',parameters:{max:original.controlBlur}}]}]})

 const props={token,project:{schemaVersion:1,root:'root',compositions:{root:{durationUs:2500000,content,instances:[]}}}}
 await page.addInitScript(value=>localStorage.setItem('video-kadr.live-track-matte-test',JSON.stringify(value)),props)
 const errors:string[]=[];page.on('pageerror',error=>errors.push(error.message))
 await page.goto('/e2e/fixtures/live-track-matte.html')
 await page.getByRole('button',{name:'Добавить Track Matte Key',exact:true}).first().click()
 const history=page.getByLabel('TrackMatte history');await expect(history).toHaveText('1')
 let count=1
 await page.getByLabel('clip Track Matte matte',{exact:true}).selectOption('2');await expect(history).toHaveText(String(++count))
 if(original.parameters.composite===1){await page.getByLabel('clip Track Matte composite',{exact:true}).selectOption('1');await expect(history).toHaveText(String(++count))}
 if(original.parameters.reverse){await page.getByLabel('clip Track Matte reverse',{exact:true}).check();await expect(history).toHaveText(String(++count))}
 const saved=JSON.parse(await page.getByLabel('TrackMatte document').innerText())
 for(const [channel,value] of [['matte','4'],['composite',String(1-original.parameters.composite)]]){
  await page.getByLabel(`clip Track Matte ${channel}`,{exact:true}).selectOption(value!);await expect(history).toHaveText(String(count+1))
  await page.getByRole('button',{name:'Undo',exact:true}).click();await expect(history).toHaveText(String(count))
  expect(JSON.parse(await page.getByLabel('TrackMatte document').innerText())).toEqual(saved)
 }
 await page.getByLabel('clip Track Matte reverse',{exact:true}).click();await expect(history).toHaveText(String(count+1))
 await page.getByRole('button',{name:'Undo',exact:true}).click();await expect(history).toHaveText(String(count))
 expect(JSON.parse(await page.getByLabel('TrackMatte document').innerText())).toEqual(saved)
 const f32=(bits:number)=>new Float32Array(new Uint32Array([bits]).buffer)[0]!
 const oracle:number[]=[]
 for(let i=0;i<original.bits.length;i+=4){
  const alpha=f32(original.bits[i+3]);const a=Math.max(0,Math.min(255,Math.round(alpha*255+1e-4)))
  for(let c=0;c<3;c++){
   const v=alpha<=1e-6?0:Math.max(0,f32(original.bits[i+c])/alpha)
   const d=v<=0.0031308?v*12.92:1.055*Math.pow(v,1/2.4)-0.055
   oracle.push(Math.round(Math.max(0,Math.min(255,Math.round(d*255+1e-4)))*a/255))
  }
  oracle.push(255)
 }

 const preview=page.waitForResponse(response=>new URL(response.url()).pathname==='/api/compositions/nested-preview')
 await page.getByRole('button',{name:'Native Preview',exact:true}).click();const response=await preview;expect(response.status(),await response.text()).toBe(200)
 await expect.poll(()=>page.getByLabel('TrackMatte preview',{exact:true}).evaluate((element,expected)=>{
  const canvas=element as HTMLCanvasElement;if(canvas.width!==18||canvas.height!==10)return false
  const pixels=canvas.getContext('2d')!.getImageData(0,0,18,10).data
  for(let i=0;i<pixels.length;i++)if(Math.abs(pixels[i]! - expected[i]!)>1)return false
  return true
 },oracle)).toBe(true)
 await expect(page.getByLabel('TrackMatte error')).toHaveText('')
 await expect(page.getByLabel('TrackMatte preview error')).toHaveText('')
 if(profileIndex===0)await page.screenshot({path:'/tmp/video-kadr-stage1399-track-matte.png',fullPage:true})
 expect(errors).toEqual([])
})

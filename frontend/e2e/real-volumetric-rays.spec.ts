import {readFile} from 'node:fs/promises'
import {spawnSync} from 'node:child_process'
import {expect,test} from '@playwright/test'
if(process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE)test.use({launchOptions:{executablePath:process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE}})
for(const profile of Array.from({length:64},(_,i)=>i)) test(`real Volumetric Rays profile${profile} nine keys undo and original preview`,async({page,request})=>{
 const fixture=JSON.parse(await readFile('../backend/tests/fixtures/filmcraft-volumetric-rays.json','utf8'))
 const original=fixture.cases.filter((c:any)=>c.width===18&&c.height===10&&c.scale===1)[profile]
 expect(original).toBeTruthy()
 const png=spawnSync('ffmpeg',['-v','error','-f','rawvideo','-pixel_format','rgba','-video_size','18x10','-i','pipe:0','-frames:v','1','-f','image2pipe','-vcodec','png','pipe:1'],{input:Buffer.from(original.rgba)})
 expect(png.status).toBe(0)
 const registered=await request.post('/api/auth/register',{data:{username:`volumetric-rays-browser-${profile}`,password:'scope-browser-test-password'}});expect(registered.status()).toBe(201)
 const {token}=await registered.json() as {token:string}
 const uploaded=await request.post('/api/upload',{headers:{Authorization:`Bearer ${token}`},multipart:{file:{name:'volumetric-rays-fixture.png',mimeType:'image/png',buffer:png.stdout}}});expect(uploaded.ok()).toBe(true)
 const {id}=await uploaded.json() as {id:string};expect(id).toBeTruthy()
 const constant=(value:number)=>({mode:'constant',value})
 const content={schemaVersion:1,timeBase:1000000,canvas:{width:18,height:10,fpsMilli:10000,background:{red:0,green:0,blue:0,alpha:1}},sources:{[id]:{id,kind:'image',durationTicks:0,width:18,height:10,hasAudio:false}},tracks:[{kind:'image',id:'main',name:'Image',hidden:false,locked:false,clips:[{id:'clip',sourceId:id,timelineStartTick:0,durationTicks:1000000,transform:{x:constant(0),y:constant(0),scaleX:constant(1),scaleY:constant(1),rotationDegrees:constant(0),anchorX:0.5,anchorY:0.5},opacity:constant(1),blendMode:'normal',effects:[{kind:'volumetric_rays',parameters:original.parameters}],enabled:true}]}]}

 const props={token,project:{schemaVersion:1,root:'root',compositions:{root:{durationUs:1000000,content,instances:[]}}}}
 await page.addInitScript(value=>localStorage.setItem('video-kadr.live-volumetric-rays-test',JSON.stringify(value)),props)
 const errors:string[]=[];page.on('pageerror',error=>errors.push(error.message))
 await page.goto('/e2e/fixtures/live-volumetric-rays.html')
 const history=page.getByLabel('Volumetric Rays history');await expect(history).toHaveText('0')
 const set=async(channel:string,value:string)=>page.getByLabel(`clip Volumetric Rays ${channel}`,{exact:true}).evaluate((element,value)=>{(element as HTMLInputElement).value=value;element.dispatchEvent(new Event('change',{bubbles:true}))},value)
 let count=0
 for(const channel of ['threshold','length','centerX','centerY','intensity','colorRed','colorGreen','colorBlue','colorAlpha']) {await page.getByLabel(`clip Volumetric Rays ${channel} key`,{exact:true}).click();await expect(history).toHaveText(String(++count))}
 const saved=JSON.parse(await page.getByLabel('Volumetric Rays document').innerText())
 const effect=saved.compositions.root.content.tracks[0].clips[0].effects[0]
 expect(effect.parameters).toEqual(original.parameters)
 for(const[channel,value]of Object.entries({threshold:original.parameters.threshold,centerX:original.parameters.center?.[0]??9,centerY:original.parameters.center?.[1]??2.5,length:original.parameters.length,intensity:original.parameters.intensity,colorRed:original.parameters.color[0],colorGreen:original.parameters.color[1],colorBlue:original.parameters.color[2],colorAlpha:original.parameters.color[3]})){expect(effect.animation[channel].track.keyframes).toEqual([{tick:500000,value}])}
 const key=page.getByLabel('clip Volumetric Rays intensity 500000 value',{exact:true})
 await key.locator('xpath=ancestor::details[1]').locator('summary').click()
 await key.evaluate(element=>{(element as HTMLInputElement).value='200';element.dispatchEvent(new Event('change',{bubbles:true}))});await expect(history).toHaveText(String(count+1))
 await page.getByRole('button',{name:'Undo',exact:true}).click();await expect(history).toHaveText(String(count));expect(JSON.parse(await page.getByLabel('Volumetric Rays document').innerText())).toEqual(saved)
 await set('colorRed', original.parameters.color[0] === 0.125 ? '0.25' : '0.125');await expect(history).toHaveText(String(count+1))
 await page.getByRole('button',{name:'Undo',exact:true}).click();await expect(history).toHaveText(String(count));expect(JSON.parse(await page.getByLabel('Volumetric Rays document').innerText())).toEqual(saved)
 await set('centerX',original.parameters.center?.[0]===1.123456789012?'2.123456789012':'1.123456789012');await expect(history).toHaveText(String(count+1))
 const changed=JSON.parse(await page.getByLabel('Volumetric Rays document').innerText()).compositions.root.content.tracks[0].clips[0].effects[0]
 expect(changed.parameters.center).toEqual([original.parameters.center?.[0]===1.123456789012?2.123456789012:1.123456789012,original.parameters.center?.[1]??null]);expect(changed.animation).toEqual(effect.animation)
 await page.getByRole('button',{name:'Undo',exact:true}).click();await expect(history).toHaveText(String(count));expect(JSON.parse(await page.getByLabel('Volumetric Rays document').innerText())).toEqual(saved)
 await page.getByLabel('clip Volumetric Rays only',{exact:true}).click();await expect(history).toHaveText(String(count+1))
 await page.getByRole('button',{name:'Undo',exact:true}).click();await expect(history).toHaveText(String(count));expect(JSON.parse(await page.getByLabel('Volumetric Rays document').innerText())).toEqual(saved)
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
 await page.getByRole('button',{name:'Native Preview',exact:true}).click();expect((await preview).status()).toBe(200)
 await expect.poll(()=>page.getByLabel('Volumetric Rays preview',{exact:true}).evaluate((element,expected)=>{
  const canvas=element as HTMLCanvasElement;if(canvas.width!==18||canvas.height!==10)return false
  const pixels=canvas.getContext('2d')!.getImageData(0,0,18,10).data
  for(let i=0;i<pixels.length;i++)if(Math.abs(pixels[i]! - expected[i]!)>1)return false
  return true
 },oracle)).toBe(true)
 await expect(page.getByLabel('Volumetric Rays error')).toHaveText('')
 expect(errors).toEqual([])
 if(profile===0)await page.screenshot({path:'/tmp/video-kadr-stage1209-volumetric-rays.png',fullPage:true})
})

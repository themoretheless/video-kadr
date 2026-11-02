import {readFile} from 'node:fs/promises'
import {spawnSync} from 'node:child_process'
import {expect,test} from '@playwright/test'
if(process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE)test.use({launchOptions:{executablePath:process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE}})
for(const profile of Array.from({length:7},(_,i)=>i)) test(`real Posterize Time profile${profile} rate key undo and original preview`,async({page,request})=>{
 const fixture=JSON.parse(await readFile('../backend/tests/fixtures/filmcraft-posterize-time.json','utf8'))
 const original=fixture.cases.filter((c:any)=>c.seconds===0.5)[profile]
 expect(original).toBeTruthy()
 const rgba=Buffer.from(Array.from({length:100},(_,k)=>Array.from({length:24},(_,i)=>[(i*17+k*11)%256,(i*7+k*23)%256,(i*31+k*3)%256,255]).flat()).flat())
 const parameters={rate:original.rate}
 const png=spawnSync('ffmpeg',['-v','error','-f','rawvideo','-pixel_format','rgba','-video_size','6x4','-framerate','100','-i','pipe:0','-c:v','ffv1','-pix_fmt','bgra','-f','matroska','pipe:1'],{input:rgba})
 expect(png.status).toBe(0)
 const registered=await request.post('/api/auth/register',{data:{username:`posterize-time-browser-${profile}`,password:'scope-browser-test-password'}});expect(registered.status()).toBe(201)
 const {token}=await registered.json() as {token:string}
 const uploaded=await request.post('/api/upload',{headers:{Authorization:`Bearer ${token}`},multipart:{file:{name:'posterize-time-fixture.mkv',mimeType:'video/x-matroska',buffer:png.stdout}}});expect(uploaded.ok()).toBe(true)
 const {id}=await uploaded.json() as {id:string};expect(id).toBeTruthy()
 const constant=(value:number)=>({mode:'constant',value})
 const content={schemaVersion:1,timeBase:1000000,canvas:{width:6,height:4,fpsMilli:10000,background:{red:0,green:0,blue:0,alpha:1}},sources:{[id]:{id,kind:'video',durationTicks:1000000,width:6,height:4,hasAudio:false}},tracks:[{kind:'video',id:'main',name:'Image',hidden:false,locked:false,clips:[{id:'clip',sourceId:id,placement:{timelineStartTick:0,sourceInTick:0,sourceOutTick:1000000,speed:1},transform:{x:constant(0),y:constant(0),scaleX:constant(1),scaleY:constant(1),rotationDegrees:constant(0),anchorX:0.5,anchorY:0.5},opacity:constant(1),blendMode:'normal',effects:[{kind:'posterize_time',parameters}],enabled:true}]}]}

 const props={token,project:{schemaVersion:1,root:'root',compositions:{root:{durationUs:1000000,content,instances:[]}}}}
 await page.addInitScript(value=>localStorage.setItem('video-kadr.live-posterize-time-test',JSON.stringify(value)),props)
 const errors:string[]=[];page.on('pageerror',error=>errors.push(error.message))
 await page.goto('/e2e/fixtures/live-posterize-time.html')
 const history=page.getByLabel('Posterize Time history');await expect(history).toHaveText('0')
 const set=async(channel:string,value:string)=>page.getByLabel(`clip Posterize Time ${channel}`,{exact:true}).evaluate((element,value)=>{(element as HTMLInputElement).value=value;element.dispatchEvent(new Event('change',{bubbles:true}))},value)
 let count=0
 await page.getByLabel('clip Posterize Time rate key',{exact:true}).click();await expect(history).toHaveText('1');count=1
 const saved=JSON.parse(await page.getByLabel('Posterize Time document').innerText())
 const effect=saved.compositions.root.content.tracks[0].clips[0].effects[0]
 expect(effect.parameters).toEqual(parameters);expect(effect.animation.rate.track.keyframes).toEqual([{tick:500000,value:parameters.rate}])
 const key=page.getByLabel('clip Posterize Time rate 500000 value',{exact:true});await key.locator('xpath=ancestor::details[1]').locator('summary').click()
 await key.evaluate(element=>{(element as HTMLInputElement).value='24';element.dispatchEvent(new Event('change',{bubbles:true}))});await expect(history).toHaveText('2')
 await page.getByRole('button',{name:'Undo',exact:true}).click();await expect(history).toHaveText('1');expect(JSON.parse(await page.getByLabel('Posterize Time document').innerText())).toEqual(saved)
 await set('rate','15');await expect(history).toHaveText('2');await page.getByRole('button',{name:'Undo',exact:true}).click();await expect(history).toHaveText('1');expect(JSON.parse(await page.getByLabel('Posterize Time document').innerText())).toEqual(saved)
 const requested=500000+Math.round((original.dt??0)*1000000);const sourceIndex=Math.floor(requested/10000)
 const oracle=Array.from({length:24},(_,i)=>[(i*17+sourceIndex*11)%256,(i*7+sourceIndex*23)%256,(i*31+sourceIndex*3)%256,255]).flat()
 const preview=page.waitForResponse(response=>new URL(response.url()).pathname==='/api/compositions/nested-preview')
 await page.getByRole('button',{name:'Native Preview',exact:true}).click();expect((await preview).status()).toBe(200)
 await expect.poll(()=>page.getByLabel('Posterize Time preview',{exact:true}).evaluate((element,expected)=>{
  const canvas=element as HTMLCanvasElement;if(canvas.width!==6||canvas.height!==4)return false
  const pixels=canvas.getContext('2d')!.getImageData(0,0,6,4).data
  for(let i=0;i<pixels.length;i++)if(Math.abs(pixels[i]! - expected[i]!)>1)return false
  return true
 },oracle)).toBe(true)
 await expect(page.getByLabel('Posterize Time error')).toHaveText('')
 expect(errors).toEqual([])
 if(profile===0)await page.screenshot({path:'/tmp/video-kadr-stage1230-posterize-time.png',fullPage:true})
})

import {readFile} from 'node:fs/promises'
import {spawnSync} from 'node:child_process'
import {expect,test} from '@playwright/test'
if(process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE)test.use({launchOptions:{executablePath:process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE}})
test('real PaintBucket editor validates scalar and color keys undo and actual preview',async({page,request})=>{
 const fixture=JSON.parse(await readFile('../backend/tests/fixtures/filmcraft-paint-bucket.json','utf8'))
 const original=fixture.cases.find((c:any)=>c.width===18&&c.height===10&&c.parameters.selector===1&&c.parameters.mode===1&&c.parameters.invert===true&&c.parameters.tolerance===10&&c.parameters.opacity===25&&c.parameters.point[0]!==null&&c.parameters.point[0]>0&&c.parameters.color[0]===-1)
 expect(original).toBeTruthy()
 const png=spawnSync('ffmpeg',['-v','error','-f','rawvideo','-pixel_format','rgba','-video_size','18x10','-i','pipe:0','-frames:v','1','-f','image2pipe','-vcodec','png','pipe:1'],{input:Buffer.from(original.rgba)})
 expect(png.status).toBe(0)
 const registered=await request.post('/api/auth/register',{data:{username:'paint-bucket-browser-user',password:'scope-browser-test-password'}});expect(registered.status()).toBe(201)
 const {token}=await registered.json() as {token:string}
 const uploaded=await request.post('/api/upload',{headers:{Authorization:`Bearer ${token}`},multipart:{file:{name:'paint-bucket-fixture.png',mimeType:'image/png',buffer:png.stdout}}});expect(uploaded.ok()).toBe(true)
 const {id}=await uploaded.json() as {id:string};expect(id).toBeTruthy()
 const constant=(value:number)=>({mode:'constant',value})
 const content={schemaVersion:1,timeBase:1000000,canvas:{width:18,height:10,fpsMilli:10000,background:{red:0,green:0,blue:0,alpha:1}},sources:{[id]:{id,kind:'image',durationTicks:0,width:18,height:10,hasAudio:false}},tracks:[{kind:'image',id:'main',name:'Image',hidden:false,locked:false,clips:[{id:'clip',sourceId:id,timelineStartTick:0,durationTicks:1000000,transform:{x:constant(0),y:constant(0),scaleX:constant(1),scaleY:constant(1),rotationDegrees:constant(0),anchorX:0.5,anchorY:0.5},opacity:constant(1),blendMode:'normal',effects:[],enabled:true}]}]}

 const props={token,project:{schemaVersion:1,root:'root',compositions:{root:{durationUs:1000000,content,instances:[]}}}}
 await page.addInitScript(value=>localStorage.setItem('video-kadr.live-paint-bucket-test',JSON.stringify(value)),props)
 const errors:string[]=[];page.on('pageerror',error=>errors.push(error.message))
 await page.goto('/e2e/fixtures/live-paint-bucket.html')
 await page.getByRole('button',{name:'Добавить PaintBucket',exact:true}).click()
 const history=page.getByLabel('PaintBucket history');await expect(history).toHaveText('1')
 const set=async(channel:string,value:string)=>page.getByLabel(`clip PaintBucket ${channel}`,{exact:true}).evaluate((element,value)=>{(element as HTMLInputElement).value=value;element.dispatchEvent(new Event('change',{bubbles:true}))},value)
 await page.getByLabel('clip PaintBucket selector',{exact:true}).selectOption('1');await expect(history).toHaveText('2')
 await page.getByLabel('clip PaintBucket mode',{exact:true}).selectOption('1');await expect(history).toHaveText('3')
 await page.getByLabel('clip PaintBucket invert',{exact:true}).check();await expect(history).toHaveText('4')
 let count=4
 for(const [channel,value] of [['pointX','1.23456789012345'],['pointY',''],['tolerance','10'],['opacity','25'],['colorR','-1'],['colorG','2'],['colorB','0.5'],['colorA','0']]) {
  await set(channel!,value!);await expect(history).toHaveText(String(++count))
  await page.getByLabel(`clip PaintBucket ${channel} key`,{exact:true}).click();await expect(history).toHaveText(String(++count))
 }
 const saved=JSON.parse(await page.getByLabel('PaintBucket document').innerText())
 const key=page.getByLabel('clip PaintBucket tolerance 500000 value',{exact:true})
 await key.locator('xpath=ancestor::details').locator('summary').click()
 await key.evaluate(element=>{(element as HTMLInputElement).value='20';element.dispatchEvent(new Event('change',{bubbles:true}))});await expect(history).toHaveText(String(count+1))
 await page.getByRole('button',{name:'Undo',exact:true}).click();await expect(history).toHaveText(String(count))
 expect(JSON.parse(await page.getByLabel('PaintBucket document').innerText())).toEqual(saved)
 await set('pointX','');await expect(history).toHaveText(String(count+1))
 const automatic=JSON.parse(await page.getByLabel('PaintBucket document').innerText()).compositions.root.content.tracks[0].clips[0].effects[0]
 expect(automatic.parameters.point).toEqual([null,null])
 await page.getByRole('button',{name:'Undo',exact:true}).click();await expect(history).toHaveText(String(count))
 expect(JSON.parse(await page.getByLabel('PaintBucket document').innerText())).toEqual(saved)
 for(const [channel,choice] of [['selector','3'],['mode','4']]) {
  await page.getByLabel(`clip PaintBucket ${channel}`,{exact:true}).selectOption(choice!);await expect(history).toHaveText(String(count+1))
  await page.getByRole('button',{name:'Undo',exact:true}).click();await expect(history).toHaveText(String(count))
  expect(JSON.parse(await page.getByLabel('PaintBucket document').innerText())).toEqual(saved)
 }
 await page.getByLabel('clip PaintBucket invert',{exact:true}).uncheck();await expect(history).toHaveText(String(count+1))
 await page.getByRole('button',{name:'Undo',exact:true}).click();await expect(history).toHaveText(String(count))
 expect(JSON.parse(await page.getByLabel('PaintBucket document').innerText())).toEqual(saved)
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
 await expect.poll(()=>page.getByLabel('PaintBucket preview',{exact:true}).evaluate((element,expected)=>{
  const canvas=element as HTMLCanvasElement;if(canvas.width!==18||canvas.height!==10)return false
  const pixels=canvas.getContext('2d')!.getImageData(0,0,18,10).data
  for(let i=0;i<pixels.length;i++)if(Math.abs(pixels[i]! - expected[i]!)>1)return false
  return true
 },oracle)).toBe(true)
 await expect(page.getByLabel('PaintBucket error')).toHaveText('')
 expect(errors).toEqual([])
})

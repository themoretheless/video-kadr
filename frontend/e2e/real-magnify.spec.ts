import {readFile} from 'node:fs/promises'
import {spawnSync} from 'node:child_process'
import {expect,test} from '@playwright/test'
if(process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE)test.use({launchOptions:{executablePath:process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE}})
test('real Magnify editor validates scalar and color keys undo and actual preview',async({page,request})=>{
 const fixture=JSON.parse(await readFile('../backend/tests/fixtures/filmcraft-magnify.json','utf8'))
 const original=fixture.cases.find((c:any)=>c.width===18&&c.height===10&&c.parameters.shape===1&&c.parameters.mode===4&&c.parameters.magnification===200&&c.parameters.size===2.5&&c.parameters.feather===0&&c.parameters.opacity===50&&c.parameters.center===null&&c.scale===1)
 expect(original).toBeTruthy()
 const png=spawnSync('ffmpeg',['-v','error','-f','rawvideo','-pixel_format','rgba','-video_size','18x10','-i','pipe:0','-frames:v','1','-f','image2pipe','-vcodec','png','pipe:1'],{input:Buffer.from(original.rgba)})
 expect(png.status).toBe(0)
 const registered=await request.post('/api/auth/register',{data:{username:'magnify-browser-user',password:'scope-browser-test-password'}});expect(registered.status()).toBe(201)
 const {token}=await registered.json() as {token:string}
 const uploaded=await request.post('/api/upload',{headers:{Authorization:`Bearer ${token}`},multipart:{file:{name:'magnify-fixture.png',mimeType:'image/png',buffer:png.stdout}}});expect(uploaded.ok()).toBe(true)
 const {id}=await uploaded.json() as {id:string};expect(id).toBeTruthy()
 const constant=(value:number)=>({mode:'constant',value})
 const content={schemaVersion:1,timeBase:1000000,canvas:{width:18,height:10,fpsMilli:10000,background:{red:0,green:0,blue:0,alpha:1}},sources:{[id]:{id,kind:'image',durationTicks:0,width:18,height:10,hasAudio:false}},tracks:[{kind:'image',id:'main',name:'Image',hidden:false,locked:false,clips:[{id:'clip',sourceId:id,timelineStartTick:0,durationTicks:1000000,transform:{x:constant(0),y:constant(0),scaleX:constant(1),scaleY:constant(1),rotationDegrees:constant(0),anchorX:0.5,anchorY:0.5},opacity:constant(1),blendMode:'normal',effects:[],enabled:true}]}]}

 const props={token,project:{schemaVersion:1,root:'root',compositions:{root:{durationUs:1000000,content,instances:[]}}}}
 await page.addInitScript(value=>localStorage.setItem('video-kadr.live-magnify-test',JSON.stringify(value)),props)
 const errors:string[]=[];page.on('pageerror',error=>errors.push(error.message))
 await page.goto('/e2e/fixtures/live-magnify.html')
 await page.getByRole('button',{name:'Добавить Magnify',exact:true}).click()
 const history=page.getByLabel('Magnify history');await expect(history).toHaveText('1')
 const set=async(channel:string,value:string)=>page.getByLabel(`clip Magnify ${channel}`,{exact:true}).evaluate((element,value)=>{(element as HTMLInputElement).value=value;element.dispatchEvent(new Event('change',{bubbles:true}))},value)
 let count=1
 for(const[channel,value]of [['shape','1'],['mode','4']]){await page.getByLabel(`clip Magnify ${channel}`,{exact:true}).selectOption(value!);await expect(history).toHaveText(String(++count))}
 for(const [channel,value] of [['centerX','9'],['centerY','5'],['magnification','200'],['size','2.5'],['feather','0'],['opacity','50']]) {
  const input=page.getByLabel(`clip Magnify ${channel}`,{exact:true})
  if(await input.inputValue()!==value){await set(channel!,value!);await expect(history).toHaveText(String(++count))}
  await page.getByLabel(`clip Magnify ${channel} key`,{exact:true}).click();await expect(history).toHaveText(String(++count))
 }
 const saved=JSON.parse(await page.getByLabel('Magnify document').innerText())
 const key=page.getByLabel('clip Magnify magnification 500000 value',{exact:true})
 await key.locator('xpath=ancestor::details').locator('summary').click()
 await key.evaluate(element=>{(element as HTMLInputElement).value='300';element.dispatchEvent(new Event('change',{bubbles:true}))});await expect(history).toHaveText(String(count+1))
 await page.getByRole('button',{name:'Undo',exact:true}).click();await expect(history).toHaveText(String(count))
 expect(JSON.parse(await page.getByLabel('Magnify document').innerText())).toEqual(saved)
 await page.getByLabel('clip Magnify shape',{exact:true}).selectOption('0');await expect(history).toHaveText(String(count+1))
 await page.getByRole('button',{name:'Undo',exact:true}).click();await expect(history).toHaveText(String(count))
 expect(JSON.parse(await page.getByLabel('Magnify document').innerText())).toEqual(saved)
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
 await expect.poll(()=>page.getByLabel('Magnify preview',{exact:true}).evaluate((element,expected)=>{
  const canvas=element as HTMLCanvasElement;if(canvas.width!==18||canvas.height!==10)return false
  const pixels=canvas.getContext('2d')!.getImageData(0,0,18,10).data
  for(let i=0;i<pixels.length;i++)if(Math.abs(pixels[i]! - expected[i]!)>1)return false
  return true
 },oracle)).toBe(true)
 await expect(page.getByLabel('Magnify error')).toHaveText('')
 expect(errors).toEqual([])
})

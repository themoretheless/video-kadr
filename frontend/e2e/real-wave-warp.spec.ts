import {readFile} from 'node:fs/promises'
import {expect,test} from '@playwright/test'
if(process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE)test.use({launchOptions:{executablePath:process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE}})
test('real Wave Warp editor validates scalar and color keys undo and actual preview',async({page,request})=>{
 const path=process.env.REAL_COMPOSITION_FIXTURE;if(!path)throw new Error('REAL_COMPOSITION_FIXTURE required')
 const registered=await request.post('/api/auth/register',{data:{username:'wave-warp-browser-user',password:'scope-browser-test-password'}});expect(registered.status()).toBe(201)
 const {token}=await registered.json() as {token:string}
 const uploaded=await request.post('/api/upload',{headers:{Authorization:`Bearer ${token}`},multipart:{file:{name:'key-fixture.mp4',mimeType:'video/mp4',buffer:await readFile(path)}}});expect(uploaded.ok()).toBe(true)
 const {id}=await uploaded.json() as {id:string};expect(id).toBeTruthy()
 const constant=(value:number)=>({mode:'constant',value})
 const content={schemaVersion:1,timeBase:1000000,canvas:{width:160,height:90,fpsMilli:10000,background:{red:0,green:0,blue:0,alpha:1}},sources:{[id]:{id,kind:'video',durationTicks:1000000,width:160,height:90,hasAudio:true}},tracks:[{kind:'video',id:'main',name:'Video',hidden:false,locked:false,clips:[{id:'clip',sourceId:id,placement:{timelineStartTick:0,sourceInTick:0,sourceOutTick:1000000,speed:1},transform:{x:constant(0),y:constant(0),scaleX:constant(1),scaleY:constant(1),rotationDegrees:constant(0),anchorX:0.5,anchorY:0.5},opacity:constant(1),blendMode:'normal',effects:[{kind:'color_lut',lut:{format:'cube',text:'LUT_3D_SIZE 2\n'+'0.5370987 0.735357 0.880825\n'.repeat(8),intensity:100}}],enabled:true}],transitions:[]}]}
 const props={token,project:{schemaVersion:1,root:'root',compositions:{root:{durationUs:1000000,content,instances:[]}}}}
 await page.addInitScript(value=>localStorage.setItem('video-kadr.live-wave-warp-test',JSON.stringify(value)),props)
 const errors:string[]=[];page.on('pageerror',error=>errors.push(error.message))
 await page.goto('/e2e/fixtures/live-wave-warp.html')
 await page.getByRole('button',{name:'Добавить Wave Warp',exact:true}).click()
 const history=page.getByLabel('Wave Warp history');await expect(history).toHaveText('1')
 const set=async(channel:string,value:string)=>page.getByLabel(`clip Wave Warp ${channel}`,{exact:true}).evaluate((element,value)=>{(element as HTMLInputElement).value=value;element.dispatchEvent(new Event('change',{bubbles:true}))},value)
 await set('height','10');await expect(history).toHaveText('2')
 await page.getByLabel('clip Wave Warp height key',{exact:true}).click();await expect(history).toHaveText('3')
 await set('width','40');await expect(history).toHaveText('4')
 await page.getByLabel('clip Wave Warp width key',{exact:true}).click();await expect(history).toHaveText('5')
 const saved=JSON.parse(await page.getByLabel('Wave Warp document').innerText())
 const key=page.getByLabel('clip Wave Warp width 500000 value',{exact:true})
 await key.locator('xpath=ancestor::details').locator('summary').click()
 await key.evaluate(element=>{(element as HTMLInputElement).value='90';element.dispatchEvent(new Event('change',{bubbles:true}))});await expect(history).toHaveText('6')
 await page.getByRole('button',{name:'Undo',exact:true}).click();await expect(history).toHaveText('5')
 expect(JSON.parse(await page.getByLabel('Wave Warp document').innerText())).toEqual(saved)
 await set('direction','-45');await expect(history).toHaveText('6')
 await page.getByLabel('clip Wave Warp direction key',{exact:true}).click();await expect(history).toHaveText('7')
 await set('speed','-1');await expect(history).toHaveText('8')
 await page.getByLabel('clip Wave Warp speed key',{exact:true}).click();await expect(history).toHaveText('9')
 const oracle=JSON.parse(await readFile('../backend/tests/fixtures/filmcraft-wave-warp.json','utf8')).browser as number[]
 const preview=page.waitForResponse(response=>new URL(response.url()).pathname==='/api/compositions/nested-preview')
 await page.getByRole('button',{name:'Native Preview',exact:true}).click();expect((await preview).status()).toBe(200)
 await expect.poll(()=>page.getByLabel('Wave Warp preview',{exact:true}).evaluate((element,expected)=>{
  const canvas=element as HTMLCanvasElement;if(canvas.width!==160||canvas.height!==90)return false
  const pixels=canvas.getContext('2d')!.getImageData(0,0,160,90).data
  for(let i=0;i<pixels.length;i++)if(Math.abs(pixels[i]! - expected[i]!)>1)return false
  return true
 },oracle)).toBe(true)
 await expect(page.getByLabel('Wave Warp error')).toHaveText('')
 expect(errors).toEqual([])
})

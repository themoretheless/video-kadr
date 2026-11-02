import {readFile} from 'node:fs/promises'
import {expect,test} from '@playwright/test'
if(process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE)test.use({launchOptions:{executablePath:process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE}})
test('real Strobe editor validates scalar and color keys undo and actual preview',async({page,request})=>{
 const path=process.env.REAL_COMPOSITION_FIXTURE;if(!path)throw new Error('REAL_COMPOSITION_FIXTURE required')
 const registered=await request.post('/api/auth/register',{data:{username:'strobe-browser-user',password:'scope-browser-test-password'}});expect(registered.status()).toBe(201)
 const {token}=await registered.json() as {token:string}
 const uploaded=await request.post('/api/upload',{headers:{Authorization:`Bearer ${token}`},multipart:{file:{name:'key-fixture.mp4',mimeType:'video/mp4',buffer:await readFile(path)}}});expect(uploaded.ok()).toBe(true)
 const {id}=await uploaded.json() as {id:string};expect(id).toBeTruthy()
 const constant=(value:number)=>({mode:'constant',value})
 const content={schemaVersion:1,timeBase:1000000,canvas:{width:160,height:90,fpsMilli:10000,background:{red:0,green:0,blue:0,alpha:1}},sources:{[id]:{id,kind:'video',durationTicks:1000000,width:160,height:90,hasAudio:true}},tracks:[{kind:'video',id:'main',name:'Video',hidden:false,locked:false,clips:[{id:'clip',sourceId:id,placement:{timelineStartTick:0,sourceInTick:0,sourceOutTick:1000000,speed:1},transform:{x:constant(0),y:constant(0),scaleX:constant(1),scaleY:constant(1),rotationDegrees:constant(0),anchorX:0.5,anchorY:0.5},opacity:constant(1),blendMode:'normal',effects:[{kind:'color_lut',lut:{format:'cube',text:'LUT_3D_SIZE 2\n'+'0.5370987 0.735357 0.880825\n'.repeat(8),intensity:100}}],enabled:true}],transitions:[]}]}
 const props={token,project:{schemaVersion:1,root:'root',compositions:{root:{durationUs:1000000,content,instances:[]}}}}
 await page.addInitScript(value=>localStorage.setItem('video-kadr.live-strobe-test',JSON.stringify(value)),props)
 const errors:string[]=[];page.on('pageerror',error=>errors.push(error.message))
 await page.goto('/e2e/fixtures/live-strobe.html')
 await page.getByRole('button',{name:'Добавить Strobe Light',exact:true}).click()
 const history=page.getByLabel('Strobe history');await expect(history).toHaveText('1')
 const set=async(channel:string,value:string)=>page.getByLabel(`clip Strobe ${channel}`,{exact:true}).evaluate((element,value)=>{(element as HTMLInputElement).value=value;element.dispatchEvent(new Event('change',{bubbles:true}))},value)
 await set('duration','0');await expect(history).toHaveText('2')
 await page.getByLabel('clip Strobe duration key',{exact:true}).click();await expect(history).toHaveText('3')
 await set('period','0.25');await expect(history).toHaveText('4')
 await page.getByLabel('clip Strobe period key',{exact:true}).click();await expect(history).toHaveText('5')
 const saved=JSON.parse(await page.getByLabel('Strobe document').innerText())
 expect(saved.compositions.root.content.tracks[0].clips[0].effects[1].animation.period.track.keyframes).toEqual([{tick:500000,value:0.25}])
 const key=page.getByLabel('clip Strobe period 500000 value',{exact:true})
 await key.locator('xpath=ancestor::details').locator('summary').click()
 await key.evaluate(element=>{(element as HTMLInputElement).value='0.5';element.dispatchEvent(new Event('change',{bubbles:true}))});await expect(history).toHaveText('6')
 await page.getByRole('button',{name:'Undo',exact:true}).click();await expect(history).toHaveText('5')
 expect(JSON.parse(await page.getByLabel('Strobe document').innerText())).toEqual(saved)
 const preview=page.waitForResponse(response=>new URL(response.url()).pathname==='/api/compositions/nested-preview')
 await page.getByRole('button',{name:'Native Preview',exact:true}).click();expect((await preview).status()).toBe(200)
 await expect.poll(()=>page.getByLabel('Strobe preview',{exact:true}).evaluate(element=>{
  const canvas=element as HTMLCanvasElement;if(canvas.width!==160||canvas.height!==90)return false
  const pixels=canvas.getContext('2d')!.getImageData(0,0,160,90).data
  for(let i=0;i<pixels.length;i+=4)if(pixels[i]!==137||pixels[i+1]!==188||pixels[i+2]!==225||pixels[i+3]!==255)return false
  return true
 })).toBe(true)
 for(const [index,channel] of ['colorR','colorG','colorB','colorA','blend'].entries()) {
  await page.getByLabel(`clip Strobe ${channel} key`,{exact:true}).click();await expect(history).toHaveText(String(6+index))
 }
 const durationKey=page.getByLabel('clip Strobe duration 500000 value',{exact:true})
 await durationKey.locator('xpath=ancestor::details').locator('summary').click()
 await durationKey.evaluate(element=>{(element as HTMLInputElement).value='1';element.dispatchEvent(new Event('change',{bubbles:true}))});await expect(history).toHaveText('11')
 const flash=page.waitForResponse(response=>new URL(response.url()).pathname==='/api/compositions/nested-preview')
 await page.getByRole('button',{name:'Native Preview',exact:true}).click();expect((await flash).status()).toBe(200)
 await expect.poll(()=>page.getByLabel('Strobe preview',{exact:true}).evaluate(element=>{
  const pixels=(element as HTMLCanvasElement).getContext('2d')!.getImageData(0,0,160,90).data
  for(let i=0;i<pixels.length;i+=4)if(pixels[i]!==255||pixels[i+1]!==255||pixels[i+2]!==255||pixels[i+3]!==255)return false
  return true
 })).toBe(true)
 await expect(page.getByLabel('Strobe error')).toHaveText('')
 expect(errors).toEqual([])
})

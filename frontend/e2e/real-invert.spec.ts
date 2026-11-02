import {readFile} from 'node:fs/promises'
import {expect,test} from '@playwright/test'
if(process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE)test.use({launchOptions:{executablePath:process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE}})
test('real Invert editor validates scalar keys and actual preview',async({page,request})=>{
 const path=process.env.REAL_COMPOSITION_FIXTURE;if(!path)throw new Error('REAL_COMPOSITION_FIXTURE required')
 const registered=await request.post('/api/auth/register',{data:{username:'invert-browser-user',password:'scope-browser-test-password'}});expect(registered.status()).toBe(201)
 const {token}=await registered.json() as {token:string}
 const uploaded=await request.post('/api/upload',{headers:{Authorization:`Bearer ${token}`},multipart:{file:{name:'invert-fixture.mp4',mimeType:'video/mp4',buffer:await readFile(path)}}});expect(uploaded.ok()).toBe(true)
 const {id}=await uploaded.json() as {id:string};expect(id).toBeTruthy()
 const constant=(value:number)=>({mode:'constant',value})
 const content={schemaVersion:1,timeBase:1000000,canvas:{width:160,height:90,fpsMilli:10000,background:{red:0,green:0,blue:0,alpha:1}},sources:{[id]:{id,kind:'video',durationTicks:1000000,width:160,height:90,hasAudio:true}},tracks:[{kind:'video',id:'main',name:'Video',hidden:false,locked:false,clips:[{id:'clip',sourceId:id,placement:{timelineStartTick:0,sourceInTick:0,sourceOutTick:1000000,speed:1},transform:{x:constant(0),y:constant(0),scaleX:constant(1),scaleY:constant(1),rotationDegrees:constant(0),anchorX:0.5,anchorY:0.5},opacity:constant(1),blendMode:'normal',effects:[{kind:'color_lut',lut:{format:'cube',text:'LUT_3D_SIZE 2\n'+'0.5370987 0.735357 0.880825\n'.repeat(8),intensity:100}}],enabled:true}],transitions:[]}]}
 const props={token,project:{schemaVersion:1,root:'root',compositions:{root:{durationUs:1000000,content,instances:[]}}}}
 await page.addInitScript(value=>localStorage.setItem('video-kadr.live-invert-test',JSON.stringify(value)),props)
 const errors:string[]=[];page.on('pageerror',error=>errors.push(error.message))
 await page.goto('/e2e/fixtures/live-invert.html')
 await page.getByRole('button',{name:'Добавить Invert',exact:true}).click()
 const history=page.getByLabel('Invert history');await expect(history).toHaveText('1')
 const invert=page.getByLabel('clip Invert blend',{exact:true})
 await expect(invert).toHaveValue('0')
 await invert.fill('0');await invert.dispatchEvent('change');await expect(history).toHaveText('2')
 await page.getByLabel('clip Invert blend ключ',{exact:true}).click();await expect(history).toHaveText('3')
 const data=JSON.parse(await page.getByLabel('Invert document').innerText())
 const effect=data.compositions.root.content.tracks[0].clips[0].effects.find((effect:{kind:string})=>effect.kind==='color_invert')
 expect(effect.parameters).toEqual({blend:0})
 expect(effect.animation.track.keyframes).toEqual([{tick:500000,value:0}])
 await page.getByText('Смешивание · 1 ключей',{exact:true}).click()
 const value=page.getByLabel('clip Invert 500000 value',{exact:true})
 await value.fill('50');await value.dispatchEvent('change');await expect(history).toHaveText('4')
 await page.getByRole('button',{name:'Undo',exact:true}).click();await expect(history).toHaveText('3');await expect(value).toHaveValue('0')
 await page.getByRole('button',{name:'Redo',exact:true}).click();await expect(history).toHaveText('4');await expect(value).toHaveValue('50')
 await expect(page.getByLabel('Invert error')).toHaveText('')
 const preview=page.waitForResponse(response=>new URL(response.url()).pathname==='/api/compositions/nested-preview')
 await page.getByRole('button',{name:'Native Preview',exact:true}).click();expect((await preview).status()).toBe(200)
 await expect(page.getByLabel('Invert preview error')).toHaveText('')
 await expect.poll(()=>page.getByLabel('Invert preview',{exact:true}).evaluate(element=>{
  const canvas=element as HTMLCanvasElement
  if(canvas.width!==160||canvas.height!==90)return false
  const pixels=canvas.getContext('2d')!.getImageData(0,0,160,90).data
  for(let i=0;i<pixels.length;i+=4)if(pixels[i]!==128||pixels[i+1]!==128||pixels[i+2]!==128||pixels[i+3]!==255)return false
  return true
 })).toBe(true)
 await page.getByLabel('clip Invert channel',{exact:true}).selectOption('4');await expect(history).toHaveText('5')
 const alphaDocument=JSON.parse(await page.getByLabel('Invert document').innerText())
 expect(alphaDocument.compositions.root.content.tracks[0].clips[0].effects[1]).toMatchObject({kind:'color_invert',parameters:{channel:4,blend:0},animation:{track:{keyframes:[{tick:500000,value:50}]}}})
 const alphaPreview=page.waitForResponse(response=>new URL(response.url()).pathname==='/api/compositions/nested-preview')
 await page.getByRole('button',{name:'Native Preview',exact:true}).click();expect((await alphaPreview).status()).toBe(200)
 await expect.poll(()=>page.getByLabel('Invert preview',{exact:true}).evaluate(element=>{
  const pixels=(element as HTMLCanvasElement).getContext('2d')!.getImageData(0,0,160,90).data
  for(let i=0;i<pixels.length;i+=4)if(pixels[i]!==0||pixels[i+1]!==0||pixels[i+2]!==0||pixels[i+3]!==255)return false
  return true
 })).toBe(true)
 await page.getByRole('button',{name:'Undo',exact:true}).click();await expect(history).toHaveText('4');await expect(page.getByLabel('clip Invert channel',{exact:true})).toHaveValue('0')
 expect(errors).toEqual([])
 if(process.env.GAMMA_SCREENSHOT_PATH)await page.screenshot({path:process.env.GAMMA_SCREENSHOT_PATH,fullPage:true})
})

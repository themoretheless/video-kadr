import {readFile} from 'node:fs/promises'
import {expect,test} from '@playwright/test'
if(process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE)test.use({launchOptions:{executablePath:process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE}})
test('real Extract editor validates scalar keys and actual preview',async({page,request})=>{
 const path=process.env.REAL_COMPOSITION_FIXTURE;if(!path)throw new Error('REAL_COMPOSITION_FIXTURE required')
 const registered=await request.post('/api/auth/register',{data:{username:'extract-browser-user',password:'scope-browser-test-password'}});expect(registered.status()).toBe(201)
 const {token}=await registered.json() as {token:string}
 const uploaded=await request.post('/api/upload',{headers:{Authorization:`Bearer ${token}`},multipart:{file:{name:'extract-fixture.mp4',mimeType:'video/mp4',buffer:await readFile(path)}}});expect(uploaded.ok()).toBe(true)
 const {id}=await uploaded.json() as {id:string};expect(id).toBeTruthy()
 const constant=(value:number)=>({mode:'constant',value})
 const content={schemaVersion:1,timeBase:1000000,canvas:{width:160,height:90,fpsMilli:10000,background:{red:0,green:0,blue:0,alpha:1}},sources:{[id]:{id,kind:'video',durationTicks:1000000,width:160,height:90,hasAudio:true}},tracks:[{kind:'video',id:'main',name:'Video',hidden:false,locked:false,clips:[{id:'clip',sourceId:id,placement:{timelineStartTick:0,sourceInTick:0,sourceOutTick:1000000,speed:1},transform:{x:constant(0),y:constant(0),scaleX:constant(1),scaleY:constant(1),rotationDegrees:constant(0),anchorX:0.5,anchorY:0.5},opacity:constant(1),blendMode:'normal',effects:[{kind:'color_lut',lut:{format:'cube',text:'LUT_3D_SIZE 2\n'+'0.5370987 0.735357 0.880825\n'.repeat(8),intensity:100}}],enabled:true}],transitions:[]}]}
 const props={token,project:{schemaVersion:1,root:'root',compositions:{root:{durationUs:1000000,content,instances:[]}}}}
 await page.addInitScript(value=>localStorage.setItem('video-kadr.live-extract-test',JSON.stringify(value)),props)
 const errors:string[]=[];page.on('pageerror',error=>errors.push(error.message))
 await page.goto('/e2e/fixtures/live-extract.html')
 await page.getByRole('button',{name:'Добавить Extract',exact:true}).click()
 const history=page.getByLabel('Extract history');await expect(history).toHaveText('1')
 const extract=page.getByLabel('clip Extract black',{exact:true})
 await expect(extract).toHaveValue('0')
 await extract.fill('0');await extract.dispatchEvent('change');await expect(history).toHaveText('2')
 await page.getByLabel('clip Extract black key',{exact:true}).click();await expect(history).toHaveText('3')
 const data=JSON.parse(await page.getByLabel('Extract document').innerText())
 const effect=data.compositions.root.content.tracks[0].clips[0].effects.find((effect:{kind:string})=>effect.kind==='color_extract')
 expect(effect.parameters).toEqual({black:0})
 expect(effect.animation.black.track.keyframes).toEqual([{tick:500000,value:0}])
 await page.getByText('Чёрный порог · 1 ключей',{exact:true}).click()
 const value=page.getByLabel('clip Extract black 500000 value',{exact:true})
 await value.fill('255');await value.dispatchEvent('change');await expect(history).toHaveText('4')
 await page.getByRole('button',{name:'Undo',exact:true}).click();await expect(history).toHaveText('3');await expect(value).toHaveValue('0')
 await page.getByRole('button',{name:'Redo',exact:true}).click();await expect(history).toHaveText('4');await expect(value).toHaveValue('255')
 await expect(page.getByLabel('Extract error')).toHaveText('')
 const preview=page.waitForResponse(response=>new URL(response.url()).pathname==='/api/compositions/nested-preview')
 await page.getByRole('button',{name:'Native Preview',exact:true}).click();expect((await preview).status()).toBe(200)
 await expect(page.getByLabel('Extract preview error')).toHaveText('')
 await expect.poll(()=>page.getByLabel('Extract preview',{exact:true}).evaluate(element=>{
  const canvas=element as HTMLCanvasElement
  if(canvas.width!==160||canvas.height!==90)return false
  const pixels=canvas.getContext('2d')!.getImageData(0,0,160,90).data
  for(let i=0;i<pixels.length;i+=4)if(pixels[i]!==0||pixels[i+1]!==0||pixels[i+2]!==0||pixels[i+3]!==255)return false
  return true
 })).toBe(true)
 const invert=page.getByLabel('clip Extract invert',{exact:true})
 await invert.check();await expect(history).toHaveText('5')
 const inversePreview=page.waitForResponse(response=>new URL(response.url()).pathname==='/api/compositions/nested-preview')
 await page.getByRole('button',{name:'Native Preview',exact:true}).click();expect((await inversePreview).status()).toBe(200)
 await expect.poll(()=>page.getByLabel('Extract preview',{exact:true}).evaluate(element=>{
  const pixels=(element as HTMLCanvasElement).getContext('2d')!.getImageData(0,0,160,90).data
  for(let i=0;i<pixels.length;i+=4)if(pixels[i]!==255||pixels[i+1]!==255||pixels[i+2]!==255||pixels[i+3]!==255)return false
  return true
 })).toBe(true)
 await page.getByRole('button',{name:'Undo',exact:true}).click();await expect(history).toHaveText('4');await expect(invert).not.toBeChecked()
 expect(errors).toEqual([])
})

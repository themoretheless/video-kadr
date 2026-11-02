import {readFile} from 'node:fs/promises'
import {expect,test} from '@playwright/test'
if(process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE)test.use({launchOptions:{executablePath:process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE}})
test('real Gamma editor validates scalar keys and actual preview',async({page,request})=>{
 const path=process.env.REAL_COMPOSITION_FIXTURE;if(!path)throw new Error('REAL_COMPOSITION_FIXTURE required')
 const registered=await request.post('/api/auth/register',{data:{username:'gamma-browser-user',password:'scope-browser-test-password'}});expect(registered.status()).toBe(201)
 const {token}=await registered.json() as {token:string}
 const uploaded=await request.post('/api/upload',{headers:{Authorization:`Bearer ${token}`},multipart:{file:{name:'gamma-fixture.mp4',mimeType:'video/mp4',buffer:await readFile(path)}}});expect(uploaded.ok()).toBe(true)
 const {id}=await uploaded.json() as {id:string};expect(id).toBeTruthy()
 const constant=(value:number)=>({mode:'constant',value})
 const content={schemaVersion:1,timeBase:1000000,canvas:{width:160,height:90,fpsMilli:10000,background:{red:0,green:0,blue:0,alpha:1}},sources:{[id]:{id,kind:'video',durationTicks:1000000,width:160,height:90,hasAudio:true}},tracks:[{kind:'video',id:'main',name:'Video',hidden:false,locked:false,clips:[{id:'clip',sourceId:id,placement:{timelineStartTick:0,sourceInTick:0,sourceOutTick:1000000,speed:1},transform:{x:constant(0),y:constant(0),scaleX:constant(1),scaleY:constant(1),rotationDegrees:constant(0),anchorX:0.5,anchorY:0.5},opacity:constant(1),blendMode:'normal',effects:[{kind:'color_lut',lut:{format:'cube',text:'LUT_3D_SIZE 2\n'+'0.5370987 0.735357 0.880825\n'.repeat(8),intensity:100}}],enabled:true}],transitions:[]}]}
 const props={token,project:{schemaVersion:1,root:'root',compositions:{root:{durationUs:1000000,content,instances:[]}}}}
 await page.addInitScript(value=>localStorage.setItem('video-kadr.live-gamma-test',JSON.stringify(value)),props)
 const errors:string[]=[];page.on('pageerror',error=>errors.push(error.message))
 await page.goto('/e2e/fixtures/live-gamma.html')
 await page.getByRole('button',{name:'Добавить Gamma Correction',exact:true}).click()
 const history=page.getByLabel('Gamma history');await expect(history).toHaveText('1')
 const gamma=page.getByLabel('clip Gamma Correction gamma',{exact:true})
 await expect(gamma).toHaveValue('10')
 await gamma.fill('28');await gamma.dispatchEvent('change');await expect(history).toHaveText('2')
 await page.getByLabel('clip Gamma Correction gamma ключ',{exact:true}).click();await expect(history).toHaveText('3')
 const data=JSON.parse(await page.getByLabel('Gamma document').innerText())
 const effect=data.compositions.root.content.tracks[0].clips[0].effects.find((effect:{kind:string})=>effect.kind==='gamma_correction')
 expect(effect.parameters).toEqual({gamma:28})
 expect(effect.animation.track.keyframes).toEqual([{tick:500000,value:28}])
 await page.getByText('Гамма · 1 ключей',{exact:true}).click()
 const value=page.getByLabel('clip Gamma Correction 500000 value',{exact:true})
 await value.fill('14.5');await value.dispatchEvent('change');await expect(history).toHaveText('4')
 await page.getByRole('button',{name:'Undo',exact:true}).click();await expect(history).toHaveText('3');await expect(value).toHaveValue('28')
 await page.getByRole('button',{name:'Redo',exact:true}).click();await expect(history).toHaveText('4');await expect(value).toHaveValue('14.5')
 await expect(page.getByLabel('Gamma error')).toHaveText('')
 const preview=page.waitForResponse(response=>new URL(response.url()).pathname==='/api/compositions/nested-preview')
 await page.getByRole('button',{name:'Native Preview',exact:true}).click();expect((await preview).status()).toBe(200)
 await expect(page.getByLabel('Gamma preview error')).toHaveText('')
 await expect.poll(()=>page.getByLabel('Gamma preview',{exact:true}).evaluate(element=>{
  const canvas=element as HTMLCanvasElement
  if(canvas.width!==160||canvas.height!==90)return false
  const pixels=canvas.getContext('2d')!.getImageData(0,0,160,90).data
  for(let i=0;i<pixels.length;i+=4)if(pixels[i]!==104||pixels[i+1]!==163||pixels[i+2]!==212||pixels[i+3]!==255)return false
  return true
 })).toBe(true)
 expect(errors).toEqual([])
 if(process.env.GAMMA_SCREENSHOT_PATH)await page.screenshot({path:process.env.GAMMA_SCREENSHOT_PATH,fullPage:true})
})

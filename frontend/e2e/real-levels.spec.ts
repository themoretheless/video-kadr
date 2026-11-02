import {readFile} from 'node:fs/promises'
import {expect,test} from '@playwright/test'
if(process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE)test.use({launchOptions:{executablePath:process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE}})
test('real Levels editor validates scalar keys and actual preview',async({page,request})=>{
 const path=process.env.REAL_COMPOSITION_FIXTURE;if(!path)throw new Error('REAL_COMPOSITION_FIXTURE required')
 const registered=await request.post('/api/auth/register',{data:{username:'levels-browser-user',password:'scope-browser-test-password'}});expect(registered.status()).toBe(201)
 const {token}=await registered.json() as {token:string}
 const uploaded=await request.post('/api/upload',{headers:{Authorization:`Bearer ${token}`},multipart:{file:{name:'levels-fixture.mp4',mimeType:'video/mp4',buffer:await readFile(path)}}});expect(uploaded.ok()).toBe(true)
 const {id}=await uploaded.json() as {id:string};expect(id).toBeTruthy()
 const constant=(value:number)=>({mode:'constant',value})
 const content={schemaVersion:1,timeBase:1000000,canvas:{width:160,height:90,fpsMilli:10000,background:{red:0,green:0,blue:0,alpha:1}},sources:{[id]:{id,kind:'video',durationTicks:1000000,width:160,height:90,hasAudio:true}},tracks:[{kind:'video',id:'main',name:'Video',hidden:false,locked:false,clips:[{id:'clip',sourceId:id,placement:{timelineStartTick:0,sourceInTick:0,sourceOutTick:1000000,speed:1},transform:{x:constant(0),y:constant(0),scaleX:constant(1),scaleY:constant(1),rotationDegrees:constant(0),anchorX:0.5,anchorY:0.5},opacity:constant(1),blendMode:'normal',effects:[{kind:'color_lut',lut:{format:'cube',text:'LUT_3D_SIZE 2\n'+'0.5370987 0.735357 0.880825\n'.repeat(8),intensity:100}}],enabled:true}],transitions:[]}]}
 const props={token,project:{schemaVersion:1,root:'root',compositions:{root:{durationUs:1000000,content,instances:[]}}}}
 await page.addInitScript(value=>localStorage.setItem('video-kadr.live-levels-test',JSON.stringify(value)),props)
 const errors:string[]=[];page.on('pageerror',error=>errors.push(error.message))
 await page.goto('/e2e/fixtures/live-levels.html')
 await page.getByRole('button',{name:'Добавить Levels',exact:true}).click()
 const history=page.getByLabel('Levels history');await expect(history).toHaveText('1')
 const levels=page.getByLabel('clip Levels gamma',{exact:true})
 await expect(levels).toHaveValue('100')
 await levels.fill('100');await levels.dispatchEvent('change');await expect(history).toHaveText('2')
 await page.getByLabel('clip Levels gamma key',{exact:true}).click();await expect(history).toHaveText('3')
 const data=JSON.parse(await page.getByLabel('Levels document').innerText())
 const effect=data.compositions.root.content.tracks[0].clips[0].effects.find((effect:{kind:string})=>effect.kind==='color_levels')
 expect(effect.parameters).toEqual({gamma:100})
 expect(effect.animation.gamma.track.keyframes).toEqual([{tick:500000,value:100}])
 await page.getByText('Гамма · 1 ключей',{exact:true}).click()
 const value=page.getByLabel('clip Levels gamma 500000 value',{exact:true})
 await value.fill('200');await value.dispatchEvent('change');await expect(history).toHaveText('4')
 await page.getByRole('button',{name:'Undo',exact:true}).click();await expect(history).toHaveText('3');await expect(value).toHaveValue('100')
 await page.getByRole('button',{name:'Redo',exact:true}).click();await expect(history).toHaveText('4');await expect(value).toHaveValue('200')
 await expect(page.getByLabel('Levels error')).toHaveText('')
 const preview=page.waitForResponse(response=>new URL(response.url()).pathname==='/api/compositions/nested-preview')
 await page.getByRole('button',{name:'Native Preview',exact:true}).click();expect((await preview).status()).toBe(200)
 await expect(page.getByLabel('Levels preview error')).toHaveText('')
 await expect.poll(()=>page.getByLabel('Levels preview',{exact:true}).evaluate(element=>{
  const canvas=element as HTMLCanvasElement
  if(canvas.width!==160||canvas.height!==90)return false
  const pixels=canvas.getContext('2d')!.getImageData(0,0,160,90).data
  for(let i=0;i<pixels.length;i+=4)if(pixels[i]!==187||pixels[i+1]!==219||pixels[i+2]!==239||pixels[i+3]!==255)return false
  return true
 })).toBe(true)
 for(const [channel,value,count] of [['outBlack','255','5'],['outWhite','0','6']] as const){
  const input=page.getByLabel(`clip Levels ${channel}`,{exact:true});await input.fill(value);await input.dispatchEvent('change');await expect(history).toHaveText(count)
 }
 const reversed=JSON.parse(await page.getByLabel('Levels document').innerText()).compositions.root.content.tracks[0].clips[0].effects[1]
 expect(reversed.parameters).toEqual({gamma:100,outBlack:255,outWhite:0})
 expect(reversed.animation.gamma.track.keyframes).toEqual([{tick:500000,value:200}])
 const reversedPreview=page.waitForResponse(response=>new URL(response.url()).pathname==='/api/compositions/nested-preview')
 await page.getByRole('button',{name:'Native Preview',exact:true}).click();expect((await reversedPreview).status()).toBe(200)
 await expect.poll(()=>page.getByLabel('Levels preview',{exact:true}).evaluate(element=>{
  const pixels=(element as HTMLCanvasElement).getContext('2d')!.getImageData(0,0,160,90).data
  for(let i=0;i<pixels.length;i+=4)if(pixels[i]!==68||pixels[i+1]!==36||pixels[i+2]!==16||pixels[i+3]!==255)return false
  return true
 })).toBe(true)
 for(const count of ['5','4']){await page.getByRole('button',{name:'Undo',exact:true}).click();await expect(history).toHaveText(count)}
 await expect(page.getByLabel('clip Levels outBlack',{exact:true})).toHaveValue('0')
 await expect(page.getByLabel('clip Levels outWhite',{exact:true})).toHaveValue('255')
 expect(errors).toEqual([])
 if(process.env.GAMMA_SCREENSHOT_PATH)await page.screenshot({path:process.env.GAMMA_SCREENSHOT_PATH,fullPage:true})
})

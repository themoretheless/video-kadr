import {readFile} from 'node:fs/promises'
import {expect,test} from '@playwright/test'
if(process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE)test.use({launchOptions:{executablePath:process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE}})
test('real Video Limiter editor validates changes and RGBA keys',async({page,request})=>{
 const path=process.env.REAL_COMPOSITION_FIXTURE;if(!path)throw new Error('REAL_COMPOSITION_FIXTURE required')
 const registered=await request.post('/api/auth/register',{data:{username:'limiter-browser-user',password:'scope-browser-test-password'}});expect(registered.status()).toBe(201)
 const {token}=await registered.json() as {token:string}
 const uploaded=await request.post('/api/upload',{headers:{Authorization:`Bearer ${token}`},multipart:{file:{name:'limiter-fixture.mp4',mimeType:'video/mp4',buffer:await readFile(path)}}});expect(uploaded.ok()).toBe(true)
 const {id}=await uploaded.json() as {id:string};expect(id).toBeTruthy()
 const constant=(value:number)=>({mode:'constant',value})
 const content={schemaVersion:1,timeBase:1000000,canvas:{width:160,height:90,fpsMilli:10000,background:{red:0,green:0,blue:0,alpha:1}},sources:{[id]:{id,kind:'video',durationTicks:1000000,width:160,height:90,hasAudio:true}},tracks:[{kind:'video',id:'main',name:'Video',hidden:false,locked:false,clips:[{id:'clip',sourceId:id,placement:{timelineStartTick:0,sourceInTick:0,sourceOutTick:1000000,speed:1},transform:{x:constant(0),y:constant(0),scaleX:constant(1),scaleY:constant(1),rotationDegrees:constant(0),anchorX:0.5,anchorY:0.5},opacity:constant(1),blendMode:'normal',effects:[{kind:'color_lut',lut:{format:'cube',text:'LUT_3D_SIZE 2\n'+'1 1 1\n'.repeat(8),intensity:100}}],enabled:true}],transitions:[]}]}
 const props={token,project:{schemaVersion:1,root:'root',compositions:{root:{durationUs:1000000,content,instances:[]}}}}
 await page.addInitScript(value=>localStorage.setItem('video-kadr.live-limiter-test',JSON.stringify(value)),props)
 const errors:string[]=[];page.on('pageerror',error=>errors.push(error.message))
 await page.goto('/e2e/fixtures/live-limiter.html')
 await page.getByRole('button',{name:'Добавить Video Limiter',exact:true}).click()
 const history=page.getByLabel('Limiter history')
 await expect(history).toHaveText('1')
 await page.getByLabel('clip Video Limiter clipLevel',{exact:true}).selectOption('9');await expect(history).toHaveText('2')
 await page.getByLabel('clip Video Limiter compression',{exact:true}).selectOption('4');await expect(history).toHaveText('3')
 await page.getByLabel('clip Video Limiter axis',{exact:true}).selectOption('2');await expect(history).toHaveText('4')
 await page.getByLabel('clip Video Limiter warning',{exact:true}).check();await expect(history).toHaveText('5')
 await page.getByLabel('clip Video Limiter color',{exact:true}).fill('#00ff00');await expect(history).toHaveText('6')
 await page.getByLabel('clip Video Limiter color key',{exact:true}).click();await expect(history).toHaveText('7')
 const data=JSON.parse(await page.getByLabel('Limiter document').innerText())
 const effect=data.compositions.root.content.tracks[0].clips[0].effects.find((effect:{kind:string})=>effect.kind==='video_limiter')
 expect(effect.parameters).toEqual({clipLevel:9,compression:4,axis:2,gamutWarning:true,warningColor:[0,1,0,1]})
 expect(effect.animation.green.track.keyframes).toEqual([{tick:500000,value:1}])
 await page.getByText('green · 1 ключей',{exact:true}).click()
 const value=page.getByLabel('clip Video Limiter green 500000 value',{exact:true})
 await value.fill('0.4');await value.dispatchEvent('change');await expect(history).toHaveText('8')
 await page.getByRole('button',{name:'Undo',exact:true}).click();await expect(history).toHaveText('7')
 await expect(value).toHaveValue('1')
 await page.getByRole('button',{name:'Redo',exact:true}).click();await expect(history).toHaveText('8')
 await expect(value).toHaveValue('0.4')
 await expect(page.getByLabel('Limiter error')).toHaveText('')
 const preview=page.waitForResponse(response=>new URL(response.url()).pathname==='/api/compositions/nested-preview')
 await page.getByRole('button',{name:'Native Preview',exact:true}).click()
 expect((await preview).status()).toBe(200)
 await expect(page.getByLabel('Limiter preview error')).toHaveText('')
 await expect.poll(()=>page.getByLabel('Limiter preview',{exact:true}).evaluate(element=>{
  const canvas=element as HTMLCanvasElement
  if(canvas.width!==160||canvas.height!==90)return false
  const pixels=canvas.getContext('2d')!.getImageData(0,0,160,90).data
  for(let i=0;i<pixels.length;i+=4)if(pixels[i]!==0||pixels[i+1]!==102||pixels[i+2]!==0||pixels[i+3]!==255)return false
  return true
 })).toBe(true)
 expect(errors).toEqual([])
 if(process.env.LIMITER_SCREENSHOT_PATH)await page.screenshot({path:process.env.LIMITER_SCREENSHOT_PATH})
})

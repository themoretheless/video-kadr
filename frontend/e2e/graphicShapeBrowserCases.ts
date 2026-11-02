import {readFile} from 'node:fs/promises'
import {spawnSync} from 'node:child_process'
import {expect,test} from '@playwright/test'
if(process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE)test.use({launchOptions:{executablePath:process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE}})
export function graphicShapeCases(appearance:number):void {
 for(let shape=0;shape<4;shape++)for(let profile=0;profile<3;profile++)test(`GraphicShape appearance${appearance} shape${shape} profile${profile} all42keys undo original preview`,async({page,request})=>{
  const fixture=JSON.parse(await readFile('../backend/tests/fixtures/filmcraft-graphic-shape.json','utf8'))
  const original=fixture.cases.find((c:any)=>c.enabled&&c.appearance===appearance&&c.shape===shape&&c.profile===profile);expect(original).toBeTruthy()
  const parameters=Object.fromEntries(Object.entries(original.effect.params).map(([name,p]:[string,any])=>{const [kind,value]=Object.entries(p.value)[0]!;return [name.replace(/_([a-z])/g,(_,c)=>c.toUpperCase()),kind==='Vec2'?[(value as any).x,(value as any).y]:value]}))
  const png=spawnSync('ffmpeg',['-v','error','-f','rawvideo','-pixel_format','rgba','-video_size','64x48','-i','pipe:0','-frames:v','1','-f','image2pipe','-vcodec','png','pipe:1'],{input:Buffer.alloc(64*48*4)});expect(png.status).toBe(0)
  const registered=await request.post('/api/auth/register',{data:{username:`graphic-shape-${appearance}-${shape}-${profile}`,password:'scope-browser-test-password'}});expect(registered.status()).toBe(201)
  const {token}=await registered.json() as {token:string}
  const uploaded=await request.post('/api/upload',{headers:{Authorization:`Bearer ${token}`},multipart:{file:{name:'graphic-shape.png',mimeType:'image/png',buffer:png.stdout}}});expect(uploaded.ok()).toBe(true);const{id}=await uploaded.json()
  const constant=(value:number)=>({mode:'constant',value})
  const content={schemaVersion:1,timeBase:1000000,canvas:{width:64,height:48,fpsMilli:24000,background:{red:0,green:0,blue:0,alpha:1}},sources:{[id]:{id,kind:'image',durationTicks:0,width:64,height:48,hasAudio:false}},tracks:[{kind:'image',id:'main',name:'Image',hidden:false,locked:false,clips:[{id:'clip',sourceId:id,timelineStartTick:1000000,durationTicks:1000000,transform:{x:constant(0),y:constant(0),scaleX:constant(1),scaleY:constant(1),rotationDegrees:constant(0),anchorX:0.5,anchorY:0.5},opacity:constant(1),blendMode:'normal',effects:[{kind:'graphic_shape',parameters}],enabled:true}]}]}
  const props={token,project:{schemaVersion:1,root:'root',compositions:{root:{durationUs:2500000,content,instances:[]}}}}
  await page.addInitScript(value=>localStorage.setItem('video-kadr.live-graphic-shape-test',JSON.stringify(value)),props)
  const errors:string[]=[];page.on('pageerror',error=>errors.push(error.message));await page.goto('/e2e/fixtures/live-graphic-shape.html')
  const history=page.getByLabel('GraphicShape history');await expect(history).toHaveText('0')
  const keys=page.locator('button[aria-label^="clip GraphicShape "][aria-label$=" key"]');expect(await keys.count()).toBe(42)
  for(let i=0;i<42;i++){await keys.nth(i).click();await expect(history).toHaveText(String(i+1))}
  const saved=JSON.parse(await page.getByLabel('GraphicShape document').innerText());const effect=saved.compositions.root.content.tracks[0].clips[0].effects[0];expect(Object.keys(effect.animation)).toHaveLength(42)
  for(const channel of ['fillG','shadowOpacity']){
   const input=page.getByLabel(`clip GraphicShape ${channel}`,{exact:true});await input.evaluate(e=>{(e as HTMLInputElement).value='0.125';e.dispatchEvent(new Event('change',{bubbles:true}))});await expect(history).toHaveText('43');await page.getByRole('button',{name:'Undo',exact:true}).click();await expect(history).toHaveText('42');expect(JSON.parse(await page.getByLabel('GraphicShape document').innerText())).toEqual(saved)
  }
  const key=page.getByLabel('clip GraphicShape fillG 500000 value',{exact:true});await key.locator('xpath=ancestor::details').locator('summary').click();await key.evaluate(e=>{(e as HTMLInputElement).value='0.125';e.dispatchEvent(new Event('change',{bubbles:true}))});await expect(history).toHaveText('43');await page.getByRole('button',{name:'Undo',exact:true}).click();await expect(history).toHaveText('42');expect(JSON.parse(await page.getByLabel('GraphicShape document').innerText())).toEqual(saved)
  await page.getByLabel('clip GraphicShape shape',{exact:true}).selectOption(String((shape+1)%4));await expect(history).toHaveText('43');await page.getByRole('button',{name:'Undo',exact:true}).click();await expect(history).toHaveText('42');expect(JSON.parse(await page.getByLabel('GraphicShape document').innerText())).toEqual(saved)
  const f32=(bits:number)=>new Float32Array(new Uint32Array([bits]).buffer)[0]!
  const oracle:number[]=[];for(let i=0;i<original.bits.length;i+=4){const a=f32(original.bits[i+3]);const alpha=Math.max(0,Math.min(255,Math.round(a*255+1e-4)));for(let c=0;c<3;c++){const v=a<=1e-6?0:Math.max(0,f32(original.bits[i+c])/a);const d=v<=0.0031308?v*12.92:1.055*Math.pow(v,1/2.4)-0.055;oracle.push(Math.round(Math.max(0,Math.min(255,Math.round(d*255+1e-4)))*alpha/255))}oracle.push(255)}
  const preview=page.waitForResponse(r=>new URL(r.url()).pathname==='/api/compositions/nested-preview');await page.getByRole('button',{name:'Native Preview',exact:true}).click();const response=await preview;expect(response.status(),await response.text()).toBe(200)
  await expect.poll(()=>page.getByLabel('GraphicShape preview',{exact:true}).evaluate((element,expected)=>{const canvas=element as HTMLCanvasElement;if(canvas.width!==64||canvas.height!==48)return false;const pixels=canvas.getContext('2d')!.getImageData(0,0,64,48).data;return pixels.every((v,i)=>Math.abs(v-expected[i]!)<=1)},oracle)).toBe(true)
  await expect(page.getByLabel('GraphicShape error')).toHaveText('');await expect(page.getByLabel('GraphicShape preview error')).toHaveText('');expect(errors).toEqual([])
  if(shape===0&&profile===0)await page.screenshot({path:`/tmp/video-kadr-stage1410-shape-${appearance}.png`,fullPage:true})
 })
}

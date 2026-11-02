import {spawnSync} from 'node:child_process'
import {mkdtempSync,readFileSync,rmSync} from 'node:fs'
import {tmpdir} from 'node:os'
import {join} from 'node:path'
import {expect,test} from '@playwright/test'
test('spherical video native preview, camera keys, orientation import and undo',async({page,request})=>{
 const pixels=Buffer.from(Array.from({length:64*32},(_,i)=>[i%64*4,Math.floor(i/64)*8,60,255]).flat())
 const encoded=spawnSync('ffmpeg',['-v','error','-f','rawvideo','-pixel_format','rgba','-video_size','64x32','-framerate','4','-i','pipe:0','-c:v','ffv1','-pix_fmt','bgra','-f','matroska','pipe:1'],{input:Buffer.concat([pixels,pixels,pixels,pixels])});expect(encoded.status).toBe(0)
 const varint=(n:number)=>{const bytes:number[]=[];do{const next=n%128;n=Math.floor(n/128);bytes.push(next|(n?128:0))}while(n);return Buffer.from(bytes)}
 const integer=(field:number,value:number)=>Buffer.concat([varint(field*8),varint(value)])
 const bytes=(field:number,value:Buffer)=>Buffer.concat([varint(field*8+2),varint(value.length),value])
 const metadata=Buffer.concat([bytes(1,Buffer.from('secret')),bytes(2,Buffer.from('Insta360 X6')),bytes(3,Buffer.from('v0.1')),integer(24,1000000),integer(51,1),integer(62,1),bytes(65,Buffer.concat([integer(1,32),integer(2,2000)]))])
 const gyro=Buffer.alloc(142*20)
 for(let i=0;i<142;i++){const at=i*20;gyro.writeBigUInt64LE(BigInt(800000+i*10000),at);[32768,33792,32768,32768,34243,32768].forEach((value,j)=>gyro.writeUInt16LE(value,at+8+j*2))}
 const block=(kind:number,format:number,payload:Buffer)=>{const suffix=Buffer.alloc(6);suffix[0]=format;suffix[1]=kind;suffix.writeUInt32LE(payload.length,2);return Buffer.concat([payload,suffix])}
 const metadataBlock=block(1,1,metadata),gyroBlock=block(3,0,gyro)
 const index=Buffer.alloc(40);index[10]=1;index[11]=1;index.writeUInt32LE(metadata.length,12);index[30]=3;index.writeUInt32LE(gyro.length,32);index.writeUInt32LE(metadataBlock.length,36)
 const indexHeader=Buffer.alloc(6);indexHeader.writeUInt32LE(index.length,2)
 const footer=Buffer.alloc(72);footer.writeUInt32LE(metadataBlock.length+gyroBlock.length+index.length+6+72,32);footer.writeUInt32LE(3,36);footer.write('8db42d694ccc418790edff439fe026bf',40)
 const input=Buffer.concat([encoded.stdout,metadataBlock,gyroBlock,index,indexHeader,footer])
 const registered=await request.post('/api/auth/register',{data:{username:'spherical-browser-test',password:'scope-browser-test-password'}});expect(registered.status()).toBe(201)
 const {token}=await registered.json()
 const upload=await request.post('/api/upload',{headers:{Authorization:`Bearer ${token}`},multipart:{file:{name:'panorama.mkv',mimeType:'video/x-matroska',buffer:input}}});expect(upload.ok()).toBe(true)
 const {id}=await upload.json()
 const info=await request.get(`/api/library/${id}/insta360`,{headers:{Authorization:`Bearer ${token}`}});expect(info.status()).toBe(200);const camera=await info.json();expect(camera.cameraModel).toBe('Insta360 X6');expect(JSON.stringify(camera)).not.toContain('secret')
 const constant=(value:number)=>({mode:'constant',value})
 const content={schemaVersion:1,timeBase:1000000,canvas:{width:64,height:32,fpsMilli:4000,background:{red:0,green:0,blue:0,alpha:1}},sources:{[id]:{id,kind:'video',durationTicks:1000000,width:64,height:32,hasAudio:false}},tracks:[{kind:'video',id:'main',name:'Panorama',hidden:false,locked:false,clips:[{id:'clip',sourceId:id,placement:{timelineStartTick:0,sourceInTick:0,sourceOutTick:1000000,speed:1},transform:{x:constant(0),y:constant(0),scaleX:constant(1),scaleY:constant(1),rotationDegrees:constant(0),anchorX:.5,anchorY:.5},opacity:constant(1),blendMode:'normal',effects:[{kind:'spherical_video',parameters:{pan:90}}],enabled:true}]}]}
 await page.addInitScript(value=>localStorage.setItem('video-kadr.live-spherical-video-test',JSON.stringify(value)),{token,project:{schemaVersion:1,root:'root',compositions:{root:{durationUs:1000000,content,instances:[]}}}})
 const errors:string[]=[];page.on('pageerror',e=>errors.push(e.message))
 await page.goto('/e2e/fixtures/live-spherical-video.html')
 const history=page.getByLabel('Spherical Video history',{exact:true})
 const preview=async(red:number)=>{
  const response=page.waitForResponse(r=>new URL(r.url()).pathname==='/api/compositions/nested-preview');await page.getByRole('button',{name:'Native Preview',exact:true}).click();expect((await response).status()).toBe(200)
  await expect.poll(()=>page.getByLabel('Spherical Video preview',{exact:true}).evaluate((node,expected)=>{const canvas=node as HTMLCanvasElement;if(canvas.width!==64||canvas.height!==32)return false;const p=canvas.getContext('2d')!.getImageData(32,16,1,1).data;return Math.abs(p[0]!-expected)<5&&Math.abs(p[1]!-124)<5&&p[3]===255},red)).toBe(true)
 }
 await page.getByRole('button',{name:'Сведения камеры',exact:true}).click();await expect(page.getByText('Insta360 X6 · v0.1',{exact:true})).toBeVisible()
 await preview(190)
 await page.getByText('Встроенный gyro Insta360',{exact:true}).click();await page.getByRole('button',{name:'Импортировать встроенный gyro',exact:true}).click();await expect(page.getByText(/Показания извлечены; нужна калибровка осей камеры/)).toBeVisible();await expect(history).toHaveText('0')
 await page.getByLabel('Gyro camera from sensor',{exact:true}).fill('[1,0,0,0]');await page.getByRole('button',{name:'Импортировать встроенную ориентацию',exact:true}).click();await expect(history).toHaveText('1')
 const nativeGyro=JSON.parse(await page.getByLabel('Spherical Video document',{exact:true}).innerText()).compositions.root.content.tracks[0].clips[0].effects[0].parameters.orientation
 expect(nativeGyro.length).toBeGreaterThanOrEqual(2);expect(nativeGyro.length).toBeLessThanOrEqual(20000);expect(nativeGyro[0].timeSeconds).toBe(0);expect(nativeGyro.at(-1).timeSeconds).toBeCloseTo(1,9);expect(Math.abs(nativeGyro.at(-1).quaternion[2])).toBeGreaterThan(.65)
 await page.getByRole('button',{name:'Undo',exact:true}).click();await expect(history).toHaveText('0')
 await page.getByLabel('clip 360° видео pan key',{exact:true}).click();await expect(history).toHaveText('1')
 await page.getByLabel('clip spherical orientation',{exact:true}).setInputFiles({name:'orientation.json',mimeType:'application/json',buffer:Buffer.from(JSON.stringify({version:1,samples:[{timeSeconds:0,quaternion:[Math.SQRT1_2,0,Math.SQRT1_2,0]}]}))})
 await expect(history).toHaveText('2');await preview(126)
 await page.getByRole('button',{name:'Undo',exact:true}).click();await expect(history).toHaveText('1');await preview(190)
 await page.getByRole('button',{name:'Redo',exact:true}).click();await expect(history).toHaveText('2');await preview(126)
 const project=JSON.parse(await page.getByLabel('Spherical Video document',{exact:true}).innerText())
 const exported=await request.post('/api/compositions/nested-export',{headers:{Authorization:`Bearer ${token}`},data:{schemaVersion:1,mode:'video_only',project,compositionId:'root',startUs:0,endUs:1000000,renderedOutputs:{}}});expect(exported.ok()).toBe(true)
 const {jobId}=await exported.json();let terminal:{status:string;result?:{url:string}}|undefined
 await expect.poll(async()=>{const response=await request.get(`/api/jobs/${jobId}`,{headers:{Authorization:`Bearer ${token}`}});expect(response.ok()).toBe(true);terminal=await response.json();return terminal!.status},{timeout:60000}).toBe('done')
 const movie=await request.get(terminal!.result!.url,{headers:{Authorization:`Bearer ${token}`}});expect(movie.ok()).toBe(true)
 const decoded=spawnSync('ffmpeg',['-v','error','-i','pipe:0','-frames:v','1','-f','rawvideo','-pix_fmt','rgba','pipe:1'],{input:await movie.body()});expect(decoded.status).toBe(0);expect(decoded.stdout.length).toBe(64*32*4)
 const pixel=decoded.stdout.subarray((16*64+32)*4,(16*64+32)*4+4);expect(Math.abs(pixel[0]!-126)).toBeLessThan(5);expect(Math.abs(pixel[1]!-124)).toBeLessThan(5);expect(pixel[3]).toBe(255)
 await expect(page.getByLabel('Spherical Video preview error',{exact:true})).toHaveText('');expect(errors).toEqual([])
 await page.screenshot({path:'../docs/qualification/insta360-2026-10-31/native-preview.png',fullPage:true})
})

test('embedded camera profile pairs lens tracks through native preview',async({page,request})=>{
 const folder=mkdtempSync(join(tmpdir(),'video-kadr-rs-browser-'));let movieBytes:Buffer
 try{const path=join(folder,'lenses.mp4');const encoded=spawnSync('ffmpeg',['-v','error','-f','lavfi','-i','color=red:s=16x16:r=4:d=1','-f','lavfi','-i','color=blue:s=16x16:r=4:d=1','-map','0:v','-map','1:v','-c:v','libx264rgb','-crf','0','-bf','0','-threads','1','-movflags','+faststart',path]);expect(encoded.status,encoded.stderr.toString()).toBe(0);movieBytes=readFileSync(path)}finally{rmSync(folder,{recursive:true,force:true})}
 const lens=(x:number)=>[1,7,7,x,7.5,0,0,90,0,0,0,...Array(13).fill(0),32,16,193]
 const calibration=[2,...lens(7.5),...lens(23.5),0x60400].join('_')
 const varint=(n:number)=>{const result:number[]=[];do{const x=n%128;n=Math.floor(n/128);result.push(x|(n?128:0))}while(n);return Buffer.from(result)}
 const text=(field:number,value:string)=>Buffer.concat([varint(field*8+2),varint(Buffer.byteLength(value)),Buffer.from(value)])
 const integer=(field:number,value:number)=>Buffer.concat([varint(field*8),varint(value)])
 const config=Buffer.concat([integer(1,32),integer(2,2000)])
 const readout=Buffer.alloc(8);readout.writeDoubleLE(15.36);const metadata=Buffer.concat([text(2,'Insta360 X6'),text(111,calibration),integer(24,1000000),integer(20,4),integer(64,2),varint(25*8+1),readout,integer(51,1),integer(62,1),varint(65*8+2),varint(config.length),config])
 const gyro=Buffer.alloc(142*20);for(let i=0;i<142;i++){const at=i*20,t=(i*10000-200000)/1000000,angle=t*Math.PI/2;gyro.writeBigUInt64LE(BigInt(800000+i*10000),at);[32768-Math.round(Math.cos(angle)*1024),32768,32768-Math.round(Math.sin(angle)*1024),32768,34243,32768].forEach((value,j)=>gyro.writeUInt16LE(value,at+8+j*2))}
 const gyroSuffix=Buffer.alloc(6);gyroSuffix[1]=3;gyroSuffix.writeUInt32LE(gyro.length,2)
 const suffix=Buffer.alloc(6);suffix[0]=1;suffix[1]=1;suffix.writeUInt32LE(metadata.length,2)
 const exposure=Buffer.alloc(4*16);for(let i=0;i<4;i++){exposure.writeBigUInt64LE(BigInt(1000000+i*250000),i*16);exposure.writeDoubleLE(.01,i*16+8)}const exposureSuffix=Buffer.alloc(6);exposureSuffix[1]=4;exposureSuffix.writeUInt32LE(exposure.length,2)
 const index=Buffer.alloc(50);index[10]=1;index[11]=1;index.writeUInt32LE(metadata.length,12);index[30]=3;index.writeUInt32LE(gyro.length,32);index.writeUInt32LE(metadata.length+6,36);index[40]=4;index.writeUInt32LE(exposure.length,42);index.writeUInt32LE(metadata.length+6+gyro.length+6,46)
 const indexSuffix=Buffer.alloc(6);indexSuffix.writeUInt32LE(50,2)
 const footer=Buffer.alloc(72);footer.writeUInt32LE(metadata.length+6+gyro.length+6+exposure.length+6+50+6+72,32);footer.writeUInt32LE(3,36);footer.write('8db42d694ccc418790edff439fe026bf',40)
 const registered=await request.post('/api/auth/register',{data:{username:'camera-profile-browser-test',password:'scope-browser-test-password'}});expect(registered.status()).toBe(201);const {token}=await registered.json()
 const uploaded=await request.post('/api/upload',{headers:{Authorization:`Bearer ${token}`},multipart:{file:{name:'lenses.mp4',mimeType:'video/mp4',buffer:Buffer.concat([movieBytes!,metadata,suffix,gyro,gyroSuffix,exposure,exposureSuffix,index,indexSuffix,footer])}}});expect(uploaded.ok()).toBe(true);const {id}=await uploaded.json()
 const profileResponse=await request.get(`/api/library/${id}/insta360/profile`,{headers:{Authorization:`Bearer ${token}`}});expect(profileResponse.ok()).toBe(true);const profile=await profileResponse.json();expect(profile.calibration.renderReady).toBe(true);expect(profile.sourceLayout).toBe('dual_video_streams')
 const constant=(value:number)=>({mode:'constant',value})
 const content={schemaVersion:1,timeBase:1000000,canvas:{width:16,height:16,fpsMilli:4000,background:{red:0,green:0,blue:0,alpha:1}},sources:{[id]:{id,kind:'video',durationTicks:1000000,width:16,height:16,hasAudio:false}},tracks:[{kind:'video',id:'main',name:'Lenses',hidden:false,locked:false,clips:[{id:'lens-clip',sourceId:id,placement:{timelineStartTick:0,sourceInTick:0,sourceOutTick:1000000,speed:1},transform:{x:constant(0),y:constant(0),scaleX:constant(1),scaleY:constant(1),rotationDegrees:constant(0),anchorX:.5,anchorY:.5},opacity:constant(1),blendMode:'normal',effects:[{kind:'spherical_video',parameters:{}}],enabled:true}]}]}
 await page.addInitScript(value=>localStorage.setItem('video-kadr.live-spherical-video-test',JSON.stringify(value)),{token,project:{schemaVersion:1,root:'root',compositions:{root:{durationUs:1000000,content,instances:[]}}}})
 await page.goto('/e2e/fixtures/live-spherical-video.html');await page.getByText('Автоматический профиль Insta360',{exact:true}).click();await page.getByRole('button',{name:'Применить профиль из камеры',exact:true}).click();await expect(page.getByText('Применена индивидуальная калибровка двух объективов. Сшивка использует оба видеопотока.',{exact:true})).toBeVisible()
 await expect(page.getByLabel('Spherical Video history',{exact:true})).toHaveText('1')
 const gyroResponsePromise=page.waitForResponse(r=>new URL(r.url()).pathname===`/api/library/${id}/insta360/gyro`)
 await page.getByText('Встроенный gyro Insta360',{exact:true}).click();await page.getByRole('checkbox',{name:'Коррекция rolling shutter',exact:true}).check();await page.getByRole('checkbox',{name:'Коррекция дрейфа по неподвижному фону видео',exact:true}).check();await page.getByRole('button',{name:'Импортировать встроенный gyro',exact:true}).click();await expect(page.getByLabel('Spherical Video history',{exact:true})).toHaveText('2');await expect(page.getByText(/Оси взяты из профиля камеры/)).toBeVisible()
 const gyroResponse=await gyroResponsePromise;expect(gyroResponse.status()).toBe(200);expect(gyroResponse.request().postDataJSON()).toMatchObject({visualDriftCorrection:true,motionPaddingSeconds:.1});expect((await gyroResponse.json()).driftCorrection.visualFrames).toBeGreaterThanOrEqual(2)
 const autoOrientation=JSON.parse(await page.getByLabel('Spherical Video document',{exact:true}).innerText()).compositions.root.content.tracks[0].clips[0].effects[0].parameters.orientation;expect(autoOrientation.at(-1).quaternion[3]).toBeLessThan(-.65)
 await page.getByLabel('lens-clip spherical stitching',{exact:true}).selectOption('optical_flow');await expect(page.getByLabel('Spherical Video history',{exact:true})).toHaveText('3')
 const flowParameters=JSON.parse(await page.getByLabel('Spherical Video document',{exact:true}).innerText()).compositions.root.content.tracks[0].clips[0].effects[0].parameters;expect(flowParameters.stitching.mode).toBe('optical_flow');expect(flowParameters.sourceFrameClock).toMatchObject({numerator:4,denominator:1});expect(flowParameters.rollingShutter.readoutSeconds).toBeCloseTo(.01536,9);expect(flowParameters.rollingShutter.captureClock.length).toBeGreaterThanOrEqual(2)
 const response=page.waitForResponse(r=>new URL(r.url()).pathname==='/api/compositions/nested-preview');await page.getByRole('button',{name:'Native Preview',exact:true}).click();expect((await response).status()).toBe(200)
 await expect.poll(()=>page.getByLabel('Spherical Video preview',{exact:true}).evaluate(node=>{const c=node as HTMLCanvasElement;if(c.width!==16)return false;const p=c.getContext('2d')!.getImageData(8,8,1,1).data;return p[0]>230&&p[2]<20&&p[3]===255})).toBe(true)
 await page.getByLabel('lens-clip 360° видео pan',{exact:true}).fill('180');await page.getByLabel('lens-clip 360° видео pan',{exact:true}).dispatchEvent('change')
 const back=page.waitForResponse(r=>new URL(r.url()).pathname==='/api/compositions/nested-preview');await page.getByRole('button',{name:'Native Preview',exact:true}).click();expect((await back).status()).toBe(200)
 await expect.poll(()=>page.getByLabel('Spherical Video preview',{exact:true}).evaluate(node=>{const c=node as HTMLCanvasElement;const p=c.getContext('2d')!.getImageData(8,8,1,1).data;return p[2]>230&&p[0]<20&&p[3]===255})).toBe(true)
 const project=JSON.parse(await page.getByLabel('Spherical Video document',{exact:true}).innerText())
 const exportResponse=await request.post('/api/compositions/nested-export',{headers:{Authorization:`Bearer ${token}`},data:{schemaVersion:1,mode:'video_only',project,compositionId:'root',startUs:0,endUs:1000000,renderedOutputs:{}}});expect(exportResponse.ok()).toBe(true);const {jobId}=await exportResponse.json();let completed:{status:string;result?:{url:string}}|undefined
 await expect.poll(async()=>{const result=await request.get(`/api/jobs/${jobId}`,{headers:{Authorization:`Bearer ${token}`}});completed=await result.json();return completed!.status},{timeout:60000}).toBe('done')
 const movie=await request.get(completed!.result!.url,{headers:{Authorization:`Bearer ${token}`}});const decoded=spawnSync('ffmpeg',['-v','error','-i','pipe:0','-frames:v','1','-f','rawvideo','-pix_fmt','rgba','pipe:1'],{input:await movie.body()});expect(decoded.status).toBe(0);expect(decoded.stdout.length).toBe(16*16*4);const centre=decoded.stdout.subarray((8*16+8)*4,(8*16+8)*4+4);expect(centre[2]).toBeGreaterThan(230);expect(centre[0]).toBeLessThan(20)
})

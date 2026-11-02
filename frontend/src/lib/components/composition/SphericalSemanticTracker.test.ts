// @ts-expect-error Tests use the Svelte client entry.
import {mount,unmount,tick} from '../../../../node_modules/svelte/src/index-client.js'
import {afterEach,beforeEach,expect,it,vi} from 'vitest'
import Tool from './SphericalSemanticTracker.svelte'
import {measureSphericalSamRegion,getSamStatus} from '$lib/api'
import type {SamVideoRegionAttachment} from '$lib/composition/types'
vi.mock('$lib/api',()=>({measureSphericalSamRegion:vi.fn(),getSamStatus:vi.fn()}))
let component:ReturnType<typeof mount>|undefined
const region:SamVideoRegionAttachment={sourceId:'source',maskSourceId:'mask',sourceHash:'a'.repeat(64),maskHash:'b'.repeat(64),maskFileHash:'c'.repeat(64),encoderHash:'d'.repeat(64),decoderHash:'e'.repeat(64),runtimeHash:'f'.repeat(64),promptHash:'1'.repeat(64),points:[[.5,.5]],width:64,height:32,frames:10,frameRate:{numerator:10,denominator:1},seedFrame:1,trackedFrames:10,forwardLostAt:null,backwardLostAt:null}
beforeEach(()=>{vi.mocked(getSamStatus).mockResolvedValue({available:true,engine:'sam',stillImages:true,video:true});vi.mocked(measureSphericalSamRegion).mockReset();document.body.innerHTML=''})
afterEach(async()=>{if(component)await unmount(component);component=undefined})
async function setup(extra:{initialFov?:number;spherical?:import('$lib/composition/types').WireSphericalVideo}={}){const onapply=vi.fn();component=mount(Tool,{target:document.body,props:{region,token:'owner',projectId:'project',revision:3,compositionId:'root',clipId:'clip',sourceInSeconds:.2,duration:.6,binding:{version:1},onapply,...extra}});await tick();return onapply}
function start(){(document.querySelector('button') as HTMLButtonElement).click()}
it('requests native verified SAM centres and emits only camera channels',async()=>{
 vi.mocked(measureSphericalSamRegion).mockResolvedValue({centres:[{pan:179,tilt:0},{pan:-179,tilt:1},{pan:-177,tilt:2}]})
 const onapply=await setup();start();await vi.waitFor(()=>expect(onapply).toHaveBeenCalledTimes(1))
 expect(measureSphericalSamRegion).toHaveBeenCalledWith('owner','project',{compositionId:'root',clipId:'clip',expectedRevision:3,region,sourceTimesUs:[200000,400000,600000]},expect.any(AbortSignal))
 expect(Object.keys(onapply.mock.calls[0]![0])).toEqual(['pan','tilt'])
})
it('does not apply incomplete native measurements',async()=>{
 vi.mocked(measureSphericalSamRegion).mockResolvedValue({centres:[{pan:0,tilt:0}]})
 const onapply=await setup();start();await vi.waitFor(()=>expect(document.querySelector('[role="alert"]')).not.toBeNull());expect(onapply).not.toHaveBeenCalled()
})
it('aborts on unmount and ignores late completion',async()=>{
 let done!:(r:{centres:{pan:number;tilt:number}[]})=>void
 vi.mocked(measureSphericalSamRegion).mockImplementation(()=>new Promise(resolve=>{done=resolve}))
 const onapply=await setup();start();await vi.waitFor(()=>expect(measureSphericalSamRegion).toHaveBeenCalled());const signal=vi.mocked(measureSphericalSamRegion).mock.calls[0]![3]!
 await unmount(component!);component=undefined;expect(signal.aborted).toBe(true)
 done({centres:[{pan:0,tilt:0},{pan:1,tilt:0},{pan:2,tilt:0}]});await tick();expect(onapply).not.toHaveBeenCalled()
})
it('opts into native object-size framing and preserves the first authored FOV key',async()=>{
 vi.mocked(measureSphericalSamRegion).mockResolvedValue({centres:[{pan:0,tilt:0,fov:71},{pan:1,tilt:0,fov:65},{pan:2,tilt:0,fov:59}]})
 const onapply=await setup({initialFov:71});(document.querySelector('input[type="checkbox"]') as HTMLInputElement).click();await tick();start();await vi.waitFor(()=>expect(onapply).toHaveBeenCalledTimes(1))
 expect(vi.mocked(measureSphericalSamRegion).mock.calls[0]![2].framing).toEqual({aspectRatio:2,targetFill:.65,initialFov:71})
 expect(onapply.mock.calls[0]![0].fov.track.keyframes[0]).toEqual({tick:0,value:71})
})
it('ignores unsolicited zoom measurements when automatic framing is disabled',async()=>{
 vi.mocked(measureSphericalSamRegion).mockResolvedValue({centres:[{pan:0,tilt:0,fov:90},{pan:1,tilt:0,fov:84},{pan:2,tilt:0,fov:78}]})
 const onapply=await setup();start();await vi.waitFor(()=>expect(onapply).toHaveBeenCalledTimes(1))
 expect(onapply.mock.calls[0]![0].fov).toBeUndefined();expect(vi.mocked(measureSphericalSamRegion).mock.calls[0]![2].framing).toBeUndefined()
})

it('rejects an unexpected first zoom instead of replacing authored camera keys',async()=>{
 vi.mocked(measureSphericalSamRegion).mockResolvedValue({centres:[{pan:0,tilt:0,fov:80},{pan:1,tilt:0,fov:78},{pan:2,tilt:0,fov:76}]})
 const onapply=await setup();(document.querySelector('input[type="checkbox"]') as HTMLInputElement).click();await tick();start();await vi.waitFor(()=>expect(document.querySelector('[role="alert"]')?.textContent).toContain('начальное поле зрения'));expect(onapply).not.toHaveBeenCalled()
})

it('neural identity recovery creates only measured keys and retains the occlusion gap',async()=>{
 vi.mocked(measureSphericalSamRegion).mockResolvedValue({centres:[{pan:179,tilt:0},{pan:-170,tilt:2}],sampleIndices:[0,2],states:['seed','occluded','reacquired']})
 const onapply=await setup();document.querySelector<HTMLInputElement>('[aria-label="Нейронная повторная идентификация SAM"]')!.click();await tick();start();await vi.waitFor(()=>expect(onapply).toHaveBeenCalledTimes(1))
 expect(vi.mocked(measureSphericalSamRegion).mock.calls[0]![2].reidentify).toBe(true)
 expect(onapply.mock.calls[0]![0].pan.track.keyframes.map((k:{tick:number})=>k.tick)).toEqual([0,400000])
 expect(document.querySelector('[role="status"]')!.textContent).toContain('Пропусков: 1, восстановлений: 1')
})
it('rejects invented measurements in a declared occlusion',async()=>{
 vi.mocked(measureSphericalSamRegion).mockResolvedValue({centres:[{pan:0,tilt:0},{pan:2,tilt:0}],sampleIndices:[0,1],states:['seed','occluded','reacquired']})
 const onapply=await setup();document.querySelector<HTMLInputElement>('[aria-label="Нейронная повторная идентификация SAM"]')!.click();await tick();start();await vi.waitFor(()=>expect(document.querySelector('[role="alert"]')).not.toBeNull());expect(onapply).not.toHaveBeenCalled()
})
it('does not start neural source inference when the local SAM model is unavailable',async()=>{
 vi.mocked(getSamStatus).mockResolvedValue({available:false,engine:'sam',stillImages:false,video:false});const onapply=await setup();document.querySelector<HTMLInputElement>('[aria-label="Нейронная повторная идентификация SAM"]')!.click();await tick();start();await vi.waitFor(()=>expect(document.querySelector('[role="alert"]')).not.toBeNull());expect(measureSphericalSamRegion).not.toHaveBeenCalled();expect(onapply).not.toHaveBeenCalled()
})

it('requires native panorama preparation for dual fisheye and binds the current proposal',async()=>{
 vi.mocked(measureSphericalSamRegion).mockResolvedValue({centres:[{pan:0,tilt:0},{pan:20,tilt:2}],sampleIndices:[0,2],states:['seed','occluded','reacquired']})
 const spherical={input:'dual_fisheye' as const};const onapply=await setup({spherical});const checkbox=document.querySelector<HTMLInputElement>('[aria-label="Нейронная повторная идентификация SAM"]')!;expect(checkbox.checked).toBe(true);expect(checkbox.disabled).toBe(true);start();await vi.waitFor(()=>expect(onapply).toHaveBeenCalledTimes(1));expect(vi.mocked(measureSphericalSamRegion).mock.calls[0]![2]).toMatchObject({reidentify:true,spherical,parameterStartUs:200000})
})

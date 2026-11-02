// @ts-expect-error Tests use the Svelte client entry.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {afterEach,beforeEach,expect,it,vi} from 'vitest'
import * as api from '$lib/api'
import {observeEnhancement} from '$lib/enhance-job-client'
import Tool from './SamVideoTrackingTool.svelte'
vi.mock('$lib/api',()=>({getSamStatus:vi.fn(),startSamVideoTracking:vi.fn(),listSamVideoTracking:vi.fn(),saveSamVideoRegion:vi.fn(),cancelImageEnhancement:vi.fn()}))
vi.mock('$lib/enhance-job-client',()=>({observeEnhancement:vi.fn()}))
let component:ReturnType<typeof mount>|undefined
const points:[number,number][]=[[0.4,0.5]]
const region={sourceId:'input',maskSourceId:'region',sourceHash:'a'.repeat(64),maskHash:'b'.repeat(64),maskFileHash:'c'.repeat(64),encoderHash:'d'.repeat(64),decoderHash:'e'.repeat(64),runtimeHash:'f'.repeat(64),promptHash:'1'.repeat(64),points}
const media={kind:'source' as const,filename:'mask.png',url:'/files/mask',createdAt:0}
const snapshot={...region,width:37,height:23,frames:4,frameRate:{numerator:4,denominator:1},seedFrame:1,trackedFrames:4,forwardLostAt:null,backwardLostAt:null}
const entry:api.SavedSamVideoRegion={source:{...media,id:'input',mediaType:'video'},mask:{...media,id:'region',mediaType:'video',width:256,height:256},snapshot}

const button=(text:string)=>Array.from(document.querySelectorAll<HTMLButtonElement>('button')).find(b=>b.textContent===text)!
beforeEach(()=>{vi.mocked(api.getSamStatus).mockResolvedValue({available:true,engine:'sam',stillImages:true,video:true});vi.mocked(api.startSamVideoTracking).mockResolvedValue({jobId:'sam'});vi.mocked(observeEnhancement).mockResolvedValue({id:'sam',status:'done'});vi.mocked(api.saveSamVideoRegion).mockResolvedValue(entry);vi.spyOn(URL,'createObjectURL').mockReturnValue('blob:region');vi.spyOn(URL,'revokeObjectURL').mockImplementation(()=>{})})
afterEach(async()=>{if(component)await unmount(component);component=undefined;document.body.innerHTML='';vi.restoreAllMocks();vi.resetAllMocks()})
async function setup(){const onready=vi.fn(),onbusy=vi.fn();component=mount(Tool,{target:document.body,props:{token:'owner',projectId:'project',sourceId:'input',sourceHash:region.sourceHash,targetKey:'clip',seedFrame:1,width:37,height:23,frames:4,frameRate:snapshot.frameRate,points,onready,onbusy}});await tick();expect(api.getSamStatus).not.toHaveBeenCalled();button('Проверить SAM').click();await vi.waitFor(()=>expect(button('Отследить объект от выбранного кадра').disabled).toBe(false));return {onready,onbusy}}
it('starts at the exact ordinal and returns a verified portable temporal region',async()=>{const {onready,onbusy}=await setup();button('Отследить объект от выбранного кадра').click();await vi.waitFor(()=>expect(onready).toHaveBeenCalledWith(entry,'sam'));expect(api.startSamVideoTracking).toHaveBeenCalledWith('owner','input',points,1,expect.any(AbortSignal));expect(api.saveSamVideoRegion).toHaveBeenCalledWith('owner','sam','project',expect.any(AbortSignal));expect(onbusy).toHaveBeenCalledWith(true);await unmount(component!);component=undefined;expect(URL.createObjectURL).not.toHaveBeenCalled()})
it('refuses a result bound to a different source',async()=>{vi.mocked(api.saveSamVideoRegion).mockResolvedValue({...entry,snapshot:{...snapshot,sourceHash:'2'.repeat(64)}});const {onready}=await setup();button('Отследить объект от выбранного кадра').click();await vi.waitFor(()=>expect(document.querySelector('[role="alert"]')?.textContent).toContain('не соответствует исходнику'));expect(onready).not.toHaveBeenCalled();expect(URL.createObjectURL).not.toHaveBeenCalled()})
it('reconnects after observation failure without a duplicate admission',async()=>{vi.mocked(observeEnhancement).mockRejectedValueOnce(new Error('connection lost'));const {onready}=await setup();button('Отследить объект от выбранного кадра').click();await vi.waitFor(()=>expect(document.querySelector('[role="alert"]')?.textContent).toContain('connection lost'));button('Продолжить трекинг SAM').click();await vi.waitFor(()=>expect(onready).toHaveBeenCalledWith(entry,'sam'));expect(api.startSamVideoTracking).toHaveBeenCalledTimes(1);expect(observeEnhancement).toHaveBeenCalledTimes(2)})
it('suppresses late acceptance after unmount and aborts observation',async()=>{let done!:(value:{id:string;status:'done'})=>void;vi.mocked(observeEnhancement).mockImplementation(()=>new Promise(resolve=>{done=resolve}));const {onready}=await setup();button('Отследить объект от выбранного кадра').click();await vi.waitFor(()=>expect(observeEnhancement).toHaveBeenCalled());const signal=vi.mocked(observeEnhancement).mock.calls[0]![2];await unmount(component!);component=undefined;expect(signal.aborted).toBe(true);done({id:'sam',status:'done'});await tick();expect(onready).not.toHaveBeenCalled();expect(api.saveSamVideoRegion).not.toHaveBeenCalled()})


it('requires video capability rather than still-image SAM availability',async()=>{vi.mocked(api.getSamStatus).mockResolvedValue({available:true,engine:'sam',stillImages:true,video:false});const onready=vi.fn();component=mount(Tool,{target:document.body,props:{token:'owner',projectId:'project',sourceId:'input',sourceHash:region.sourceHash,targetKey:'clip',seedFrame:1,width:37,height:23,frames:4,frameRate:snapshot.frameRate,points,onready,onbusy:vi.fn()}});await tick();button('Проверить SAM').click();await vi.waitFor(()=>expect(document.body.textContent).toContain('недоступна'));expect(button('Отследить объект от выбранного кадра').disabled).toBe(true);expect(api.startSamVideoTracking).not.toHaveBeenCalled()})

it.each(['clock','geometry','count','media'])('refuses saved temporal result with mismatched %s',async(kind)=>{
 const bad=structuredClone(entry)
 if(kind==='clock')bad.snapshot={...bad.snapshot,frameRate:{...bad.snapshot.frameRate,numerator:5}}
 if(kind==='geometry')bad.snapshot={...bad.snapshot,width:38}
 if(kind==='count'){bad.snapshot={...bad.snapshot,frames:5,trackedFrames:5}}
 if(kind==='media')bad.mask.mediaType='image'
 vi.mocked(api.saveSamVideoRegion).mockResolvedValue(bad)
 const {onready}=await setup();button('Отследить объект от выбранного кадра').click()
 await vi.waitFor(()=>expect(document.querySelector('[role="alert"]')?.textContent).toContain('не соответствует исходнику'));expect(onready).not.toHaveBeenCalled()
})
it('reopens source-filtered completed video history without a new tracking admission',async()=>{
 const {onready}=await setup();vi.mocked(api.listSamVideoTracking).mockResolvedValue({jobs:[{jobId:'old',sourceId:'input',status:'done',createdAt:1,progress:100},{jobId:'foreign',sourceId:'other',status:'done',createdAt:1,progress:100}],nextCursor:null})
 button('Обновить историю SAM').click();await vi.waitFor(()=>expect(document.querySelectorAll('li button')).toHaveLength(1));(document.querySelector('li button') as HTMLButtonElement).click();await vi.waitFor(()=>expect(onready).toHaveBeenCalledWith(entry,'old'));expect(api.startSamVideoTracking).not.toHaveBeenCalled();expect(api.saveSamVideoRegion).toHaveBeenCalledWith('owner','old','project',expect.any(AbortSignal))
})
it('cancels and reconnects to the same admitted tracking job after observation failure',async()=>{
 vi.mocked(observeEnhancement).mockRejectedValueOnce(new Error('offline')).mockResolvedValueOnce({id:'sam',status:'cancelled'});vi.mocked(api.cancelImageEnhancement).mockResolvedValue(undefined)
 const {onready,onbusy}=await setup();button('Отследить объект от выбранного кадра').click();await vi.waitFor(()=>expect(document.querySelector('[role="alert"]')?.textContent).toContain('offline'));button('Отменить трекинг SAM').click();await vi.waitFor(()=>expect(onbusy).toHaveBeenLastCalledWith(false));expect(api.cancelImageEnhancement).toHaveBeenCalledWith('owner','sam',expect.any(AbortSignal));expect(api.startSamVideoTracking).toHaveBeenCalledTimes(1);expect(onready).not.toHaveBeenCalled()
})

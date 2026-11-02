// @ts-expect-error Tests use the Svelte client entry.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {afterEach,beforeEach,expect,it,vi} from 'vitest'
import * as api from '$lib/api'
import {observeEnhancement} from '$lib/enhance-job-client'
import type {Job} from '$lib/types'
import Panel from './PersonVideoPanel.svelte'
import Harness from './test-fixtures/PersonVideoHandoffHarness.svelte'
import {videoRegionRecipe} from './test-fixtures/videoRegionRecipe'
vi.mock('$lib/api',async original=>({...await original<typeof import('$lib/api')>(),startProjectVideoCorrection:vi.fn(),startPersonVideo:vi.fn(),continueVideoWithRegion:vi.fn(),getVideoCorrectionRecipe:vi.fn(),getVideoMaskFrame:vi.fn(),getVideoSourceFrame:vi.fn(),getVideoRegionFrame:vi.fn(),startVideoCorrection:vi.fn(),getSamStatus:vi.fn(),startSamVideoTracking:vi.fn(),saveSamVideoRegion:vi.fn(),cancelImageEnhancement:vi.fn(),savePersonVideo:vi.fn(),listPersonVideos:vi.fn()}))
vi.mock('$lib/enhance-job-client',()=>({observeEnhancement:vi.fn()}))
let component:ReturnType<typeof mount>|undefined
const value=videoRegionRecipe(),snapshot=value.projectOrigin!.snapshot,region={...snapshot.trackedRegions![0]!,maskSourceId:'new-region'}
const base={id:'original',kind:'source' as const,filename:'source.mov',url:'/private',createdAt:0,mediaType:'video' as const}
const entry:api.SavedSamVideoRegion={source:base,mask:{...base,id:'new-region',width:256,height:256},snapshot:region}
const button=(name:string)=>Array.from(document.querySelectorAll<HTMLButtonElement>('button')).find(b=>b.textContent===name)!
const done=(id:string):Job=>({id,status:'done',result:{width:37,height:23,frames:4,frameRate:{numerator:4,denominator:1}}})
beforeEach(()=>{
 const recipes=new Map<string,api.VideoFrameCorrection[]>()
 vi.mocked(api.startVideoCorrection).mockImplementation(async(_token,_id,corrections)=>{recipes.set('corrected',structuredClone(corrections));return{jobId:'corrected'}})
 vi.mocked(api.startProjectVideoCorrection).mockResolvedValue({jobId:'initial'});vi.mocked(api.continueVideoWithRegion).mockResolvedValue({jobId:'continuation'});vi.mocked(observeEnhancement).mockImplementation(async(_token,id)=>done(id))
 vi.mocked(api.getVideoCorrectionRecipe).mockImplementation(async(_token,id)=>{return({...value,corrections:recipes.get(id)??[],projectOrigin:{...value.projectOrigin!,snapshot:id!=='initial'?{...snapshot,trackedRegions:[...snapshot.trackedRegions!,region]}:snapshot}})})
 vi.mocked(api.getVideoMaskFrame).mockResolvedValue(new Blob(['mask']));vi.mocked(api.getVideoSourceFrame).mockResolvedValue(new Blob(['source']));vi.mocked(api.getVideoRegionFrame).mockResolvedValue({png:new Blob(['region']),maskHash:'4'.repeat(64)})
 vi.mocked(api.getSamStatus).mockResolvedValue({available:true,engine:'sam',stillImages:true,video:true});vi.mocked(api.startSamVideoTracking).mockResolvedValue({jobId:'tracking'});vi.mocked(api.saveSamVideoRegion).mockResolvedValue(entry);vi.mocked(api.cancelImageEnhancement).mockResolvedValue(undefined)
 vi.spyOn(console,'warn');vi.spyOn(URL,'createObjectURL').mockReturnValue('blob:private');vi.spyOn(URL,'revokeObjectURL').mockImplementation(()=>{})
})
afterEach(async()=>{if(component)await unmount(component);component=undefined;document.body.innerHTML='';vi.restoreAllMocks();vi.resetAllMocks()})
async function start(){
 const onattached=vi.fn();component=mount(Harness,{target:document.body,props:{onattached}});await tick();button('Открыть сохранённые видеомаски').click();await vi.waitFor(()=>expect(button('Открыть видеомаски')).toBeDefined());button('Открыть видеомаски').click();await vi.waitFor(()=>expect(document.querySelector('[aria-label="Полотно видеомаски"]')).not.toBeNull())
 const index=document.querySelector<HTMLInputElement>('[aria-label="Кадр видеомаски"]')!;index.value='1';index.dispatchEvent(new Event('change',{bubbles:true}));await tick();await vi.waitFor(()=>expect(document.querySelector('[aria-label="Полотно видеомаски"]')).not.toBeNull())
 const select=document.querySelector<HTMLSelectElement>('[aria-label="Основа полотна видеомаски"]')!;select.value='source';select.dispatchEvent(new Event('change',{bubbles:true}));await tick();await vi.waitFor(()=>expect(document.querySelector('[aria-label="Полотно видеомаски"] image')).not.toBeNull())
 button('Выбрать точку объекта SAM').click();await tick();document.querySelector('[aria-label="Полотно видеомаски"]')!.dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',bubbles:true}));await tick();button('Проверить SAM').click();await vi.waitFor(()=>expect(button('Отследить объект от выбранного кадра').disabled).toBe(false));button('Отследить объект от выбранного кадра').click();return onattached
}
it('adopts the completed session after one inventory action and automatically opens its preferred region',async()=>{
 const attached=await start();await vi.waitFor(()=>expect(document.querySelector<HTMLSelectElement>('[aria-label="Сохранённая область видеомаски"]')?.value).toBe('new-region'))
 expect(document.querySelector('[aria-label="Полотно видеомаски"]')?.closest('details')?.open).toBe(true);expect((document.querySelector('[aria-label="Кадр видеомаски"]') as HTMLInputElement).value).toBe('1');expect((document.querySelector('[aria-label="Основа полотна видеомаски"]') as HTMLSelectElement).value).toBe('source');expect(attached).toHaveBeenCalledTimes(1);expect(attached).toHaveBeenCalledWith(entry,snapshot);expect(vi.mocked(api.getVideoCorrectionRecipe).mock.calls.map(c=>c[1])).toEqual(['initial','continuation']);expect(console.warn).not.toHaveBeenCalled();expect(api.continueVideoWithRegion).toHaveBeenCalledWith('owner','initial','tracking',[],expect.any(AbortSignal));expect(api.startProjectVideoCorrection).toHaveBeenCalledTimes(1);expect(api.savePersonVideo).not.toHaveBeenCalled();expect(document.querySelector('[data-region-count]')?.getAttribute('data-region-count')).toBe('2')
 button('Показать область кадра').click();await vi.waitFor(()=>expect(api.getVideoRegionFrame).toHaveBeenCalledWith('owner','continuation','new-region',1,expect.any(AbortSignal)))
})
it('reconnects to the already admitted continuation after observation failure',async()=>{
 let failed=false;vi.mocked(observeEnhancement).mockImplementation(async(_token,id)=>{if(id==='continuation'&&!failed){failed=true;throw new Error('observation offline')}return done(id)})
 const attached=await start();await vi.waitFor(()=>expect(document.body.textContent).toContain('observation offline'));button('Продолжить трекинг SAM').click();await vi.waitFor(()=>expect(document.querySelector<HTMLSelectElement>('[aria-label="Сохранённая область видеомаски"]')?.value).toBe('new-region'));expect(api.continueVideoWithRegion).toHaveBeenCalledTimes(1);expect(attached).toHaveBeenCalledTimes(1)
})
it.each(['аккаунт','клип'])('aborts and suppresses late admission after changing %s',async(kind)=>{
 let finish!:(value:{jobId:string})=>void;vi.mocked(api.continueVideoWithRegion).mockImplementation(()=>new Promise(resolve=>{finish=resolve}));const attached=await start();await vi.waitFor(()=>expect(api.continueVideoWithRegion).toHaveBeenCalled());const signal=vi.mocked(api.continueVideoWithRegion).mock.calls[0]![4]!
 button(`Сменить ${kind} подготовки`).click();await tick();expect(signal.aborted).toBe(true);finish({jobId:'late'});await tick();expect(attached).not.toHaveBeenCalled();expect(document.querySelector('[data-region-count]')?.getAttribute('data-region-count')).toBe('1');expect(api.getVideoCorrectionRecipe).not.toHaveBeenCalledWith('other','late',expect.anything())
})
it('refuses adoption if the document changes during continuation observation',async()=>{
 let finish!:(job:Job)=>void;vi.mocked(observeEnhancement).mockImplementation(async(_token,id)=>id==='continuation'?await new Promise(resolve=>{finish=resolve}):done(id));const attached=await start();await vi.waitFor(()=>expect(button('Отменить подготовку области')).toBeDefined());button('Изменить документ подготовки').click();await tick();finish(done('continuation'));await vi.waitFor(()=>expect(document.body.textContent).toContain('Выбор клипа изменился'));expect(attached).not.toHaveBeenCalled()
})

it('cancels the continuation job without attaching its region',async()=>{
 let finish!:(job:Job)=>void;vi.mocked(observeEnhancement).mockImplementation(async(_token,id)=>id==='continuation'?await new Promise(resolve=>{finish=resolve}):done(id));const attached=await start();await vi.waitFor(()=>expect(button('Отменить подготовку области')).toBeDefined());button('Отменить подготовку области').click();await vi.waitFor(()=>expect(api.cancelImageEnhancement).toHaveBeenCalledWith('owner','continuation',expect.any(AbortSignal)));finish({id:'continuation',status:'cancelled'});await vi.waitFor(()=>expect(document.body.textContent).toContain('Подготовка области не завершена'));expect(attached).not.toHaveBeenCalled();expect(document.querySelector('[data-region-count]')?.getAttribute('data-region-count')).toBe('1')
})

it('preserves the current ordinal and overlay view across a subsequent mask correction',async()=>{
 await start();await vi.waitFor(()=>expect(document.querySelector<HTMLSelectElement>('[aria-label="Сохранённая область видеомаски"]')?.value).toBe('new-region'))
 const selected=document.querySelector<HTMLSelectElement>('[aria-label="Сохранённая область видеомаски"]')!;selected.value=snapshot.trackedRegions![0]!.maskSourceId;selected.dispatchEvent(new Event('change',{bubbles:true}));await tick()
 const index=document.querySelector<HTMLInputElement>('[aria-label="Кадр видеомаски"]')!;index.value='2';index.dispatchEvent(new Event('change',{bubbles:true}));await tick();await vi.waitFor(()=>expect(document.querySelector('[aria-label="Полотно видеомаски"]')).not.toBeNull())
 const view=document.querySelector<HTMLSelectElement>('[aria-label="Основа полотна видеомаски"]')!;view.value='overlay';view.dispatchEvent(new Event('change',{bubbles:true}));await tick();await vi.waitFor(()=>expect(document.querySelectorAll('[aria-label="Полотно видеомаски"] image')).toHaveLength(2));document.querySelector('[aria-label="Полотно видеомаски"]')!.dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',bubbles:true}));await tick();button('Пересчитать видеомаски').click()
 await vi.waitFor(()=>expect(api.getVideoCorrectionRecipe).toHaveBeenCalledWith('owner','corrected',expect.any(AbortSignal)))
 await vi.waitFor(()=>expect(api.getVideoSourceFrame).toHaveBeenCalledWith('owner','corrected',2,expect.any(AbortSignal)))
 expect(document.querySelector<HTMLInputElement>('[aria-label="Кадр видеомаски"]')?.value).toBe('2');expect(document.querySelector<HTMLSelectElement>('[aria-label="Основа полотна видеомаски"]')?.value).toBe('overlay');expect(document.querySelector<HTMLSelectElement>('[aria-label="Сохранённая область видеомаски"]')?.value).toBe(snapshot.trackedRegions![0]!.maskSourceId);expect(vi.mocked(api.getVideoCorrectionRecipe).mock.calls.map(c=>c[1])).toEqual(['initial','continuation','corrected'])
})

it('keeps navigation for ordinary job-history corrections without project regions or SAM',async()=>{
 vi.mocked(api.listPersonVideos).mockResolvedValue({jobs:[{jobId:'plain',sourceId:'original',status:'done',createdAt:1,progress:100}],nextCursor:null})
 vi.mocked(api.getVideoCorrectionRecipe).mockImplementation(async(_token,id)=>({baseJobId:'plain-root',maskHash:'a'.repeat(64),corrections:id==='corrected'?(vi.mocked(api.startVideoCorrection).mock.calls[0]?.[2]??[]):[]}))
 component=mount(Panel,{target:document.body,props:{token:'owner',projectId:'project',sourceId:'original',targetKey:'clip',width:37,height:23,context:{},onaccepted:vi.fn()}});await tick();button('Обновить историю удаления фона').click();await vi.waitFor(()=>expect(document.querySelector('li button')).not.toBeNull());(document.querySelector('li button') as HTMLButtonElement).click();await vi.waitFor(()=>expect(button('Открыть видеомаски')).toBeDefined());button('Открыть видеомаски').click();await vi.waitFor(()=>expect(document.querySelector('[aria-label="Полотно видеомаски"]')).not.toBeNull())
 const index=document.querySelector<HTMLInputElement>('[aria-label="Кадр видеомаски"]')!;index.value='3';index.dispatchEvent(new Event('change',{bubbles:true}));await tick();const view=document.querySelector<HTMLSelectElement>('[aria-label="Основа полотна видеомаски"]')!;view.value='overlay';view.dispatchEvent(new Event('change',{bubbles:true}));await tick();await vi.waitFor(()=>expect(document.querySelectorAll('[aria-label="Полотно видеомаски"] image')).toHaveLength(2));document.querySelector('[aria-label="Полотно видеомаски"]')!.dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',bubbles:true}));await tick();button('Пересчитать видеомаски').click()
 await vi.waitFor(()=>expect(api.getVideoSourceFrame).toHaveBeenCalledWith('owner','corrected',3,expect.any(AbortSignal)));expect(document.querySelector<HTMLInputElement>('[aria-label="Кадр видеомаски"]')?.value).toBe('3');expect(document.querySelector<HTMLSelectElement>('[aria-label="Основа полотна видеомаски"]')?.value).toBe('overlay');expect(api.continueVideoWithRegion).not.toHaveBeenCalled();expect(api.getSamStatus).not.toHaveBeenCalled();expect(vi.mocked(api.getVideoCorrectionRecipe).mock.calls.map(c=>c[1])).toEqual(['plain','corrected'])
})

it('restores the exact completed brush session through inventory Undo and Redo without readmitting work',async()=>{
 await start();await vi.waitFor(()=>expect(document.querySelector<HTMLSelectElement>('[aria-label="Сохранённая область видеомаски"]')?.value).toBe('new-region'))
 button('Undo области').click();await tick()
 await vi.waitFor(()=>expect(vi.mocked(api.getVideoCorrectionRecipe).mock.calls.map(c=>c[1])).toEqual(['initial','continuation','initial']))
 expect(document.querySelector('[data-region-count]')?.getAttribute('data-region-count')).toBe('1')
 expect(document.querySelector<HTMLInputElement>('[aria-label="Кадр видеомаски"]')?.value).toBe('1')
 expect(document.querySelector<HTMLSelectElement>('[aria-label="Основа полотна видеомаски"]')?.value).toBe('source')
 expect(document.querySelector<HTMLSelectElement>('[aria-label="Сохранённая область видеомаски"]')?.value).not.toBe('new-region')
 button('Redo области').click();await tick()
 await vi.waitFor(()=>expect(document.querySelector<HTMLSelectElement>('[aria-label="Сохранённая область видеомаски"]')?.value).toBe('new-region'))
 expect(vi.mocked(api.getVideoCorrectionRecipe).mock.calls.map(c=>c[1])).toEqual(['initial','continuation','initial','continuation'])
 expect(document.querySelector<HTMLInputElement>('[aria-label="Кадр видеомаски"]')?.value).toBe('1')
 expect(document.querySelector<HTMLSelectElement>('[aria-label="Основа полотна видеомаски"]')?.value).toBe('source')
 expect(api.startProjectVideoCorrection).toHaveBeenCalledTimes(1);expect(api.continueVideoWithRegion).toHaveBeenCalledTimes(1);expect(api.startVideoCorrection).not.toHaveBeenCalled();expect(api.cancelImageEnhancement).not.toHaveBeenCalled()
})

it('clears retained inventory sessions when the account changes',async()=>{
 await start();await vi.waitFor(()=>expect(document.querySelector<HTMLSelectElement>('[aria-label="Сохранённая область видеомаски"]')?.value).toBe('new-region'))
 button('Сменить аккаунт подготовки').click();await tick();button('Undo области').click();await tick();button('Redo области').click();await tick()
 expect(document.querySelector('[aria-label="Полотно видеомаски"]')).toBeNull()
 expect(vi.mocked(observeEnhancement).mock.calls.some(c=>c[0]==='other')).toBe(false)
 expect(api.startProjectVideoCorrection).toHaveBeenCalledTimes(1);expect(api.continueVideoWithRegion).toHaveBeenCalledTimes(1)
})

it('rechecks restored jobs and refuses revoked results without starting replacement work',async()=>{
 await start();await vi.waitFor(()=>expect(document.querySelector<HTMLSelectElement>('[aria-label="Сохранённая область видеомаски"]')?.value).toBe('new-region'))
 button('Undo области').click();await tick();await vi.waitFor(()=>expect(vi.mocked(api.getVideoCorrectionRecipe).mock.calls.map(c=>c[1])).toEqual(['initial','continuation','initial']))
 const reads=vi.mocked(api.getVideoCorrectionRecipe).mock.calls.length
 vi.mocked(observeEnhancement).mockImplementation(async(_token,id)=>{if(id==='continuation')throw new Error('Доступ к области отозван');return done(id)})
 button('Redo области').click();await tick();await vi.waitFor(()=>expect(document.body.textContent).toContain('Доступ к области отозван'))
 expect(document.querySelector('[aria-label="Полотно видеомаски"]')).toBeNull();expect(api.getVideoCorrectionRecipe).toHaveBeenCalledTimes(reads)
 expect(api.startProjectVideoCorrection).toHaveBeenCalledTimes(1);expect(api.continueVideoWithRegion).toHaveBeenCalledTimes(1);expect(api.cancelImageEnhancement).not.toHaveBeenCalled()
})

it('retains later unsubmitted frame, view and region selection separately for each inventory snapshot',async()=>{
 await start();await vi.waitFor(()=>expect(document.querySelector<HTMLSelectElement>('[aria-label="Сохранённая область видеомаски"]')?.value).toBe('new-region'))
 const frame=document.querySelector<HTMLInputElement>('[aria-label="Кадр видеомаски"]')!;frame.value='3';frame.dispatchEvent(new Event('change',{bubbles:true}));await tick()
 const view=document.querySelector<HTMLSelectElement>('[aria-label="Основа полотна видеомаски"]')!;view.value='overlay';view.dispatchEvent(new Event('change',{bubbles:true}));await tick()
 const region=document.querySelector<HTMLSelectElement>('[aria-label="Сохранённая область видеомаски"]')!;region.value=snapshot.trackedRegions![0]!.maskSourceId;region.dispatchEvent(new Event('change',{bubbles:true}));await tick()
 await vi.waitFor(()=>expect(api.getVideoSourceFrame).toHaveBeenCalledWith('owner','continuation',3,expect.any(AbortSignal)))
 button('Undo области').click();await tick();await vi.waitFor(()=>expect(document.querySelector<HTMLInputElement>('[aria-label="Кадр видеомаски"]')?.value).toBe('1'));expect(api.getVideoSourceFrame).toHaveBeenCalledWith('owner','initial',1,expect.any(AbortSignal))
 expect(document.querySelector<HTMLInputElement>('[aria-label="Кадр видеомаски"]')?.value).toBe('1');expect(document.querySelector<HTMLSelectElement>('[aria-label="Основа полотна видеомаски"]')?.value).toBe('source')
 button('Redo области').click();await tick();await vi.waitFor(()=>expect(document.querySelector<HTMLInputElement>('[aria-label="Кадр видеомаски"]')?.value).toBe('3'))
 expect(document.querySelector<HTMLSelectElement>('[aria-label="Основа полотна видеомаски"]')?.value).toBe('overlay');expect(document.querySelector<HTMLSelectElement>('[aria-label="Сохранённая область видеомаски"]')?.value).toBe(snapshot.trackedRegions![0]!.maskSourceId)
 expect(vi.mocked(api.getVideoCorrectionRecipe).mock.calls.map(c=>c[1])).toEqual(['initial','continuation','initial','continuation']);expect(api.startVideoCorrection).not.toHaveBeenCalled();expect(api.continueVideoWithRegion).toHaveBeenCalledTimes(1);expect(api.startProjectVideoCorrection).toHaveBeenCalledTimes(1)
})

async function openRetainedHistory(sourceId='original'){
 vi.mocked(api.listPersonVideos).mockResolvedValue({jobs:[{jobId:'retained',sourceId:'original',status:'done',createdAt:1,progress:100}],nextCursor:null})
 component=mount(Panel,{target:document.body,props:{token:'owner',projectId:'project',sourceId,targetKey:'clip',width:37,height:23,context:{},savedMask:{clipId:'clip',compositionId:'root',snapshot},onaccepted:vi.fn()}})
 await tick();button('Обновить историю удаления фона').click();await vi.waitFor(()=>expect(document.querySelector('li button')).not.toBeNull());(document.querySelector('li button') as HTMLButtonElement).click()
}

it.each(['project','clip','composition','snapshot'])('refuses history bound to a different %s before exposing a brush',async(kind)=>{
 const origin=structuredClone(value.projectOrigin!)
 if(kind==='project')origin.projectId='elsewhere'
 if(kind==='clip')origin.clipId='other-clip'
 if(kind==='composition')origin.compositionId='other-timeline'
 if(kind==='snapshot')origin.snapshot={...origin.snapshot,maskHash:'9'.repeat(64)}
 vi.mocked(api.getVideoCorrectionRecipe).mockResolvedValue({...value,projectOrigin:origin})
 await openRetainedHistory();await vi.waitFor(()=>expect(document.body.textContent).toContain('другому проекту, клипу или снимку'))
 expect(button('Открыть видеомаски')).toBeUndefined();expect(document.querySelector('[aria-label="Полотно видеомаски"]')).toBeNull();expect(api.startProjectVideoCorrection).not.toHaveBeenCalled();expect(api.continueVideoWithRegion).not.toHaveBeenCalled();expect(api.startPersonVideo).not.toHaveBeenCalled()
})

it.each([false,true])('reopens matching durable history after panel remount, including unsaved baseline=%s',async(unsaved)=>{
 const origin=structuredClone(value.projectOrigin!)
 if(unsaved){origin.savedSnapshot=structuredClone(snapshot);origin.snapshot={...snapshot,trackedRegions:[...snapshot.trackedRegions!,region]}}
 else origin.snapshot=Object.fromEntries(Object.entries(snapshot).reverse()) as unknown as typeof snapshot
 vi.mocked(api.getVideoCorrectionRecipe).mockResolvedValue({...value,corrections:[],projectOrigin:origin})
 for(let iteration=0;iteration<2;iteration++){
  await openRetainedHistory();await vi.waitFor(()=>expect(button('Открыть видеомаски')).toBeDefined());button('Открыть видеомаски').click();await vi.waitFor(()=>expect(document.querySelector('[aria-label="Полотно видеомаски"]')).not.toBeNull())
  expect(document.querySelector<HTMLSelectElement>('[aria-label="Сохранённая область видеомаски"]')?.options.length).toBe(unsaved?2:1)
  await unmount(component!);component=undefined;document.body.innerHTML=''
 }
 expect(api.startProjectVideoCorrection).not.toHaveBeenCalled();expect(api.continueVideoWithRegion).not.toHaveBeenCalled();expect(api.startPersonVideo).not.toHaveBeenCalled();expect(api.startVideoCorrection).not.toHaveBeenCalled();expect(api.cancelImageEnhancement).not.toHaveBeenCalled()
 expect(vi.mocked(api.getVideoCorrectionRecipe).mock.calls.map(c=>c[1])).toEqual(['retained','retained','retained','retained'])
})

it('aborts history preflight and suppresses its late result after an account change',async()=>{
 let finish!:(recipe:api.VideoCorrectionRecipe)=>void
 vi.mocked(api.getVideoCorrectionRecipe).mockImplementation(()=>new Promise(resolve=>{finish=resolve}))
 vi.mocked(api.listPersonVideos).mockResolvedValue({jobs:[{jobId:'retained',sourceId:'original',status:'done',createdAt:1,progress:100}],nextCursor:null})
 component=mount(Harness,{target:document.body,props:{onattached:vi.fn()}});await tick();button('Обновить историю удаления фона').click();await vi.waitFor(()=>expect(document.querySelector('li button')).not.toBeNull());(document.querySelector('li button') as HTMLButtonElement).click()
 await vi.waitFor(()=>expect(api.getVideoCorrectionRecipe).toHaveBeenCalledWith('owner','retained',expect.any(AbortSignal)))
 const signal=vi.mocked(api.getVideoCorrectionRecipe).mock.calls[0]![2]!
 button('Сменить аккаунт подготовки').click();await tick();expect(signal.aborted).toBe(true);finish(value);await tick()
 expect(button('Открыть видеомаски')).toBeUndefined();expect(document.querySelector('[aria-label="Полотно видеомаски"]')).toBeNull();expect(vi.mocked(observeEnhancement).mock.calls.some(c=>c[0]==='other')).toBe(false)
 expect(api.startProjectVideoCorrection).not.toHaveBeenCalled();expect(api.startPersonVideo).not.toHaveBeenCalled()
})

it('finds saved-mask history through its original even while the clip displays a processed copy',async()=>{
 vi.mocked(api.getVideoCorrectionRecipe).mockResolvedValue({...value,corrections:[]})
 await openRetainedHistory('processed-copy');await vi.waitFor(()=>expect(button('Открыть видеомаски')).toBeDefined())
 expect(api.listPersonVideos).toHaveBeenCalledWith('owner','original',undefined,expect.any(AbortSignal));expect(api.startPersonVideo).not.toHaveBeenCalled();expect(api.startProjectVideoCorrection).not.toHaveBeenCalled()
})

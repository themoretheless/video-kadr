import {videoRegionRecipe} from './test-fixtures/videoRegionRecipe'
// @ts-expect-error Tests use the Svelte client entry.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {afterEach,beforeEach,expect,it,vi} from 'vitest'
import {getSamStatus,startSamVideoTracking,saveSamVideoRegion,getVideoRegionFrame,getVideoSourceFrame,getVideoCorrectionRecipe,getVideoMaskFrame,startVideoCorrection} from '$lib/api'
import {observeEnhancement} from '$lib/enhance-job-client'
import Editor from './VideoMaskBrushEditor.svelte'
import Harness from './test-fixtures/VideoMaskScopeHarness.svelte'
vi.mock('$lib/api',async original=>({...await original<typeof import('$lib/api')>(),getSamStatus:vi.fn(),startSamVideoTracking:vi.fn(),saveSamVideoRegion:vi.fn(),getVideoRegionFrame:vi.fn(),getVideoSourceFrame:vi.fn(),getVideoCorrectionRecipe:vi.fn(),getVideoMaskFrame:vi.fn(),startVideoCorrection:vi.fn()}))
vi.mock('$lib/enhance-job-client',()=>({observeEnhancement:vi.fn()}))
let component:ReturnType<typeof mount>|undefined
const button=(text:string)=>Array.from(document.querySelectorAll<HTMLButtonElement>('button')).find(b=>b.textContent===text)!
const canvas=()=>document.querySelector<HTMLButtonElement>('[aria-label="Полотно видеомаски"]')!
const key=(text:string)=>canvas().dispatchEvent(new KeyboardEvent('keydown',{key:text,bubbles:true,cancelable:true}))
beforeEach(()=>{vi.mocked(getVideoRegionFrame).mockResolvedValue({png:new Blob([new Uint8Array(64)]),maskHash:'4'.repeat(64)});vi.mocked(getVideoSourceFrame).mockResolvedValue(new Blob([new Uint8Array(64)]));vi.mocked(getVideoCorrectionRecipe).mockResolvedValue({baseJobId:'original-job',maskHash:'a'.repeat(64),corrections:[]});vi.mocked(getVideoMaskFrame).mockResolvedValue(new Blob([new Uint8Array(64)]));vi.mocked(startVideoCorrection).mockResolvedValue({jobId:'corrected-job'});vi.spyOn(URL,'createObjectURL').mockReturnValue('blob:mask');vi.spyOn(URL,'revokeObjectURL').mockImplementation(()=>{})})
afterEach(async()=>{if(component)await unmount(component);component=undefined;document.body.innerHTML='';vi.restoreAllMocks();vi.resetAllMocks()})
async function setup(harness=false,frameRate?:{numerator:number;denominator:number}){const onready=vi.fn(),onchanged=vi.fn();component=harness?mount(Harness,{target:document.body,props:{onready,onchanged}}):mount(Editor,{target:document.body,props:{token:'owner',jobId:'base',width:37,height:23,frames:4,frameRate,onready,onchanged}});await tick();button('Открыть видеомаски').click();await vi.waitFor(()=>expect(canvas()).not.toBeNull());return{onready,onchanged}}
async function frame(n:number){const input=document.querySelector<HTMLInputElement>('[aria-label="Кадр видеомаски"]')!;input.value=String(n);input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(getVideoMaskFrame).toHaveBeenLastCalledWith('owner','original-job',n,expect.any(AbortSignal)));await vi.waitFor(()=>expect(canvas()).not.toBeNull())}
it('authors independent ordinal recipes, switches original masks and admits one explicit correction',async()=>{const{onready,onchanged}=await setup();key('Enter');await tick();await frame(2);key('ArrowRight');key('Enter');await tick();expect(onchanged).toHaveBeenLastCalledWith(true);button('Пересчитать видеомаски').click();await vi.waitFor(()=>expect(onready).toHaveBeenCalledWith('corrected-job',{frame:2,view:'mask'}));const args=vi.mocked(startVideoCorrection).mock.calls[0]!;expect(args[0]).toBe('owner');expect(args[1]).toBe('base');expect(args[2].map(e=>e.frame)).toEqual([0,2]);expect(args[2][1]!.correction.strokes[0]!.points[0]![0]).toBeCloseTo(.51);expect(URL.revokeObjectURL).toHaveBeenCalledWith('blob:mask')})
it('restores saved strokes per frame and resets to the original automatic sequence',async()=>{vi.mocked(getVideoCorrectionRecipe).mockResolvedValue({baseJobId:'original-job',maskHash:'a'.repeat(64),corrections:[{frame:2,correction:{custom:true,feather:0,strokes:[{tool:'eraser',size:.1,points:[[.2,.3]],at:null}]}}]});const{onchanged}=await setup();expect(canvas().querySelectorAll('path')).toHaveLength(0);await frame(2);expect(canvas().querySelectorAll('path')).toHaveLength(1);expect(onchanged).toHaveBeenLastCalledWith(false);button('Сбросить выбранный кадр').click();await tick();button('Пересчитать видеомаски').click();await vi.waitFor(()=>expect(startVideoCorrection).toHaveBeenCalledWith('owner','base',[],expect.any(AbortSignal)))})
it('undoes and redoes authoring across frames and discards back to the saved baseline',async()=>{const{onchanged}=await setup();key('Enter');await tick();await frame(1);key('Enter');await tick();button('Отменить правку кадра').click();await tick();expect(canvas().querySelectorAll('path')).toHaveLength(0);button('Повторить правку кадра').click();await tick();expect(canvas().querySelectorAll('path')).toHaveLength(1);button('Отбросить правки видео').click();await tick();expect(onchanged).toHaveBeenLastCalledWith(false);expect(button('Пересчитать видеомаски').disabled).toBe(true)})
it('aborts private frame reads and suppresses late pixels after account changes',async()=>{let resolve!:(value:Blob)=>void;vi.mocked(getVideoMaskFrame).mockImplementation(()=>new Promise(r=>{resolve=r}));component=mount(Harness,{target:document.body,props:{onready:vi.fn(),onchanged:vi.fn()}});await tick();button('Открыть видеомаски').click();await vi.waitFor(()=>expect(getVideoMaskFrame).toHaveBeenCalled());const signal=vi.mocked(getVideoMaskFrame).mock.calls[0]![3]!;button('Сменить владельца видео').click();await tick();expect(signal.aborted).toBe(true);resolve(new Blob([new Uint8Array(64)]));await tick();expect(URL.createObjectURL).not.toHaveBeenCalled();expect(canvas()).toBeNull()})
it('suppresses a late correction admission after changing the selected job',async()=>{const{onready}=await setup(true);let resolve!:(value:{jobId:string})=>void;vi.mocked(startVideoCorrection).mockImplementation(()=>new Promise(r=>{resolve=r}));key('Enter');await tick();button('Пересчитать видеомаски').click();await vi.waitFor(()=>expect(startVideoCorrection).toHaveBeenCalled());const signal=vi.mocked(startVideoCorrection).mock.calls[0]![3]!;button('Сменить задание видео').click();await tick();expect(signal.aborted).toBe(true);resolve({jobId:'old'});await tick();expect(onready).not.toHaveBeenCalled();expect(URL.revokeObjectURL).toHaveBeenCalledWith('blob:mask')})
it('refuses temporal saved recipes before opening private frames',async()=>{const correction={custom:true,feather:0,strokes:[{tool:'brush' as const,size:.1,points:[[.5,.5] as [number,number]],at:0}]};vi.mocked(getVideoCorrectionRecipe).mockResolvedValue({baseJobId:'original-job',maskHash:'a'.repeat(64),corrections:[{frame:0,correction}]});component=mount(Editor,{target:document.body,props:{token:'owner',jobId:'base',width:37,height:23,frames:4,onready:vi.fn(),onchanged:vi.fn()}});await tick();button('Открыть видеомаски').click();await vi.waitFor(()=>expect(document.querySelector('[role="alert"]')).not.toBeNull());expect(getVideoMaskFrame).not.toHaveBeenCalled()})

it('retries a failed frame read without losing unsaved corrections on another ordinal',async()=>{
 const{onchanged}=await setup();key('Enter');await tick();vi.mocked(getVideoMaskFrame).mockRejectedValueOnce(new Error('offline')).mockResolvedValue(new Blob([new Uint8Array(64)]));const input=document.querySelector<HTMLInputElement>('[aria-label="Кадр видеомаски"]')!;input.value='2';input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(button('Повторить загрузку кадра')).toBeDefined());expect(onchanged).toHaveBeenLastCalledWith(true);button('Повторить загрузку кадра').click();await vi.waitFor(()=>expect(canvas()).not.toBeNull());button('Пересчитать видеомаски').click();await vi.waitFor(()=>expect(startVideoCorrection).toHaveBeenCalled());expect(vi.mocked(startVideoCorrection).mock.calls[0]![2].map(edit=>edit.frame)).toEqual([0])
})

it('selects source time and replaces an exclusive range with one local Undo',async()=>{
 await setup(false,{numerator:4,denominator:1});const time=document.querySelector<HTMLInputElement>('[aria-label="Время кадра видеомаски"]')!;time.value='0,5';time.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(getVideoMaskFrame).toHaveBeenLastCalledWith('owner','original-job',2,expect.any(AbortSignal)));await vi.waitFor(()=>expect(canvas()).not.toBeNull());expect(time.value).toBe('0.5');key('Enter');await tick();
 const end=document.querySelector<HTMLInputElement>('[aria-label="Конец диапазона видеомаски"]')!;end.value='0.75';end.dispatchEvent(new Event('input',{bubbles:true}));await tick();button('Заменить правки диапазона').click();await tick();button('Отменить правку кадра').click();await tick();button('Пересчитать видеомаски').click();await vi.waitFor(()=>expect(startVideoCorrection).toHaveBeenCalled());expect(vi.mocked(startVideoCorrection).mock.calls[0]![2].map(edit=>edit.frame)).toEqual([2])
})
it('preserves range recipes through Redo and submits only intersecting frame ordinals',async()=>{
 await setup(false,{numerator:4,denominator:1});key('Enter');await tick();const start=document.querySelector<HTMLInputElement>('[aria-label="Начало диапазона видеомаски"]')!,end=document.querySelector<HTMLInputElement>('[aria-label="Конец диапазона видеомаски"]')!;start.value='0.251';start.dispatchEvent(new Event('input',{bubbles:true}));end.value='0.75';end.dispatchEvent(new Event('input',{bubbles:true}));await tick();button('Заменить правки диапазона').click();await tick();button('Отменить правку кадра').click();await tick();button('Повторить правку кадра').click();await tick();button('Пересчитать видеомаски').click();await vi.waitFor(()=>expect(startVideoCorrection).toHaveBeenCalled());expect(vi.mocked(startVideoCorrection).mock.calls[0]![2].map(edit=>edit.frame)).toEqual([0,1,2])
})
it('refuses end-of-video selection and invalid ranges without losing authored masks',async()=>{
 await setup(false,{numerator:4,denominator:1});key('Enter');await tick();const count=vi.mocked(getVideoMaskFrame).mock.calls.length,time=document.querySelector<HTMLInputElement>('[aria-label="Время кадра видеомаски"]')!;time.value='1';time.dispatchEvent(new Event('change',{bubbles:true}));await tick();expect(time.value).toBe('0');expect(getVideoMaskFrame).toHaveBeenCalledTimes(count);const end=document.querySelector<HTMLInputElement>('[aria-label="Конец диапазона видеомаски"]')!;end.value='2';end.dispatchEvent(new Event('input',{bubbles:true}));await tick();button('Заменить правки диапазона').click();await tick();expect(document.querySelector('[role="alert"]')).not.toBeNull();button('Пересчитать видеомаски').click();await vi.waitFor(()=>expect(startVideoCorrection).toHaveBeenCalled());expect(vi.mocked(startVideoCorrection).mock.calls[0]![2].map(edit=>edit.frame)).toEqual([0])
})
it('keeps ordinal authoring available for old results without a source clock',async()=>{await setup();expect(document.querySelector('[aria-label="Время кадра видеомаски"]')).toBeNull();await frame(3);expect(canvas()).not.toBeNull()})

it('paints over original source pixels and mask overlay without discarding pending edits',async()=>{
 let urls=0;vi.mocked(URL.createObjectURL).mockImplementation(()=>`blob:frame-${++urls}`);await setup();key('Enter');await tick();const view=document.querySelector<HTMLSelectElement>('[aria-label="Основа полотна видеомаски"]')!;
 view.value='source';view.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(canvas()?.querySelector('image')?.getAttribute('href')).toBe('blob:frame-3'));expect(getVideoSourceFrame).toHaveBeenCalledWith('owner','base',0,expect.any(AbortSignal));expect(canvas().querySelectorAll('image')).toHaveLength(1);expect(canvas().querySelectorAll('path')).toHaveLength(1);
 view.value='overlay';view.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(canvas()?.querySelectorAll('image')).toHaveLength(2));expect(canvas().querySelectorAll('image')[0]!.getAttribute('href')).toBe('blob:frame-5');expect(canvas().querySelectorAll('image')[1]!.getAttribute('opacity')).toBe('0.45');expect(URL.revokeObjectURL).toHaveBeenCalledWith('blob:frame-3');expect(canvas().querySelectorAll('path')).toHaveLength(1);button('Пересчитать видеомаски').click();await vi.waitFor(()=>expect(startVideoCorrection).toHaveBeenCalled());expect(vi.mocked(startVideoCorrection).mock.calls[0]![2].map(edit=>edit.frame)).toEqual([0])
})
it('can return to mask view after a failed source read while retaining dirty strokes',async()=>{
 const{onchanged}=await setup();key('Enter');await tick();vi.mocked(getVideoSourceFrame).mockRejectedValueOnce(new Error('source offline'));const view=document.querySelector<HTMLSelectElement>('[aria-label="Основа полотна видеомаски"]')!;view.value='source';view.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(button('Повторить загрузку кадра')).toBeDefined());expect(document.querySelector('[role="alert"]')?.textContent).toBe('source offline');view.value='mask';view.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(canvas()).not.toBeNull());expect(canvas().querySelectorAll('path')).toHaveLength(1);expect(onchanged).toHaveBeenLastCalledWith(true)
})
it('aborts late source pixels on account changes before creating source URLs',async()=>{
 await setup(true);let resolve!:(blob:Blob)=>void;vi.mocked(getVideoSourceFrame).mockImplementation(()=>new Promise(r=>{resolve=r}));const view=document.querySelector<HTMLSelectElement>('[aria-label="Основа полотна видеомаски"]')!;view.value='source';view.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(getVideoSourceFrame).toHaveBeenCalled());const signal=vi.mocked(getVideoSourceFrame).mock.calls[0]![3]!;vi.mocked(URL.createObjectURL).mockClear();button('Сменить владельца видео').click();await tick();expect(signal.aborted).toBe(true);resolve(new Blob([new Uint8Array(64)]));await tick();expect(URL.createObjectURL).not.toHaveBeenCalled();expect(canvas()).toBeNull()
})

it('reopens inventory-bound strokes and preserves them through a plain edit, Undo/Redo and submission',async()=>{
 const value=videoRegionRecipe();vi.mocked(getVideoCorrectionRecipe).mockResolvedValue(value)
 const{onchanged}=await setup();await frame(1);expect(onchanged).toHaveBeenLastCalledWith(false)
 expect(canvas().querySelectorAll('path')).toHaveLength(1);key('Enter');await tick();expect(onchanged).toHaveBeenLastCalledWith(true)
 button('Отменить правку кадра').click();await tick();expect(onchanged).toHaveBeenLastCalledWith(false)
 button('Повторить правку кадра').click();await tick();button('Пересчитать видеомаски').click();await vi.waitFor(()=>expect(startVideoCorrection).toHaveBeenCalled())
 const submitted=vi.mocked(startVideoCorrection).mock.calls[0]![2][0]!.correction.strokes
 expect(submitted).toHaveLength(2);expect(submitted[0]!.region).toEqual(value.corrections[0]!.correction.strokes[0]!.region);expect(submitted[1]!.region).toBeUndefined()
})

it('previews the actual selected ordinal and authors explicit saved-region keep/remove strokes',async()=>{
 const value=videoRegionRecipe();vi.mocked(getVideoCorrectionRecipe).mockResolvedValue(value);const{onchanged}=await setup();await frame(1)
 button('Показать область кадра').click();await vi.waitFor(()=>expect(document.querySelector('image[data-video-region-preview]')).not.toBeNull());expect(getVideoRegionFrame).toHaveBeenLastCalledWith('owner','base','tracked',1,expect.any(AbortSignal));expect(onchanged).toHaveBeenLastCalledWith(false)
 button('Сохранить область кадра').click();await tick();expect(onchanged).toHaveBeenLastCalledWith(true)
 await frame(2);expect(button('Удалить область кадра').disabled).toBe(true);expect(document.querySelector('image[data-video-region-preview]')).toBeNull()
 vi.mocked(getVideoRegionFrame).mockResolvedValueOnce({png:new Blob([new Uint8Array(64)]),maskHash:'5'.repeat(64)});button('Показать область кадра').click();await vi.waitFor(()=>expect(button('Удалить область кадра').disabled).toBe(false));button('Удалить область кадра').click();await tick()
 button('Пересчитать видеомаски').click();await vi.waitFor(()=>expect(startVideoCorrection).toHaveBeenCalled());const submitted=vi.mocked(startVideoCorrection).mock.calls[0]![2]
 expect(submitted.find(e=>e.frame===1)!.correction.strokes.at(-1)).toMatchObject({tool:'smartBrush',region:{maskSourceId:'tracked',maskHash:'4'.repeat(64)}})
 expect(submitted.find(e=>e.frame===2)!.correction.strokes.at(-1)).toMatchObject({tool:'smartEraser',region:{maskSourceId:'tracked',maskHash:'5'.repeat(64)}})
})
it.each(['frame','owner'])('aborts private region reads on %s change and suppresses late object URLs',async mode=>{
 vi.mocked(getVideoCorrectionRecipe).mockResolvedValue(videoRegionRecipe());await setup(mode==='owner');let resolve!:(value:{png:Blob;maskHash:string})=>void;vi.mocked(getVideoRegionFrame).mockImplementation(()=>new Promise(r=>{resolve=r}));button('Показать область кадра').click();await vi.waitFor(()=>expect(getVideoRegionFrame).toHaveBeenCalled());const signal=vi.mocked(getVideoRegionFrame).mock.calls[0]![4]!
 if(mode==='owner'){button('Сменить владельца видео').click();await tick()}else{await frame(2)}
 expect(signal.aborted).toBe(true);vi.mocked(URL.createObjectURL).mockClear();resolve({png:new Blob([new Uint8Array(64)]),maskHash:'4'.repeat(64)});await tick();expect(URL.createObjectURL).not.toHaveBeenCalled();expect(document.querySelector('image[data-video-region-preview]')).toBeNull()
})
it('retains unsaved plain strokes on region read failure and can retry or hide the preview',async()=>{
 vi.mocked(getVideoCorrectionRecipe).mockResolvedValue(videoRegionRecipe());await setup();key('Enter');await tick();vi.mocked(getVideoRegionFrame).mockRejectedValueOnce(new Error('region offline'));button('Показать область кадра').click();await vi.waitFor(()=>expect(document.querySelector('[role="alert"]')?.textContent).toContain('region offline'));button('Показать область кадра').click();await vi.waitFor(()=>expect(document.querySelector('image[data-video-region-preview]')).not.toBeNull());button('Скрыть область').click();await tick();expect(document.querySelector('image[data-video-region-preview]')).toBeNull();button('Пересчитать видеомаски').click();await vi.waitFor(()=>expect(startVideoCorrection).toHaveBeenCalled());expect(vi.mocked(startVideoCorrection).mock.calls[0]![2].map(e=>e.frame).sort()).toEqual([0,1]);expect(URL.revokeObjectURL).toHaveBeenCalledWith('blob:mask')
})

it.each(['pointer','keyboard'])('picks a source point by %s without a brush edit and launches exact-ordinal tracking',async(kind)=>{
 const value=videoRegionRecipe(),snapshot=value.projectOrigin!.snapshot,region=snapshot.trackedRegions![0]!
 vi.mocked(getVideoCorrectionRecipe).mockResolvedValue({...value,corrections:[]})
 vi.mocked(getSamStatus).mockResolvedValue({available:true,engine:'sam',stillImages:true,video:true});vi.mocked(startSamVideoTracking).mockResolvedValue({jobId:'tracking'});vi.mocked(observeEnhancement).mockResolvedValue({id:'tracking',status:'done'})
 const base={kind:'source' as const,filename:'region.mkv',url:'/private',createdAt:0,mediaType:'video' as const}
 const entry={source:{...base,id:region.sourceId},mask:{...base,id:region.maskSourceId,width:256,height:256},snapshot:region};vi.mocked(saveSamVideoRegion).mockResolvedValue(entry)
 const ontracked=vi.fn().mockResolvedValue(undefined),onchanged=vi.fn()
 component=mount(Editor,{target:document.body,props:{token:'owner',jobId:'base',width:37,height:23,frames:4,frameRate:snapshot.frameRate,onready:vi.fn(),onchanged,ontracked}});await tick();button('Открыть видеомаски').click();await vi.waitFor(()=>expect(canvas()).not.toBeNull());await frame(1)
 const view=document.querySelector<HTMLSelectElement>('[aria-label="Основа полотна видеомаски"]')!;view.value='source';view.dispatchEvent(new Event('change',{bubbles:true}));await tick();await vi.waitFor(()=>expect(canvas()?.querySelector('image')?.getAttribute('href')).toBe('blob:mask'))
 vi.spyOn(canvas(),'getBoundingClientRect').mockReturnValue({left:0,top:0,width:100,height:100,right:100,bottom:100,x:0,y:0,toJSON(){return{}}})
 button('Выбрать точку объекта SAM').click();await tick();if(kind==='pointer')canvas().dispatchEvent(new PointerEvent('pointerdown',{button:0,clientX:40,clientY:50,pointerId:1,bubbles:true}));else key('Enter');await tick()
 expect(canvas().querySelectorAll('path')).toHaveLength(0);expect(onchanged).toHaveBeenLastCalledWith(false)
 button('Проверить SAM').click();await vi.waitFor(()=>expect(button('Отследить объект от выбранного кадра').disabled).toBe(false));button('Отследить объект от выбранного кадра').click()
 await vi.waitFor(()=>expect(ontracked).toHaveBeenCalledWith(entry,snapshot,'tracking',[],{frame:1,view:'source'}));expect(startSamVideoTracking).toHaveBeenCalledWith('owner','original',[[kind==='pointer'?.4:.5,.5]],1,expect.any(AbortSignal));expect(startVideoCorrection).not.toHaveBeenCalled()
})

it('prepares all region ordinals before a single range Undo/Redo and correction submission',async()=>{
 const value=videoRegionRecipe();vi.mocked(getVideoCorrectionRecipe).mockResolvedValue(value);await setup(false,{numerator:4,denominator:1});await frame(1)
 vi.mocked(getVideoRegionFrame).mockImplementation(async(_token,_job,_id,index)=>({png:new Blob(['unused']),maskHash:String(index+4).repeat(64)}))
 button('Заменить правки диапазона').click();await vi.waitFor(()=>expect(getVideoRegionFrame).toHaveBeenCalledTimes(4));await vi.waitFor(()=>expect(button('Пересчитать видеомаски').disabled).toBe(false))
 button('Отменить правку кадра').click();await tick();expect(button('Пересчитать видеомаски').disabled).toBe(true);button('Повторить правку кадра').click();await tick();button('Пересчитать видеомаски').click();await vi.waitFor(()=>expect(startVideoCorrection).toHaveBeenCalled())
 const edits=vi.mocked(startVideoCorrection).mock.calls[0]![2];expect(edits.map(e=>e.frame)).toEqual([0,1,2,3]);expect(edits.map(e=>e.correction.strokes[0]!.region!.maskHash)).toEqual([4,5,6,7].map(n=>String(n).repeat(64)))
})
it('cancels region range preparation and ignores a late frame without an edit or Undo record',async()=>{
 const value=videoRegionRecipe();vi.mocked(getVideoCorrectionRecipe).mockResolvedValue(value);await setup(false,{numerator:4,denominator:1});await frame(1)
 let resolve!:(value:{png:Blob;maskHash:string})=>void;vi.mocked(getVideoRegionFrame).mockImplementation(()=>new Promise(r=>{resolve=r}))
 button('Заменить правки диапазона').click();await vi.waitFor(()=>expect(getVideoRegionFrame).toHaveBeenCalledTimes(1));const signal=vi.mocked(getVideoRegionFrame).mock.calls[0]![4]!
 expect(button('Пересчитать видеомаски').disabled).toBe(true);button('Отменить применение диапазона').click();expect(signal.aborted).toBe(true);resolve({png:new Blob(['late']),maskHash:'4'.repeat(64)});await tick();await vi.waitFor(()=>expect(button('Отменить применение диапазона')).toBeUndefined())
 expect(button('Отменить правку кадра').disabled).toBe(true);expect(button('Пересчитать видеомаски').disabled).toBe(true);expect(getVideoRegionFrame).toHaveBeenCalledTimes(1)
})
it('keeps the prior recipe when a later range-plane request fails',async()=>{
 const value=videoRegionRecipe();vi.mocked(getVideoCorrectionRecipe).mockResolvedValue(value);await setup(false,{numerator:4,denominator:1});await frame(1)
 vi.mocked(getVideoRegionFrame).mockResolvedValueOnce({png:new Blob(['plane']),maskHash:'4'.repeat(64)}).mockRejectedValueOnce(new Error('region offline'))
 button('Заменить правки диапазона').click();await vi.waitFor(()=>expect(document.querySelector('[role="alert"]')?.textContent).toBe('region offline'));expect(button('Отменить правку кадра').disabled).toBe(true);expect(button('Пересчитать видеомаски').disabled).toBe(true)
 expect(canvas().querySelectorAll('path')).toHaveLength(1)
})

it('aborts pending range bindings on unmount without creating a correction',async()=>{
 const value=videoRegionRecipe();vi.mocked(getVideoCorrectionRecipe).mockResolvedValue(value);const {onready}=await setup(false,{numerator:4,denominator:1});await frame(1)
 let resolve!:(value:{png:Blob;maskHash:string})=>void;vi.mocked(getVideoRegionFrame).mockImplementation(()=>new Promise(r=>{resolve=r}))
 button('Заменить правки диапазона').click();await vi.waitFor(()=>expect(getVideoRegionFrame).toHaveBeenCalledTimes(1));const signal=vi.mocked(getVideoRegionFrame).mock.calls[0]![4]!
 expect(button('Следующий кадр маски').disabled).toBe(true);expect(button('Предыдущий кадр маски').disabled).toBe(true)
 await unmount(component!);component=undefined;expect(signal.aborted).toBe(true);resolve({png:new Blob(['late']),maskHash:'4'.repeat(64)});await tick();expect(onready).not.toHaveBeenCalled();expect(startVideoCorrection).not.toHaveBeenCalled();expect(getVideoRegionFrame).toHaveBeenCalledTimes(1)
})

it('retains an explicitly opened brush disclosure through recipe and frame updates',async()=>{
 const onready=vi.fn(),onchanged=vi.fn();component=mount(Editor,{target:document.body,props:{token:'owner',jobId:'base',width:37,height:23,frames:4,onready,onchanged}});await tick();const details=document.querySelector('details')!;details.open=true;details.dispatchEvent(new Event('toggle'));await tick();button('Открыть видеомаски').click();await vi.waitFor(()=>expect(canvas()).not.toBeNull());expect(details.open).toBe(true);await frame(1);expect(details.open).toBe(true)
})

it('falls back to a safe initial ordinal and mask view for invalid requested navigation',async()=>{
 component=mount(Editor,{target:document.body,props:{token:'owner',jobId:'base',width:37,height:23,frames:4,autoOpen:true,initialFrame:99,initialView:'source',onready:vi.fn(),onchanged:vi.fn()}});await tick();await vi.waitFor(()=>expect(canvas()).not.toBeNull());expect(document.querySelector<HTMLInputElement>('[aria-label="Кадр видеомаски"]')?.value).toBe('0');expect(document.querySelector<HTMLSelectElement>('[aria-label="Основа полотна видеомаски"]')?.value).toBe('mask');expect(getVideoMaskFrame).toHaveBeenCalledWith('owner','original-job',0,expect.any(AbortSignal));expect(getVideoSourceFrame).not.toHaveBeenCalled()
})

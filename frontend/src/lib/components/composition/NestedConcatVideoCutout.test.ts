import * as api from '$lib/api'
// @ts-expect-error Tests use the Svelte client entry.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {beforeEach,afterEach,expect,it,vi} from 'vitest'
import {nestedComposition,nestedHistory,reconcileNestedSavedVideoSmartRegion,startNestedSavedVideoSmartRegion,attachNestedSavedVideoSmartRegion,analyseNestedSavedVideoCutout,previewNestedSavedVideoCutout,setNestedSavedVideoCutout} from '$lib/state/nestedComposition.svelte'
import Panel from './NestedConcatVideoCutout.svelte'
vi.mock('$lib/state/nestedComposition.svelte',()=>({nestedComposition:{projectId:'saved',revision:7,timeUs:100000},nestedHistory:{busy:false,error:''},reconcileNestedSavedVideoSmartRegion:vi.fn(),startNestedSavedVideoSmartRegion:vi.fn(),attachNestedSavedVideoSmartRegion:vi.fn(),analyseNestedSavedVideoCutout:vi.fn(),previewNestedSavedVideoCutout:vi.fn(),setNestedSavedVideoCutout:vi.fn()}))
let component:ReturnType<typeof mount>|undefined
beforeEach(()=>{vi.spyOn(api,'cancelNestedVideoSmartOperation').mockResolvedValue({cancelled:true});nestedHistory.error='';vi.mocked(reconcileNestedSavedVideoSmartRegion).mockResolvedValue('unapplied');const data=new Map<string,string>();vi.stubGlobal('localStorage',{getItem:(key:string)=>data.get(key)??null,setItem:(key:string,value:string)=>{data.set(key,value)},removeItem:(key:string)=>{data.delete(key)},clear:()=>data.clear()})})
afterEach(async()=>{if(component)await unmount(component);component=undefined;document.body.innerHTML='';vi.restoreAllMocks();vi.resetAllMocks();vi.unstubAllGlobals();Object.assign(nestedComposition,{document:null,projectId:'saved',revision:7,timeUs:100000})})
it('analyses and paints the returned selected frame',async()=>{
 const putImageData=vi.fn();vi.spyOn(HTMLCanvasElement.prototype,'getContext').mockReturnValue({createImageData:()=>({data:new Uint8ClampedArray(4)}),putImageData} as unknown as CanvasRenderingContext2D)
 vi.mocked(analyseNestedSavedVideoCutout).mockResolvedValue({sourceId:'raw',analysedRuns:1,analysedFrames:4,emptyRuns:0,cacheReused:false});vi.mocked(previewNestedSavedVideoCutout).mockResolvedValue({width:1,height:1,rgba:new Uint8ClampedArray([0,255,0,128])})
 component=mount(Panel,{target:document.body,props:{token:'owner',compositionId:'timeline',clipId:'video'}});await tick();document.querySelector('button')!.click();await vi.waitFor(()=>expect(putImageData).toHaveBeenCalled());expect(analyseNestedSavedVideoCutout).toHaveBeenCalledWith('owner','timeline','video',expect.any(AbortSignal),'auto');expect(previewNestedSavedVideoCutout).toHaveBeenCalledWith('owner','timeline','video',100000,expect.any(AbortSignal),0.01);expect(putImageData.mock.calls[0]![0].data).toEqual(new Uint8ClampedArray([0,255,0,128]));expect(document.querySelector('canvas')!.hidden).toBe(false)
})
it('cancels pending analysis and never starts preview after cancellation',async()=>{let finish!:(value:null)=>void;vi.mocked(analyseNestedSavedVideoCutout).mockReturnValue(new Promise(resolve=>{finish=resolve}));component=mount(Panel,{target:document.body,props:{token:'owner',compositionId:'timeline',clipId:'video'}});await tick();document.querySelector('button')!.click();await tick();const signal=vi.mocked(analyseNestedSavedVideoCutout).mock.calls[0]![3]!;Array.from(document.querySelectorAll('button')).find(b=>b.textContent==='Отменить обработку видео')!.click();expect(signal.aborted).toBe(true);finish(null);await tick();expect(previewNestedSavedVideoCutout).not.toHaveBeenCalled()})

it('enables the saved clip from the inspector without rerunning analysis',async()=>{
 vi.mocked(setNestedSavedVideoCutout).mockResolvedValue(true)
 component=mount(Panel,{target:document.body,props:{token:'owner',compositionId:'timeline',clipId:'video'}});await tick()
 Array.from(document.querySelectorAll('button')).find(button=>button.textContent==='Включить вырезание в проекте')!.click();await vi.waitFor(()=>expect(setNestedSavedVideoCutout).toHaveBeenCalledWith('owner','timeline','video',true,expect.any(AbortSignal),'auto',undefined,0.01));expect(analyseNestedSavedVideoCutout).not.toHaveBeenCalled();expect(previewNestedSavedVideoCutout).not.toHaveBeenCalled()
})

it('reapplies a newly analysed model to an enabled clip without disabling it',async()=>{
 const person='88d4531297118f595bf2fd60f6f566aec2e559393802d1f436c380f0cbbd2828',object='60920e99c45464f2ba57bee2ad08c919a52bbf852739e96947fbb4358c0d964a'
 Object.assign(nestedComposition,{document:{compositions:{timeline:{content:{tracks:[{kind:'video',clips:[{id:'video',concatCutout:{subject:'auto',modelHash:person,sourceHash:'a'.repeat(64),runtimeHash:'b'.repeat(64)}}]}]}}}}})
 vi.spyOn(HTMLCanvasElement.prototype,'getContext').mockReturnValue({createImageData:()=>({data:new Uint8ClampedArray(4)}),putImageData:vi.fn()} as unknown as CanvasRenderingContext2D)
 vi.mocked(analyseNestedSavedVideoCutout).mockResolvedValue({subject:'auto',modelHash:object,sourceId:'raw',analysedRuns:1,analysedFrames:2,emptyRuns:0,cacheReused:false});vi.mocked(previewNestedSavedVideoCutout).mockResolvedValue({width:1,height:1,rgba:new Uint8ClampedArray(4)});vi.mocked(setNestedSavedVideoCutout).mockResolvedValue(true)
 component=mount(Panel,{target:document.body,props:{token:'owner',compositionId:'timeline',clipId:'video'}});await tick();document.querySelector('button')!.click();await vi.waitFor(()=>expect(Array.from(document.querySelectorAll('button')).some(button=>button.textContent==='Применить настройки вырезания')).toBe(true))
 await vi.waitFor(()=>expect(document.querySelector('fieldset')!.disabled).toBe(false));const selection=document.querySelector('select')!;selection.value='person';selection.dispatchEvent(new Event('change',{bubbles:true}));await tick();expect(Array.from(document.querySelectorAll('button')).some(button=>button.textContent==='Применить настройки вырезания')).toBe(false);selection.value='auto';selection.dispatchEvent(new Event('change',{bubbles:true}));await tick()
 Array.from(document.querySelectorAll('button')).find(button=>button.textContent==='Применить настройки вырезания')!.click();await vi.waitFor(()=>expect(setNestedSavedVideoCutout).toHaveBeenCalledWith('owner','timeline','video',true,expect.any(AbortSignal),'auto',object,0));expect(vi.mocked(setNestedSavedVideoCutout).mock.calls.every(call=>call[3]===true)).toBe(true)
})

it('reports an empty source range without showing an older cached family',async()=>{
 vi.mocked(analyseNestedSavedVideoCutout).mockResolvedValue({subject:'auto',modelHash:null,sourceId:'raw',analysedRuns:0,analysedFrames:0,emptyRuns:1,cacheReused:false})
 component=mount(Panel,{target:document.body,props:{token:'owner',compositionId:'timeline',clipId:'video'}});await tick();document.querySelector('button')!.click();await vi.waitFor(()=>expect(document.body.textContent).toContain('В выбранном диапазоне нет кадров'));expect(previewNestedSavedVideoCutout).not.toHaveBeenCalled();expect(document.querySelector('canvas')!.hidden).toBe(true)
})

it('previews and applies changed edge softness without rerunning the model',async()=>{
 const person='88d4531297118f595bf2fd60f6f566aec2e559393802d1f436c380f0cbbd2828'
 Object.assign(nestedComposition,{document:{compositions:{timeline:{content:{tracks:[{kind:'video',clips:[{id:'video',concatCutout:{subject:'person',modelHash:person,sourceHash:'a'.repeat(64),runtimeHash:'b'.repeat(64),feather:0.01}}]}]}}}}})
 vi.spyOn(HTMLCanvasElement.prototype,'getContext').mockReturnValue({createImageData:()=>({data:new Uint8ClampedArray(4)}),putImageData:vi.fn()} as unknown as CanvasRenderingContext2D);vi.mocked(previewNestedSavedVideoCutout).mockResolvedValue({width:1,height:1,rgba:new Uint8ClampedArray(4)});vi.mocked(setNestedSavedVideoCutout).mockResolvedValue(true)
 component=mount(Panel,{target:document.body,props:{token:'owner',compositionId:'timeline',clipId:'video'}});await tick();const input=document.querySelector('input')!;expect(input.value).toBe('0.01');input.value='0.2';input.dispatchEvent(new Event('input',{bubbles:true}));await tick();expect(Array.from(document.querySelectorAll('button')).find(button=>button.textContent==='Применить настройки вырезания')!.disabled).toBe(true);input.value='0.025';input.dispatchEvent(new Event('input',{bubbles:true}));await tick()
 Array.from(document.querySelectorAll('button')).find(button=>button.textContent==='Показать готовый кадр на курсоре')!.click();await vi.waitFor(()=>expect(previewNestedSavedVideoCutout).toHaveBeenCalledWith('owner','timeline','video',100000,expect.any(AbortSignal),0.025));await vi.waitFor(()=>expect(document.querySelector('fieldset')!.disabled).toBe(false));Array.from(document.querySelectorAll('button')).find(button=>button.textContent==='Применить настройки вырезания')!.click();await vi.waitFor(()=>expect(setNestedSavedVideoCutout).toHaveBeenCalledWith('owner','timeline','video',true,expect.any(AbortSignal),'person',person,0.025));expect(analyseNestedSavedVideoCutout).not.toHaveBeenCalled()
})

it('paints normalized source strokes and applies them with the saved settings',async()=>{
 vi.spyOn(HTMLCanvasElement.prototype,'getContext').mockReturnValue({createImageData:()=>({data:new Uint8ClampedArray(8)}),putImageData:vi.fn()} as unknown as CanvasRenderingContext2D)
 vi.mocked(previewNestedSavedVideoCutout).mockResolvedValue({width:2,height:1,rgba:new Uint8ClampedArray(8)});vi.mocked(setNestedSavedVideoCutout).mockResolvedValue(true)
 component=mount(Panel,{target:document.body,props:{token:'owner',compositionId:'timeline',clipId:'video'}});await tick()
 const edit=document.querySelector<HTMLInputElement>('input[type="checkbox"]')!;edit.click();await tick()
 Array.from(document.querySelectorAll('button')).find(b=>b.textContent==='Показать готовый кадр на курсоре')!.click()
 await vi.waitFor(()=>expect(document.querySelector('canvas')!.hidden).toBe(false))
 expect(previewNestedSavedVideoCutout).toHaveBeenLastCalledWith('owner','timeline','video',100000,expect.any(AbortSignal),0.01,undefined,true)
 const canvas=document.querySelector('canvas')!;Object.assign(canvas,{setPointerCapture:vi.fn(),hasPointerCapture:()=>true,releasePointerCapture:vi.fn(),getBoundingClientRect:()=>({left:20,top:10,width:200,height:100})})
 const pointer=(type:string,x:number,y:number)=>{const event=new MouseEvent(type,{bubbles:true,button:0,clientX:x,clientY:y});Object.defineProperty(event,'pointerId',{value:1});canvas.dispatchEvent(event)}
 pointer('pointerdown',70,60);pointer('pointermove',170,60);pointer('pointerup',170,60)
 await vi.waitFor(()=>expect(previewNestedSavedVideoCutout).toHaveBeenCalledTimes(2));await vi.waitFor(()=>expect(document.querySelector('fieldset')!.disabled).toBe(false))
 const correction={custom:true,feather:0.01,strokes:[{tool:'brush',size:0.1,points:[[0.25,0.5],[0.75,0.5]],at:null}]}
 expect(previewNestedSavedVideoCutout).toHaveBeenLastCalledWith('owner','timeline','video',100000,expect.any(AbortSignal),0.01,correction,true)
 Array.from(document.querySelectorAll('button')).find(b=>b.textContent==='Включить вырезание в проекте')!.click()
 await vi.waitFor(()=>expect(setNestedSavedVideoCutout).toHaveBeenLastCalledWith('owner','timeline','video',true,expect.any(AbortSignal),'auto',undefined,0.01,correction))
})

it('shows the excluded mask over original pixels without changing authored settings',async()=>{
 const putImageData=vi.fn();vi.spyOn(HTMLCanvasElement.prototype,'getContext').mockReturnValue({createImageData:()=>({data:new Uint8ClampedArray(4)}),putImageData} as unknown as CanvasRenderingContext2D)
 vi.mocked(previewNestedSavedVideoCutout).mockResolvedValueOnce({width:1,height:1,rgba:new Uint8ClampedArray([10,20,30,0])}).mockResolvedValueOnce({width:1,height:1,rgba:new Uint8ClampedArray([10,20,30,255])})
 component=mount(Panel,{target:document.body,props:{token:'owner',compositionId:'timeline',clipId:'video'}});await tick();document.querySelector<HTMLInputElement>('input[type="checkbox"]')!.click();await tick()
 const view=document.querySelector<HTMLSelectElement>('select[aria-label="Вид видеомаски"]')!;view.value='overlay';view.dispatchEvent(new Event('change',{bubbles:true}));await tick()
 Array.from(document.querySelectorAll('button')).find(b=>b.textContent==='Показать готовый кадр на курсоре')!.click();await vi.waitFor(()=>expect(putImageData).toHaveBeenCalled())
 expect(putImageData.mock.calls.at(-1)![0].data).toEqual(new Uint8ClampedArray([120,11,17,255]));expect(previewNestedSavedVideoCutout).toHaveBeenLastCalledWith('owner','timeline','video',100000,expect.any(AbortSignal),0.01,undefined,true,true);expect(setNestedSavedVideoCutout).not.toHaveBeenCalled()
})

it('clears temporal tracks together with strokes when restoring the automatic mask',async()=>{
 const model='88d4531297118f595bf2fd60f6f566aec2e559393802d1f436c380f0cbbd2828',snapshot={sourceId:'raw',maskSourceId:'region',sourceHash:'a'.repeat(64),maskHash:'b'.repeat(64),maskFileHash:'c'.repeat(64),encoderHash:'d'.repeat(64),decoderHash:'e'.repeat(64),runtimeHash:'f'.repeat(64),promptHash:'0'.repeat(64),points:[[.25,.5]],width:2,height:2,frames:1,frameRate:{numerator:10,denominator:1},seedFrame:0,trackedFrames:1,forwardLostAt:null,backwardLostAt:null}
 Object.assign(nestedComposition,{document:{compositions:{timeline:{content:{tracks:[{kind:'video',clips:[{id:'video',sourceId:'raw',concatCutout:{subject:'person',modelHash:model,sourceHash:'a'.repeat(64),runtimeHash:'b'.repeat(64),correction:{custom:true,feather:0,strokes:[{tool:'smartBrush',size:.1,points:snapshot.points,at:0}]},regionTracks:[{stroke:0,snapshot}]}}]}]}}}}})
 vi.spyOn(HTMLCanvasElement.prototype,'getContext').mockReturnValue({createImageData:()=>({data:new Uint8ClampedArray(4)}),putImageData:vi.fn()} as unknown as CanvasRenderingContext2D);vi.mocked(previewNestedSavedVideoCutout).mockResolvedValue({width:1,height:1,rgba:new Uint8ClampedArray(4)});vi.mocked(setNestedSavedVideoCutout).mockResolvedValue(true)
 component=mount(Panel,{target:document.body,props:{token:'owner',compositionId:'timeline',clipId:'video'}});await tick();document.querySelector<HTMLInputElement>('input[type="checkbox"]')!.click();await tick()
 Array.from(document.querySelectorAll('button')).find(b=>b.textContent==='Вернуть автоматическую маску')!.click();await vi.waitFor(()=>expect(previewNestedSavedVideoCutout).toHaveBeenCalled())
 expect(previewNestedSavedVideoCutout).toHaveBeenLastCalledWith('owner','timeline','video',100000,expect.any(AbortSignal),0,{custom:false,strokes:[],feather:0},true,undefined,[])
 await vi.waitFor(()=>expect(document.querySelector('fieldset')!.disabled).toBe(false));Array.from(document.querySelectorAll('button')).find(b=>b.textContent==='Применить настройки вырезания')!.click();await vi.waitFor(()=>expect(setNestedSavedVideoCutout).toHaveBeenLastCalledWith('owner','timeline','video',true,expect.any(AbortSignal),'person',model,0,{custom:false,strokes:[],feather:0},[]))
})

async function smartPanel(){
 const model='88d4531297118f595bf2fd60f6f566aec2e559393802d1f436c380f0cbbd2828';Object.assign(nestedComposition,{document:{compositions:{timeline:{content:{sources:{raw:{id:'raw',kind:'video'}},tracks:[{kind:'video',clips:[{id:'video',sourceId:'raw',concatCutout:{subject:'person',modelHash:model,sourceHash:'a'.repeat(64),runtimeHash:'b'.repeat(64)}}]}]}}}}})
 vi.spyOn(HTMLCanvasElement.prototype,'getContext').mockReturnValue({createImageData:()=>({data:new Uint8ClampedArray(4)}),putImageData:vi.fn()} as unknown as CanvasRenderingContext2D);vi.mocked(previewNestedSavedVideoCutout).mockResolvedValue({width:1,height:1,rgba:new Uint8ClampedArray(4)});vi.mocked(startNestedSavedVideoSmartRegion).mockResolvedValue({jobId:'smart-job',sourceId:'raw',sourceUs:100000,seedFrame:1,frames:3,frameRate:{numerator:10,denominator:1},expectedRevision:7})
 component=mount(Panel,{target:document.body,props:{token:'owner',compositionId:'timeline',clipId:'video'}});await tick();document.querySelector<HTMLInputElement>('input[type="checkbox"]')!.click();await tick();Array.from(document.querySelectorAll('button')).find(b=>b.textContent==='Показать готовый кадр на курсоре')!.click();await vi.waitFor(()=>expect(document.querySelector('canvas')!.hidden).toBe(false));await vi.waitFor(()=>expect(document.querySelector('fieldset')!.disabled).toBe(false))
 const select=document.querySelector<HTMLSelectElement>('select[aria-label="Кисть видеомаски"]')!;select.value='smartBrush';select.dispatchEvent(new Event('change',{bubbles:true}));await tick();const canvas=document.querySelector('canvas')!;Object.assign(canvas,{setPointerCapture:vi.fn(),hasPointerCapture:()=>true,releasePointerCapture:vi.fn(),getBoundingClientRect:()=>({left:20,top:10,width:200,height:100})})
 for(const type of ['pointerdown','pointerup']){const event=new MouseEvent(type,{bubbles:true,button:0,clientX:70,clientY:60});Object.defineProperty(event,'pointerId',{value:1});canvas.dispatchEvent(event)}
}
it('runs a smart stroke through observation and confirmed history attachment',async()=>{
 vi.spyOn(api,'observeNestedVideoSmartRegion').mockResolvedValue({id:'smart-job',status:'done'});vi.mocked(attachNestedSavedVideoSmartRegion).mockResolvedValue(true);await smartPanel();await vi.waitFor(()=>expect(attachNestedSavedVideoSmartRegion).toHaveBeenCalled())
 expect(startNestedSavedVideoSmartRegion).toHaveBeenCalledWith('owner','timeline','video',[[.25,.5]],expect.any(AbortSignal),expect.any(String));expect(attachNestedSavedVideoSmartRegion).toHaveBeenCalledWith('owner','timeline','video',100000,expect.objectContaining({jobId:'smart-job'}),[[.25,.5]],'smartBrush',.1,expect.any(AbortSignal));expect(setNestedSavedVideoCutout).not.toHaveBeenCalled()
})
it('explicitly cancels an owned smart job and never attaches its late result',async()=>{
 const observe=vi.spyOn(api,'observeNestedVideoSmartRegion').mockImplementation((_token,_id,signal)=>new Promise((_resolve,reject)=>signal!.addEventListener('abort',()=>reject(new DOMException('Aborted','AbortError')),{once:true}))),cancel=vi.mocked(api.cancelNestedVideoSmartOperation);await smartPanel();await vi.waitFor(()=>expect(observe).toHaveBeenCalled());Array.from(document.querySelectorAll('button')).find(b=>b.textContent==='Отменить обработку видео')!.click();await vi.waitFor(()=>expect(cancel).toHaveBeenCalledWith('saved',expect.any(String),'owner'));await vi.waitFor(()=>expect(document.querySelector('fieldset')!.disabled).toBe(false));expect(attachNestedSavedVideoSmartRegion).not.toHaveBeenCalled()
})

it('stops observation on unmount without cancelling the durable smart job',async()=>{
 const observe=vi.spyOn(api,'observeNestedVideoSmartRegion').mockImplementation((_token,_id,signal)=>new Promise((_resolve,reject)=>signal!.addEventListener('abort',()=>reject(new DOMException('Aborted','AbortError')),{once:true}))),cancel=vi.spyOn(api,'cancelNestedVideoSmartRegion').mockResolvedValue({ok:true});await smartPanel();await vi.waitFor(()=>expect(observe).toHaveBeenCalled());const signal=observe.mock.calls[0]![2]!;await unmount(component!);component=undefined;expect(signal.aborted).toBe(true);expect(cancel).not.toHaveBeenCalled();expect(attachNestedSavedVideoSmartRegion).not.toHaveBeenCalled()
})

it('resumes an accepted smart stroke after remount without starting another job',async()=>{
 const observe=vi.spyOn(api,'observeNestedVideoSmartRegion').mockImplementationOnce((_token,_id,signal)=>new Promise((_resolve,reject)=>signal!.addEventListener('abort',()=>reject(new DOMException('Aborted','AbortError')),{once:true}))).mockResolvedValueOnce({id:'smart-job',status:'done'});vi.mocked(attachNestedSavedVideoSmartRegion).mockResolvedValue(true)
 await smartPanel();await vi.waitFor(()=>expect(observe).toHaveBeenCalledTimes(1));await unmount(component!);component=undefined
 component=mount(Panel,{target:document.body,props:{token:'owner',compositionId:'timeline',clipId:'video'}});await tick();await vi.waitFor(()=>expect(Array.from(document.querySelectorAll('button')).some(b=>b.textContent==='Продолжить правку SAM')).toBe(true));const resume=Array.from(document.querySelectorAll('button')).find(b=>b.textContent==='Продолжить правку SAM')!;expect(resume.disabled).toBe(false);resume.click()
 await vi.waitFor(()=>expect(attachNestedSavedVideoSmartRegion).toHaveBeenCalledTimes(1));expect(startNestedSavedVideoSmartRegion).toHaveBeenCalledTimes(1);expect(observe.mock.calls[1]![1]).toBe('smart-job');await vi.waitFor(()=>expect(Array.from(document.querySelectorAll('button')).some(b=>b.textContent==='Продолжить правку SAM')).toBe(false))
})

it('retains a pending smart task but disables attachment to a changed revision',async()=>{
 const observe=vi.spyOn(api,'observeNestedVideoSmartRegion').mockImplementation((_token,_id,signal)=>new Promise((_resolve,reject)=>signal!.addEventListener('abort',()=>reject(new DOMException('Aborted','AbortError')),{once:true})));await smartPanel();await vi.waitFor(()=>expect(observe).toHaveBeenCalled());await unmount(component!);component=undefined;nestedComposition.revision=9
 component=mount(Panel,{target:document.body,props:{token:'owner',compositionId:'timeline',clipId:'video'}});await tick();await vi.waitFor(()=>expect(Array.from(document.querySelectorAll('button')).some(b=>b.textContent==='Продолжить правку SAM')).toBe(true));expect(Array.from(document.querySelectorAll('button')).find(b=>b.textContent==='Продолжить правку SAM')!.disabled).toBe(true);expect(startNestedSavedVideoSmartRegion).toHaveBeenCalledTimes(1);expect(attachNestedSavedVideoSmartRegion).not.toHaveBeenCalled()
})
it('finishes a smart stroke even if browser persistence refuses the pending record',async()=>{
 vi.spyOn(localStorage,'setItem').mockImplementation(()=>{throw new DOMException('Full','QuotaExceededError')});vi.spyOn(api,'observeNestedVideoSmartRegion').mockResolvedValue({id:'smart-job',status:'done'});vi.mocked(attachNestedSavedVideoSmartRegion).mockResolvedValue(true);await smartPanel();await vi.waitFor(()=>expect(attachNestedSavedVideoSmartRegion).toHaveBeenCalledTimes(1));expect(startNestedSavedVideoSmartRegion).toHaveBeenCalledTimes(1)
})

it('rechecks a pending record written after mount before submitting another SAM job',async()=>{
 vi.spyOn(localStorage,'getItem').mockReturnValueOnce(null).mockReturnValue('unknown accepted intent')
 await smartPanel();await vi.waitFor(()=>expect(nestedHistory.error).toContain('повторный запуск остановлен'))
 expect(startNestedSavedVideoSmartRegion).not.toHaveBeenCalled();expect(attachNestedSavedVideoSmartRegion).not.toHaveBeenCalled()
})

it('persists before acknowledgement and resumes with the same operation after remount',async()=>{
 const writes=vi.spyOn(localStorage,'setItem');vi.mocked(startNestedSavedVideoSmartRegion).mockImplementationOnce((_token,_node,_clip,_points,signal)=>new Promise((_resolve,reject)=>signal!.addEventListener('abort',()=>reject(new DOMException('Aborted','AbortError')),{once:true})))
 const observe=vi.spyOn(api,'observeNestedVideoSmartRegion').mockResolvedValue({id:'smart-job',status:'done'});vi.mocked(attachNestedSavedVideoSmartRegion).mockResolvedValue(true)
 await smartPanel();await vi.waitFor(()=>expect(startNestedSavedVideoSmartRegion).toHaveBeenCalledTimes(1))
 const [key,raw]=writes.mock.calls[0]!,draft=JSON.parse(raw);expect(draft.version).toBe(3);expect(draft.started).toBeUndefined();expect(draft.id).toBe(vi.mocked(startNestedSavedVideoSmartRegion).mock.calls[0]![5]);expect(observe).not.toHaveBeenCalled()
 await unmount(component!);component=undefined;expect(localStorage.getItem(key)).toBe(raw)
 component=mount(Panel,{target:document.body,props:{token:'owner',compositionId:'timeline',clipId:'video'}});await tick();const button=()=>Array.from(document.querySelectorAll('button')).find(b=>b.textContent==='Продолжить правку SAM');await vi.waitFor(()=>expect(button()?.disabled).toBe(false));button()!.click()
 await vi.waitFor(()=>expect(attachNestedSavedVideoSmartRegion).toHaveBeenCalledTimes(1));expect(startNestedSavedVideoSmartRegion).toHaveBeenCalledTimes(2);expect(vi.mocked(startNestedSavedVideoSmartRegion).mock.calls[1]![5]).toBe(draft.id);expect(localStorage.getItem(key)).toBeNull()
})
it('cancels an operation while its start response is still unknown',async()=>{
 const writes=vi.spyOn(localStorage,'setItem');vi.mocked(startNestedSavedVideoSmartRegion).mockImplementationOnce((_token,_node,_clip,_points,signal)=>new Promise((_resolve,reject)=>signal!.addEventListener('abort',()=>reject(new DOMException('Aborted','AbortError')),{once:true})))
 await smartPanel();await vi.waitFor(()=>expect(startNestedSavedVideoSmartRegion).toHaveBeenCalledTimes(1));const [key,raw]=writes.mock.calls[0]!,draft=JSON.parse(raw)
 Array.from(document.querySelectorAll('button')).find(b=>b.textContent==='Отменить обработку видео')!.click();await vi.waitFor(()=>expect(api.cancelNestedVideoSmartOperation).toHaveBeenCalledWith('saved',draft.id,'owner'));await vi.waitFor(()=>expect(localStorage.getItem(key)).toBeNull());expect(attachNestedSavedVideoSmartRegion).not.toHaveBeenCalled()
})

it('keeps an unacknowledged intent and refuses replay over local document changes',async()=>{
 const writes=vi.spyOn(localStorage,'setItem');vi.mocked(startNestedSavedVideoSmartRegion).mockImplementationOnce((_token,_node,_clip,_points,signal)=>new Promise((_resolve,reject)=>signal!.addEventListener('abort',()=>reject(new DOMException('Aborted','AbortError')),{once:true})))
 await smartPanel();await vi.waitFor(()=>expect(startNestedSavedVideoSmartRegion).toHaveBeenCalledTimes(1));const [key,raw]=writes.mock.calls[0]!
 await unmount(component!);component=undefined
 const changed=JSON.parse(JSON.stringify(nestedComposition.document));changed.compositions.timeline.content.tracks[0].name='Renamed locally';nestedComposition.document=changed
 component=mount(Panel,{target:document.body,props:{token:'owner',compositionId:'timeline',clipId:'video'}});await tick();const button=()=>Array.from(document.querySelectorAll('button')).find(b=>b.textContent==='Продолжить правку SAM');await vi.waitFor(()=>expect(button()?.disabled).toBe(false));button()!.click()
 await vi.waitFor(()=>expect(nestedHistory.error).toContain('Исходный документ запуска SAM изменился'));expect(startNestedSavedVideoSmartRegion).toHaveBeenCalledTimes(1);expect(attachNestedSavedVideoSmartRegion).not.toHaveBeenCalled();expect(localStorage.getItem(key)).toBe(raw)
})
it('retains the unacknowledged operation when its explicit cancellation fails',async()=>{
 vi.mocked(api.cancelNestedVideoSmartOperation).mockRejectedValue(new Error('offline cancellation'));const writes=vi.spyOn(localStorage,'setItem');vi.mocked(startNestedSavedVideoSmartRegion).mockImplementationOnce((_token,_node,_clip,_points,signal)=>new Promise((_resolve,reject)=>signal!.addEventListener('abort',()=>reject(new DOMException('Aborted','AbortError')),{once:true})))
 await smartPanel();await vi.waitFor(()=>expect(startNestedSavedVideoSmartRegion).toHaveBeenCalledTimes(1));const [key,raw]=writes.mock.calls[0]!
 Array.from(document.querySelectorAll('button')).find(b=>b.textContent==='Отменить обработку видео')!.click();await vi.waitFor(()=>expect(nestedHistory.error).toBe('offline cancellation'));expect(localStorage.getItem(key)).toBe(raw);expect(attachNestedSavedVideoSmartRegion).not.toHaveBeenCalled()
})
it('clears the lost-response error after a successful explicit cancellation',async()=>{
 vi.mocked(startNestedSavedVideoSmartRegion).mockRejectedValueOnce(new Error('lost acknowledgement'))
 await smartPanel();await vi.waitFor(()=>expect(nestedHistory.error).toBe('lost acknowledgement'))
 const cancel=()=>Array.from(document.querySelectorAll('button')).find(b=>b.textContent==='Отменить правку SAM');await vi.waitFor(()=>expect(cancel()?.disabled).toBe(false));cancel()!.click();await vi.waitFor(()=>expect(api.cancelNestedVideoSmartOperation).toHaveBeenCalled());await vi.waitFor(()=>expect(nestedHistory.error).toBe(''))
})

it('does not clear a newer editor error when cancellation completes',async()=>{
 let complete!:(value:unknown)=>void;vi.mocked(api.cancelNestedVideoSmartOperation).mockReturnValue(new Promise(resolve=>{complete=resolve}));vi.mocked(startNestedSavedVideoSmartRegion).mockRejectedValueOnce(new Error('lost acknowledgement'))
 await smartPanel();await vi.waitFor(()=>expect(nestedHistory.error).toBe('lost acknowledgement'));const cancel=()=>Array.from(document.querySelectorAll('button')).find(b=>b.textContent==='Отменить правку SAM');await vi.waitFor(()=>expect(cancel()?.disabled).toBe(false));cancel()!.click();await vi.waitFor(()=>expect(api.cancelNestedVideoSmartOperation).toHaveBeenCalled());nestedHistory.error='new editor error';complete({cancelled:true});await vi.waitFor(()=>expect(cancel()).toBeUndefined());expect(nestedHistory.error).toBe('new editor error')
})

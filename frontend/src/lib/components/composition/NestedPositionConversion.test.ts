// @ts-expect-error Mounted tests require the Svelte client entry.
import { mount,tick,unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { beforeEach,afterEach,it,expect,vi } from 'vitest'
import { previewPositionConversion,evaluateNestedFrame,type NativePositionConversionResult } from '$lib/api'
import { nestedComposition,nestedHistory,closeNestedComposition,undoNestedComposition,redoNestedComposition } from '$lib/state/nestedComposition.svelte'
import { nestedEffectPreview } from '$lib/state/nestedEffectPreview.svelte'
import { authState } from '$lib/state/auth.svelte'
import { agentSession } from '$lib/agent/session.svelte'
import { createComposition,addTrack,addClip } from '$lib/composition/commands'
import { buildCompositionWireDocument } from '$lib/composition/payload'
import Inspector from './NestedMotionPathInspector.svelte'
vi.mock('$lib/api',async original=>({...await original<typeof import('$lib/api')>(),previewPositionConversion:vi.fn(),evaluateNestedFrame:vi.fn()}))
let component:ReturnType<typeof mount>|undefined
const key=(t_us:number,value:number)=>({t_us,value,in_:[2/3,2/3] as const,out:[1/3,1/3] as const,segment:{kind:'Linear' as const}})
const scalar=(a:number,b:number)=>({keys:[key(0,a),key(1000000,b)],extrapolate:{before:'Hold' as const,after:'Hold' as const}})
const answer:NativePositionConversionResult={nodes:null,tracks:[scalar(0,40),scalar(15,15)],sampleCount:2,nodeCount:0,maxErrorPx:0.2,checkCount:121,withinTolerance:true,limit:null}
const node=(id:string,x:number)=>({id,point:{x,y:15},inHandle:{x:0,y:0},outHandle:{x:0,y:0},segment:'line' as const,tangentMode:'corner' as const})
beforeEach(()=>{
 closeNestedComposition();nestedEffectPreview.value=null;authState.token='conversion-session'
 vi.mocked(previewPositionConversion).mockReset();vi.mocked(previewPositionConversion).mockResolvedValue(answer)
 vi.mocked(evaluateNestedFrame).mockReset();vi.mocked(evaluateNestedFrame).mockResolvedValue({entries:[]})
 let doc=createComposition({width:64,height:48,fps:30,backgroundColor:'#000000FF'})
 doc=addTrack(doc,{id:'text',kind:'text',name:'Text',locked:false,hidden:false,clips:[]})
 doc=addClip(doc,'text',{id:'title',kind:'text',text:'Path',timelineStartTicks:1000000,durationTicks:2000000,x:17,y:23,opacity:1,style:{fontSizePx:24,color:'#FFFFFFFF',align:'center'}})
 const content=buildCompositionWireDocument(doc),clip=content.tracks[0]!.clips[0]!
 if(!('transform' in clip))throw new Error('Visual clip required')
 Object.assign(clip.transform,{position:{mode:'path',path:{nodes:[node('start',0),node('end',40)]},progress:{mode:'keyframes',track:{timeBase:1000,interpolation:'linear',sampleClock:{rateNum:2,rateDen:1,offsetNum:250000},keyframes:[{tick:250,value:0},{tick:1250,value:1}]}}}})
 nestedComposition.document={schemaVersion:1,root:'root',compositions:{root:{content,instances:[]}}};nestedComposition.activeCompositionId='root';nestedComposition.projectId='saved';nestedComposition.revision=3;nestedComposition.timeUs=1250000;nestedComposition.renderedOutputs={root:'old-render'}
})
afterEach(async()=>{if(component)await unmount(component);component=undefined;closeNestedComposition();nestedEffectPreview.value=null;authState.token=null;document.body.innerHTML=''})
function button(text:string){const item=[...document.querySelectorAll<HTMLButtonElement>('button')].find(element=>element.textContent===text);if(!item)throw new Error(`Missing button ${text}`);return item}
function input(label:string,value:string){const element=document.querySelector<HTMLInputElement>(`[aria-label="Нативная конвертация: ${label}"]`)!;element.value=value;element.dispatchEvent(new Event('input',{bubbles:true}));element.dispatchEvent(new Event('change',{bubbles:true}))}
function clip(){const value=nestedComposition.document!.compositions.root!.content.tracks[0]!.clips[0]!;if(!('transform' in value))throw new Error('Visual clip required');return value}
async function open(){component=mount(Inspector,{target:document.body});await tick();button(clip().transform.position?.mode==='path'?'Конвертировать движение в XY':'Конвертировать движение в Path').click();await tick()}
async function calculate(){button('Рассчитать конвертацию позиции').click();await vi.waitFor(()=>expect(previewPositionConversion).toHaveBeenCalled())}
it('previews the native clocked conversion without mutation and applies the reviewed result as one Undo/Redo',async()=>{
 const before=JSON.stringify(nestedComposition.document),shadow=JSON.stringify(clip().transform.x)
 await open();await calculate();await vi.waitFor(()=>expect(nestedEffectPreview.value).not.toBeNull())
 const preview=JSON.stringify(nestedEffectPreview.value!.project)
 expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedHistory.past).toHaveLength(0);expect(evaluateNestedFrame).not.toHaveBeenCalled();expect(nestedComposition.renderedOutputs).toEqual({root:'old-render'})
 expect(previewPositionConversion).toHaveBeenCalledWith(expect.objectContaining({tracks:[expect.objectContaining({sampleGrid:{timeBase:1000,rateNum:2,rateDen:1,offsetNum:250000}})],options:{fpsNum:30000,fpsDen:1000,startFrame:0,endFrame:60,tolerancePx:0.5,everyFrames:1,xyMode:'editable'}}),expect.any(AbortSignal))
 expect(document.body.textContent).toContain('Ошибка 0.200 px')
 button('Применить конвертацию позиции').click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 expect(JSON.stringify(nestedComposition.document)).toBe(preview);expect(clip().transform.position?.mode).toBe('xy');expect(JSON.stringify(clip().transform.x)).toBe(shadow)
 expect(previewPositionConversion).toHaveBeenCalledOnce();expect(evaluateNestedFrame).toHaveBeenCalledOnce();expect(nestedEffectPreview.value).toBeNull();expect(nestedComposition.renderedOutputs).toEqual({})
 undoNestedComposition();expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedComposition.renderedOutputs).toEqual({root:'old-render'})
 redoNestedComposition();expect(JSON.stringify(nestedComposition.document)).toBe(preview)
})
it('converts animated XY and makes stable path IDs before both preview and apply',async()=>{
 Object.assign(clip().transform,{position:{mode:'xy',x:{mode:'keyframes',track:{timeBase:24,interpolation:'linear',keyframes:[{tick:0,value:0},{tick:24,value:40}]}},y:{mode:'constant',value:15}}})
 vi.mocked(previewPositionConversion).mockResolvedValue({...answer,nodes:[{point:{x:0,y:15},incoming:{x:0,y:0},outgoing:{x:0,y:0},cubic:false},{point:{x:40,y:15},incoming:{x:0,y:0},outgoing:{x:0,y:0},cubic:false}],tracks:[scalar(0,1)],nodeCount:2})
 await open();await calculate();await vi.waitFor(()=>expect(nestedEffectPreview.value).not.toBeNull())
 expect(vi.mocked(previewPositionConversion).mock.calls[0]![0].tracks[0]!.sampleGrid?.timeBase).toBe(24)
 const preview=JSON.stringify(nestedEffectPreview.value!.project);button('Применить конвертацию позиции').click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1))
 expect(JSON.stringify(nestedComposition.document)).toBe(preview);expect(clip().transform.position).toMatchObject({mode:'path',path:{nodes:[{id:expect.stringMatching(/^conversion-.+-0$/)},{id:expect.stringMatching(/^conversion-.+-1$/)}]}})
})
it('sends the selected frame range and baking interval',async()=>{
 await open();input('начальный кадр','3');input('конечный кадр','42');input('допуск','0.25');input('интервал','4')
 const mode=document.querySelector<HTMLSelectElement>('[aria-label="Нативная конвертация: режим"]')!;mode.value='bake';expect(mode.value).toBe('bake');mode.dispatchEvent(new Event('change',{bubbles:true}));await tick();expect(mode.value).toBe('bake');await calculate()
 expect(vi.mocked(previewPositionConversion).mock.calls[0]![0].options).toMatchObject({startFrame:3,endFrame:42,tolerancePx:0.25,everyFrames:4,xyMode:'bake'})
})
it.each(['session','scope','clock','project','in_place','projectId','parameters','lock','busy','cancel','unmount'] as const)('aborts and discards a late conversion response on %s',async kind=>{
 let finish!:(result:NativePositionConversionResult)=>void
 vi.mocked(previewPositionConversion).mockImplementation(()=>new Promise(resolve=>{finish=resolve}))
 const before=JSON.stringify(nestedComposition.document);await open();await calculate();const signal=vi.mocked(previewPositionConversion).mock.calls[0]![1]!
 if(kind==='session')authState.token='other-session'
 if(kind==='scope')nestedComposition.activeCompositionId='other'
 if(kind==='clock')nestedComposition.timeUs++
 if(kind==='project')nestedComposition.document=JSON.parse(before)
 if(kind==='in_place')Object.assign(clip().transform,{x:{mode:'constant',value:0.75}})
 if(kind==='projectId')nestedComposition.projectId='other-project'
 if(kind==='parameters')input('допуск','0.25')
 if(kind==='lock')agentSession.lock={owner:'agent',reason:'editing',expiresAt:Date.now()+60000}
 if(kind==='busy')nestedHistory.busy=true
 if(kind==='cancel')button('Отменить конвертацию позиции').click()
 if(kind==='unmount'){await unmount(component!);component=undefined}
 const expected=JSON.stringify(nestedComposition.document)
 await tick();expect(signal.aborted).toBe(true);finish(answer);await tick();expect(nestedEffectPreview.value).toBeNull();expect(JSON.stringify(nestedComposition.document)).toBe(expected);expect(nestedHistory.past).toHaveLength(0);expect(evaluateNestedFrame).not.toHaveBeenCalled()
})
it('shows unmet tolerance without enabling apply or publishing a candidate',async()=>{
 vi.mocked(previewPositionConversion).mockResolvedValue({...answer,withinTolerance:false,maxErrorPx:0.75,limit:'frame_grid'})
 await open();await calculate();await vi.waitFor(()=>expect(document.body.textContent).toContain('Ошибка 0.750 px'))
 expect(button('Применить конвертацию позиции').disabled).toBe(true);expect(nestedEffectPreview.value).toBeNull();expect(nestedHistory.past).toHaveLength(0)
})
it('restores the original preview after complete-project validation refuses the conversion',async()=>{
 vi.mocked(evaluateNestedFrame).mockRejectedValue(new Error('Native full-project refusal'))
 const before=JSON.stringify(nestedComposition.document);await open();await calculate();await vi.waitFor(()=>expect(nestedEffectPreview.value).not.toBeNull())
 button('Применить конвертацию позиции').click();await vi.waitFor(()=>expect(document.body.textContent).toContain('Native full-project refusal'))
 expect(nestedEffectPreview.value).toBeNull();expect(nestedHistory.past).toHaveLength(0);expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedComposition.renderedOutputs).toEqual({root:'old-render'});expect(button('Применить конвертацию позиции').disabled).toBe(true)
})
it('cancels during complete-project validation and refuses the late accepted frame',async()=>{
 let finish!:(value:{entries:[]})=>void
 vi.mocked(evaluateNestedFrame).mockImplementation(()=>new Promise(resolve=>{finish=resolve}))
 const before=JSON.stringify(nestedComposition.document);await open();await calculate();await vi.waitFor(()=>expect(nestedEffectPreview.value).not.toBeNull())
 button('Применить конвертацию позиции').click();await vi.waitFor(()=>expect(finish).toBeDefined());button('Отменить конвертацию позиции').click();await tick()
 expect(vi.mocked(evaluateNestedFrame).mock.calls[0]![2]!.aborted).toBe(true);finish({entries:[]});await vi.waitFor(()=>expect(nestedHistory.busy).toBe(false))
 expect(nestedEffectPreview.value).toBeNull();expect(nestedHistory.past).toHaveLength(0);expect(JSON.stringify(nestedComposition.document)).toBe(before)
})
it('refuses invalid ranges before issuing a native request',async()=>{
 await open();input('конечный кадр','100');await tick();button('Рассчитать конвертацию позиции').click();await vi.waitFor(()=>expect(document.body.textContent).toContain('Проверьте диапазон'))
 expect(previewPositionConversion).not.toHaveBeenCalled();expect(nestedHistory.past).toHaveLength(0)
})

it('discards a prepared conversion when another preview takes ownership without clearing that preview',async()=>{
 await open();await calculate();await vi.waitFor(()=>expect(nestedEffectPreview.value).not.toBeNull())
 const foreign={...nestedEffectPreview.value!,owner:Symbol('other preview'),gesture:{}}
 nestedEffectPreview.value=foreign;await tick()
 expect(nestedEffectPreview.value).toBe(foreign);expect(button('Применить конвертацию позиции').disabled).toBe(true)
 expect(vi.mocked(previewPositionConversion).mock.calls[0]![1]!.aborted).toBe(true)
 button('Рассчитать конвертацию позиции').click();await tick();expect(previewPositionConversion).toHaveBeenCalledOnce();expect(nestedEffectPreview.value).toBe(foreign)
})

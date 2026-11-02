import { writeFileSync } from 'node:fs'
// @ts-expect-error Mounted tests require the Svelte client entry.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { beforeEach, afterEach, expect, it, vi } from 'vitest'
import { evaluateMotionPath, evaluateNestedFrame, type NativeMotionPathResponse } from '$lib/api'
import { nestedComposition, nestedHistory, closeNestedComposition, undoNestedComposition } from '$lib/state/nestedComposition.svelte'
import { nestedPathOverlay } from '$lib/state/nestedPathOverlay.svelte'
import { nestedEffectPreview } from '$lib/state/nestedEffectPreview.svelte'
import { agentSession } from '$lib/agent/session.svelte'
import { authState } from '$lib/state/auth.svelte'
import { createComposition, addTrack, addClip } from '$lib/composition/commands'
import { buildCompositionWireDocument } from '$lib/composition/payload'
import Inspector from './NestedMotionPathInspector.svelte'
import Overlay from './NestedMotionPathOverlay.svelte'
vi.mock('$lib/api', async original => ({ ...await original<typeof import('$lib/api')>(), evaluateMotionPath: vi.fn(), evaluateNestedFrame: vi.fn() }))
let inspector: ReturnType<typeof mount> | undefined, overlay: ReturnType<typeof mount> | undefined
const node=(id:string,x:number,y=-5)=>({id,point:{x,y},inHandle:{x:-2,y:0},outHandle:{x:2,y:0},segment:'line' as const,tangentMode:'corner' as const})
const path={nodes:[node('start',-10),node('end',10)]}
beforeEach(()=>{
 closeNestedComposition();nestedPathOverlay.value=null;nestedEffectPreview.value=null;authState.token='path-session'
 vi.mocked(evaluateMotionPath).mockReset();vi.mocked(evaluateNestedFrame).mockReset();vi.mocked(evaluateNestedFrame).mockResolvedValue({entries:[]})
 let doc=createComposition({width:64,height:48,fps:30,backgroundColor:'#000000FF'})
 doc=addTrack(doc,{id:'text',kind:'text',name:'Text',locked:false,hidden:false,clips:[]})
 doc=addClip(doc,'text',{id:'title',kind:'text',text:'Path',timelineStartTicks:1000000,durationTicks:2000000,x:17,y:23,opacity:1,style:{fontSizePx:24,color:'#FFFFFFFF',align:'center'}})
 const content=buildCompositionWireDocument(doc),clip=content.tracks[0]!.clips[0]!;if(!('transform' in clip))throw new Error('Visual clip required')
 Object.assign(clip.transform,{position:{mode:'path',path:structuredClone(path),progress:{mode:'keyframes',track:{timeBase:1000,interpolation:'linear',sampleClock:{rateNum:2,rateDen:1,offsetNum:250000},keyframes:[{tick:250,value:0},{tick:1250,value:1}]}}}})
 nestedComposition.document={schemaVersion:1,root:'root',compositions:{root:{content,instances:[]}}};nestedComposition.projectId='project';nestedComposition.revision=3;nestedComposition.activeCompositionId='root';nestedComposition.timeUs=1250000;nestedComposition.renderedOutputs={root:'old-render'}
})
afterEach(async()=>{if(overlay)await unmount(overlay);if(inspector)await unmount(inspector);overlay=undefined;inspector=undefined;closeNestedComposition();authState.token=null;document.body.innerHTML=''})
async function open(){
 inspector=mount(Inspector,{target:document.body});overlay=mount(Overlay,{target:document.body});await tick()
 expect(document.querySelector('[aria-label="Траектория на кадре"]')).toBeNull()
 const toggle=document.querySelector<HTMLInputElement>('[aria-label="Редактировать путь на кадре"]')!;toggle.checked=true;toggle.dispatchEvent(new Event('change',{bubbles:true}));await tick()
 const svg=document.querySelector<SVGSVGElement>('[aria-label="Траектория на кадре"]')!;expect(svg).not.toBeNull();svg.getBoundingClientRect=()=>new DOMRect(0,0,128,96);svg.setPointerCapture=()=>{};svg.hasPointerCapture=()=>false
 return svg
}
function pointer(target:Element,type:string,x:number,y:number,id=1){const event=new MouseEvent(type,{bubbles:true,button:0,clientX:x,clientY:y});Object.defineProperty(event,'pointerId',{value:id});target.dispatchEvent(event)}
function position(){const clip=nestedComposition.document!.compositions.root!.content.tracks[0]!.clips[0]!;if(!('transform' in clip)||clip.transform.position?.mode!=='path')throw new Error('Path required');return clip.transform.position}
it.each(['point','in_handle','out_handle'] as const)('drags a spatial %s through native preview and one Undo without modifying the progress clock',async part=>{
 const edited={nodes:[{...node('start',-10),...(part==='point'?{point:{x:-2,y:-1}}:part==='in_handle'?{inHandle:{x:6,y:4}}:{outHandle:{x:10,y:4}})},node('end',10)]}
 vi.mocked(evaluateMotionPath).mockResolvedValue({path:edited,length:20,points:[]})
 const before=JSON.stringify(nestedComposition.document),svg=await open()
 expect(svg.getAttribute('viewBox')).toBe('0 0 64 48');expect(svg.querySelector('g')!.getAttribute('transform')).toBe('translate(32 24)')
 const label=part==='point'?'Узел пути на кадре 1':`Ручка пути 1 ${part}`
 const handle=svg.querySelector<SVGCircleElement>(`[aria-label="${label}"]`)!
 const x=(32+Number(handle.getAttribute('cx')))*2,y=(24+Number(handle.getAttribute('cy')))*2
 pointer(handle,'pointerdown',x,y);pointer(svg,'pointermove',x+16,y+8)
 await vi.waitFor(()=>expect(nestedEffectPreview.value).not.toBeNull())
 expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedHistory.past).toHaveLength(0)
 expect(evaluateMotionPath).toHaveBeenCalledWith({operation:'edit',index:0,part,value:part==='point'?{x:-2,y:-1}:part==='in_handle'?{x:6,y:4}:{x:10,y:4},path},expect.any(AbortSignal))
 pointer(svg,'pointerup',x+16,y+8);await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 expect(position().path).toEqual(edited);expect(position().progress).toMatchObject({track:{sampleClock:{rateNum:2,rateDen:1,offsetNum:250000}}})
 expect(nestedComposition.renderedOutputs).toEqual({});expect(nestedEffectPreview.value).toBeNull()
 const oracle=process.env.NATIVE_PATH_SPATIAL_ORACLE
 if(oracle&&part==='point')writeFileSync(oracle,JSON.stringify({schemaVersion:1,request:vi.mocked(evaluateMotionPath).mock.calls.at(-1)![0],composition:nestedComposition.document!.compositions.root!.content}))
 undoNestedComposition();expect(JSON.stringify(nestedComposition.document)).toBe(before)
})
it.each(['escape','pointercancel','capture','clock','project','session','lock','disable','unmount'] as const)('cancels a pending spatial gesture on %s and discards the late Rust response',async kind=>{
 let finish!:(value:NativeMotionPathResponse)=>void
 vi.mocked(evaluateMotionPath).mockImplementation(()=>new Promise(resolve=>{finish=resolve}))
 const before=JSON.stringify(nestedComposition.document),svg=await open(),handle=svg.querySelector('[aria-label="Узел пути на кадре 1"]')!
 pointer(handle,'pointerdown',44,38);pointer(svg,'pointermove',60,46);await vi.waitFor(()=>expect(finish).toBeDefined())
 const signal=vi.mocked(evaluateMotionPath).mock.calls[0]![1]!
 if(kind==='escape')window.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape'}))
 if(kind==='pointercancel')pointer(svg,'pointercancel',60,46)
 if(kind==='capture')svg.dispatchEvent(new Event('lostpointercapture'))
 if(kind==='clock')nestedComposition.timeUs=1300000
 if(kind==='project')nestedComposition.document=JSON.parse(before)
 if(kind==='session')authState.token='other-session'
 if(kind==='lock')agentSession.lock={owner:'agent',reason:'editing',expiresAt:Date.now()+60000}
 if(kind==='disable'){const toggle=document.querySelector<HTMLInputElement>('[aria-label="Редактировать путь на кадре"]')!;toggle.checked=false;toggle.dispatchEvent(new Event('change',{bubbles:true}))}
 if(kind==='unmount'){await unmount(overlay!);overlay=undefined}
 await tick();expect(signal.aborted).toBe(true)
 finish({path:{nodes:[node('start',-2,-1),node('end',10)]},length:20,points:[]});await tick()
 pointer(svg,'pointerup',60,46);await tick()
 expect(nestedEffectPreview.value).toBeNull();expect(nestedHistory.past).toHaveLength(0);expect(evaluateNestedFrame).not.toHaveBeenCalled();expect(JSON.stringify(nestedComposition.document)).toBe(before)
})
it('uses native nearest-point insertion and refuses misses without an empty history entry',async()=>{
 vi.mocked(evaluateMotionPath).mockImplementation(async request=>{
  if(request.operation==='nearest')return{path,length:20,points:[],nearest:{segment:0,t:0.5,distance:2}}
  if(request.operation==='insert')return{path:{nodes:[path.nodes[0]!,{...node(request.id,0),inHandle:{x:0,y:0},outHandle:{x:0,y:0}},path.nodes[1]!]},length:20,points:[]}
  throw new Error('Unexpected geometry operation')
 })
 const svg=await open();svg.dispatchEvent(new MouseEvent('dblclick',{bubbles:true,clientX:64,clientY:38}))
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1))
 expect(evaluateMotionPath).toHaveBeenNthCalledWith(1,{operation:'nearest',path,point:{x:0,y:-5}})
 expect(evaluateMotionPath).toHaveBeenNthCalledWith(2,{operation:'insert',path,segment:0,t:0.5,id:expect.any(String)})
 expect(position().path.nodes).toHaveLength(3);undoNestedComposition();await tick()
 vi.mocked(evaluateMotionPath).mockResolvedValue({path,length:20,points:[],nearest:{segment:0,t:0.5,distance:5}})
 document.querySelector<SVGSVGElement>('[aria-label="Траектория на кадре"]')!.getBoundingClientRect=()=>new DOMRect(0,0,128,96)
 document.querySelector('[aria-label="Траектория на кадре"]')!.dispatchEvent(new MouseEvent('dblclick',{bubbles:true,clientX:64,clientY:38}))
 await vi.waitFor(()=>expect(nestedHistory.error).toContain('No nearby'));expect(nestedHistory.past).toHaveLength(0)
})

it('coalesces pointer geometry requests and ignores a late solved path after cancellation',async()=>{
 const pending:Array<(value:NativeMotionPathResponse)=>void>=[]
 vi.mocked(evaluateMotionPath).mockImplementation(()=>new Promise(resolve=>pending.push(resolve)))
 const svg=await open(),handle=svg.querySelector('[aria-label="Узел пути на кадре 1"]')!,before=JSON.stringify(nestedComposition.document)
 pointer(handle,'pointerdown',44,38);pointer(svg,'pointermove',60,46);await vi.waitFor(()=>expect(pending).toHaveLength(1))
 pointer(svg,'pointermove',64,50);pointer(svg,'pointermove',68,54);await tick();expect(pending).toHaveLength(1)
 pending[0]!({path:{nodes:[node('start',-2,-1),node('end',10)]},length:20,points:[]})
 await vi.waitFor(()=>expect(pending).toHaveLength(2));expect(nestedEffectPreview.value).not.toBeNull()
 expect(vi.mocked(evaluateMotionPath).mock.calls[1]![0]).toMatchObject({operation:'edit',value:{x:2,y:3}})
 window.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape'}));await tick()
 pending[1]!({path:{nodes:[node('start',2,3),node('end',10)]},length:20,points:[]});await tick()
 expect(nestedEffectPreview.value).toBeNull();expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedHistory.past).toHaveLength(0)
})
it('supports keyboard movement, ignores clicks and keeps a single pointer owner',async()=>{
 const edited={nodes:[node('start',-20),node('end',10)]}
 vi.mocked(evaluateMotionPath).mockResolvedValue({path:edited,length:30,points:[]})
 const svg=await open(),handle=svg.querySelector('[aria-label="Узел пути на кадре 1"]')!
 pointer(handle,'pointerdown',44,38);pointer(svg,'pointerup',44,38);await tick()
 expect(evaluateMotionPath).not.toHaveBeenCalled();expect(nestedHistory.past).toHaveLength(0)
 handle.dispatchEvent(new KeyboardEvent('keydown',{bubbles:true,key:'ArrowLeft',shiftKey:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 expect(evaluateMotionPath).toHaveBeenCalledWith({operation:'edit',index:0,part:'point',value:{x:-20,y:-5},path})
 undoNestedComposition();await tick();vi.mocked(evaluateMotionPath).mockClear()
 const currentSvg=document.querySelector<SVGSVGElement>('[aria-label="Траектория на кадре"]')!;currentSvg.getBoundingClientRect=()=>new DOMRect(0,0,128,96);currentSvg.setPointerCapture=()=>{};currentSvg.hasPointerCapture=()=>false
 const currentHandle=currentSvg.querySelector('[aria-label="Узел пути на кадре 1"]')!
 pointer(currentHandle,'pointerdown',44,38,1);pointer(currentHandle,'pointerdown',44,38,2);pointer(currentSvg,'pointermove',60,46,2);await tick()
 expect(evaluateMotionPath).not.toHaveBeenCalled();pointer(currentSvg,'pointercancel',44,38,1)
})
it('restores the authored path and draft ownership if complete-project validation refuses a released drag',async()=>{
 vi.mocked(evaluateMotionPath).mockResolvedValue({path:{nodes:[node('start',-2,-1),node('end',10)]},length:20,points:[]})
 vi.mocked(evaluateNestedFrame).mockRejectedValueOnce(new Error('scene refused'))
 const before=JSON.stringify(nestedComposition.document),svg=await open(),handle=svg.querySelector('[aria-label="Узел пути на кадре 1"]')!
 pointer(handle,'pointerdown',44,38);pointer(svg,'pointermove',60,46);await vi.waitFor(()=>expect(nestedEffectPreview.value).not.toBeNull())
 pointer(svg,'pointerup',60,46);await vi.waitFor(()=>expect(nestedHistory.error).toBe('scene refused'));await tick()
 expect(nestedEffectPreview.value).toBeNull();expect(nestedHistory.past).toHaveLength(0);expect(JSON.stringify(nestedComposition.document)).toBe(before)
 expect(Number(document.querySelector('[aria-label="Узел пути на кадре 1"]')!.getAttribute('cx'))).toBe(-10)
})

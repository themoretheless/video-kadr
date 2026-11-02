// @ts-expect-error Tests require the Svelte client entry under Vitest SSR resolution.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { evaluateNestedFrame, sampleConcatCurve, type NativeNestedProject } from '$lib/api'
import { closeNestedComposition, nestedComposition, nestedHistory, undoNestedComposition, redoNestedComposition } from '$lib/state/nestedComposition.svelte'
import NestedConcatCurvesInspector from './NestedConcatCurvesInspector.svelte'
vi.mock('$lib/api', () => ({ evaluateNestedFrame: vi.fn(), sampleConcatCurve: vi.fn(),sampleToneCurve:vi.fn() }))
let component: ReturnType<typeof mount> | undefined
beforeEach(() => {
 closeNestedComposition()
 vi.mocked(evaluateNestedFrame).mockResolvedValue({entries:[]})
 vi.mocked(sampleConcatCurve).mockResolvedValue(Array.from({length:256},(_,i)=>i/255))
 nestedComposition.document={schemaVersion:1,root:'root',compositions:{root:{content:{tracks:[{kind:'video',id:'track',name:'Video',locked:false,clips:[{id:'clip',effects:[]}]}]},instances:[]}}} as unknown as NativeNestedProject
 nestedComposition.renderedOutputs={root:'movie'}
})
afterEach(async()=>{ if(component) await unmount(component); component=undefined;closeNestedComposition();vi.clearAllMocks();document.body.innerHTML='' })
function click(name:string) {const button=[...document.querySelectorAll('button')].find(b=>b.textContent?.trim()===name);expect(button).toBeDefined();button!.click()}
it('adds identity control points, edits a channel and keeps the other channels with Undo/Redo',async()=>{
 component=mount(NestedConcatCurvesInspector,{target:document.body});await tick();click('Добавить Concat Curves')
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick();click('Добавить точку')
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));await tick()
 const input=document.querySelector<HTMLInputElement>('input[aria-label="Яркость (luma) точка 3 выход"]')!
 input.value='0.7';input.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(3));await tick()
 const select=document.querySelector<HTMLSelectElement>('select')!;select.value='blue';select.dispatchEvent(new Event('change',{bubbles:true}));await tick();expect(document.querySelector('svg')?.getAttribute('aria-label')).toBe('Кривая Синий');click('Добавить точку')
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(4))
 const track=nestedComposition.document!.compositions.root!.content.tracks[0]!;if(track.kind!=='video')throw new Error('video expected')
 const effect=track.clips[0]!.effects[0]!;if(effect.kind!=='concat_curves')throw new Error('curves expected')
 expect(effect.parameters.luma).toEqual([[0,0],[1,1],[0.5,0.7]])
 expect(effect.parameters.blue).toEqual([[0,0],[1,1],[0.5,0.5]])
 expect(sampleConcatCurve).toHaveBeenCalled()
 undoNestedComposition();undoNestedComposition();undoNestedComposition();undoNestedComposition()
 expect(nestedComposition.renderedOutputs).toEqual({root:'movie'})
 redoNestedComposition();expect(nestedComposition.renderedOutputs).toEqual({})
})
it('aborts graph requests when disposed and ignores late samples',async()=>{
 let resolve!: (values: readonly number[]) => void
 vi.mocked(sampleConcatCurve).mockReturnValue(new Promise((done) => { resolve = done }))
 component=mount(NestedConcatCurvesInspector,{target:document.body});await tick();click('Добавить Concat Curves')
 await vi.waitFor(()=>expect(sampleConcatCurve).toHaveBeenCalled())
 const signal=vi.mocked(sampleConcatCurve).mock.calls[0]![1]!
 const path=document.querySelector<SVGPathElement>('.curve')!
 await unmount(component);component=undefined
 expect(signal.aborted).toBe(true)
 resolve(Array.from({length:256},()=>0.5));await tick()
 expect(path.getAttribute('d')).toBe('')
})

it('offers only source channels and enforces the eight-point budget with Undo',async()=>{
 component=mount(NestedConcatCurvesInspector,{target:document.body});await tick();click('Добавить Concat Curves');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick();expect([...document.querySelectorAll('select option')].map(o=>(o as HTMLOptionElement).value)).toEqual(['luma','red','green','blue']);click('Добавить точку');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));await tick();for(let i=0;i<5;i++){click('Добавить точку');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(3+i));await tick()}const add=[...document.querySelectorAll('button')].find(b=>b.textContent?.trim()==='Добавить точку')!;expect(add.disabled).toBe(true);add.click();expect(nestedHistory.past).toHaveLength(7);click('Удалить точку 8');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(8));undoNestedComposition();await tick();expect(add.disabled).toBe(true)
})
it('refuses a late curve edit after an account-context switch',async()=>{
 const{default:Harness}=await import('./fixtures/ConcatCurvesHarness.svelte');const harness=mount(Harness,{target:document.body});component=harness;await tick();click('Добавить Concat Curves');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick();click('Добавить точку');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));await tick();const before=JSON.stringify(nestedComposition.document);let resolve!:(v:{entries:[]})=>void;vi.mocked(evaluateNestedFrame).mockReturnValueOnce(new Promise(done=>resolve=done));const input=document.querySelector<HTMLInputElement>('input[aria-label="Яркость (luma) точка 3 выход"]')!;input.value='0.7';input.dispatchEvent(new Event('change',{bubbles:true}));await tick();harness.changeSession();await tick();resolve({entries:[]});await vi.waitFor(()=>expect(nestedHistory.busy).toBe(false));expect(nestedHistory.past).toHaveLength(2);expect(JSON.stringify(nestedComposition.document)).toBe(before)
})
it('previews a point drag without history writes, holds its candidate through validation and commits once',async()=>{
 const{nestedEffectPreview}=await import('$lib/state/nestedEffectPreview.svelte');component=mount(NestedConcatCurvesInspector,{target:document.body});await tick();click('Добавить Concat Curves');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick();click('Добавить точку');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));await tick();nestedComposition.renderedOutputs={root:'old-movie'};const before=JSON.stringify(nestedComposition.document);const svg=document.querySelector('svg')!;Object.defineProperty(svg,'getScreenCTM',{value:()=>null,configurable:true});vi.spyOn(svg,'getBoundingClientRect').mockReturnValue({left:0,top:0,width:200,height:200,right:200,bottom:200,x:0,y:0,toJSON:()=>({})});const point=svg.querySelectorAll<SVGCircleElement>('circle')[2]!;Object.defineProperty(point,'setPointerCapture',{value:vi.fn(),configurable:true});const pointer=(type:string,x:number,y:number)=>{const e=new MouseEvent(type,{bubbles:true,button:0,clientX:x,clientY:y});Object.defineProperty(e,'pointerId',{value:7});point.dispatchEvent(e)}
 pointer('pointerdown',100,100);pointer('pointermove',140,40);await tick();expect(nestedHistory.past).toHaveLength(2);expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedComposition.renderedOutputs).toEqual({root:'old-movie'});expect(nestedEffectPreview.value).not.toBeNull();point.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true}));await tick();expect(nestedEffectPreview.value).toBeNull();expect(JSON.stringify(nestedComposition.document)).toBe(before)
 let resolve!:(value:{entries:[]})=>void;vi.mocked(evaluateNestedFrame).mockReturnValueOnce(new Promise(done=>resolve=done));pointer('pointerdown',100,100);pointer('pointermove',140,40);pointer('pointerup',140,40);await tick();expect(nestedHistory.busy).toBe(true);expect(nestedEffectPreview.value).not.toBeNull();expect(JSON.stringify(nestedComposition.document)).toBe(before);resolve({entries:[]});await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(3));await tick();expect(nestedEffectPreview.value).toBeNull();const track=nestedComposition.document!.compositions.root!.content.tracks[0]!;if(track.kind!=='video')throw new Error('video');const e=track.clips[0]!.effects![0]!;if(e.kind!=='concat_curves')throw new Error('curves');expect(e.parameters.luma?.[2]?.[0]).toBeCloseTo(0.7);expect(e.parameters.luma?.[2]?.[1]).toBeCloseTo(0.8);undoNestedComposition();expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedComposition.renderedOutputs).toEqual({root:'old-movie'})
})

it('replaces an imported singleton with source identity anchors before adding, with one Undo and eight slots',async()=>{
 const track=nestedComposition.document!.compositions.root!.content.tracks[0]!;if(track.kind!=='video')throw new Error('video');const base=nestedComposition.document!;const root=base.compositions.root!;nestedComposition.document={...base,compositions:{...base.compositions,root:{...root,content:{...root.content,tracks:[{...track,clips:[{...track.clips[0]!,effects:[{kind:'concat_curves',parameters:{luma:[[0.2,0.9]],red:[[0,0.1],[1,0.8]]}}]}]}]}}}};
 const before=JSON.stringify(nestedComposition.document);component=mount(NestedConcatCurvesInspector,{target:document.body});await tick();click('Добавить точку');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick();
 let effect= nestedComposition.document!.compositions.root!.content.tracks[0]!;if(effect.kind!=='video')throw new Error('video');const grade=effect.clips[0]!.effects[0]!;if(grade.kind!=='concat_curves')throw new Error('curves');expect(grade.parameters.luma).toEqual([[0,0],[1,1],[0.5,0.5]]);expect(grade.parameters.red).toEqual([[0,0.1],[1,0.8]]);
 for(let n=4;n<=8;n++){click('Добавить точку');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(n-2));await tick()}
 const add=[...document.querySelectorAll('button')].find(b=>b.textContent?.trim()==='Добавить точку')!;expect(add.disabled).toBe(true);add.click();await tick();expect(nestedHistory.past).toHaveLength(6);
 for(let n=0;n<6;n++)undoNestedComposition();expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedComposition.renderedOutputs).toEqual({root:'movie'})
})

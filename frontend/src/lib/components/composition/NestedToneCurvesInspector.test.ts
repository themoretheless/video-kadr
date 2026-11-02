// @ts-expect-error Tests require the Svelte client entry under Vitest SSR resolution.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { evaluateNestedFrame, sampleToneCurve, type NativeNestedProject } from '$lib/api'
import { closeNestedComposition, nestedComposition, nestedHistory, undoNestedComposition, redoNestedComposition } from '$lib/state/nestedComposition.svelte'
import NestedToneCurvesInspector from './NestedToneCurvesInspector.svelte'
vi.mock('$lib/api', () => ({ evaluateNestedFrame: vi.fn(), sampleToneCurve: vi.fn() }))
let component: ReturnType<typeof mount> | undefined
beforeEach(() => {
 closeNestedComposition()
 vi.mocked(evaluateNestedFrame).mockResolvedValue({entries:[]})
 vi.mocked(sampleToneCurve).mockResolvedValue(Array.from({length:256},(_,i)=>i/255))
 nestedComposition.document={schemaVersion:1,root:'root',compositions:{root:{content:{tracks:[{kind:'video',id:'track',name:'Video',locked:false,clips:[{id:'clip',effects:[]}]}]},instances:[]}}} as unknown as NativeNestedProject
 nestedComposition.renderedOutputs={root:'movie'}
})
afterEach(async()=>{ if(component) await unmount(component); component=undefined;closeNestedComposition();vi.clearAllMocks();document.body.innerHTML='' })
function click(name:string) {const button=[...document.querySelectorAll('button')].find(b=>b.textContent?.trim()===name);expect(button).toBeDefined();button!.click()}
it('adds identity control points, edits a channel and keeps the other channels with Undo/Redo',async()=>{
 component=mount(NestedToneCurvesInspector,{target:document.body});await tick();click('Добавить RGB-кривые')
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick();click('Добавить точку')
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));await tick()
 const input=document.querySelector<HTMLInputElement>('input[aria-label="Общая точка 2 выход"]')!
 input.value='0.7';input.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(3));await tick()
 const select=document.querySelector<HTMLSelectElement>('select')!;select.value='blue';select.dispatchEvent(new Event('change',{bubbles:true}));await tick();expect(document.querySelector('svg')?.getAttribute('aria-label')).toBe('Кривая Синий');click('Добавить точку')
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(4))
 const track=nestedComposition.document!.compositions.root!.content.tracks[0]!;if(track.kind!=='video')throw new Error('video expected')
 const effect=track.clips[0]!.effects[0]!;if(effect.kind!=='tone_curves')throw new Error('curves expected')
 expect(effect.curves.master).toEqual([[0,0],[0.5,0.7],[1,1]])
 expect(effect.curves.blue).toEqual([[0,0],[0.5,0.5],[1,1]])
 expect(sampleToneCurve).toHaveBeenCalled()
 undoNestedComposition();undoNestedComposition();undoNestedComposition();undoNestedComposition()
 expect(nestedComposition.renderedOutputs).toEqual({root:'movie'})
 redoNestedComposition();expect(nestedComposition.renderedOutputs).toEqual({})
})
it('aborts graph requests when disposed and ignores late samples',async()=>{
 let resolve!: (values: readonly number[]) => void
 vi.mocked(sampleToneCurve).mockReturnValue(new Promise((done) => { resolve = done }))
 component=mount(NestedToneCurvesInspector,{target:document.body});await tick();click('Добавить RGB-кривые')
 await vi.waitFor(()=>expect(sampleToneCurve).toHaveBeenCalled())
 const signal=vi.mocked(sampleToneCurve).mock.calls[0]![1]!
 const path=document.querySelector<SVGPathElement>('.curve')!
 await unmount(component);component=undefined
 expect(signal.aborted).toBe(true)
 resolve(Array.from({length:256},()=>0.5));await tick()
 expect(path.getAttribute('d')).toBe('')
})

it('authors secondary curves with neutral points and asks Rust for periodic graph samples',async()=>{
 component=mount(NestedToneCurvesInspector,{target:document.body});await tick();click('Добавить RGB-кривые')
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 const select=document.querySelector<HTMLSelectElement>('select')!
 for (const channel of ['hueVsSat','hueVsHue','hueVsLuma','lumaVsSat','satVsSat']) {
  select.value=channel;select.dispatchEvent(new Event('change',{bubbles:true}));await tick();click('Добавить точку')
  await vi.waitFor(()=>{
   const track=nestedComposition.document!.compositions.root!.content.tracks[0]!
   if(track.kind!=='video')throw new Error('video expected')
   const effect=track.clips[0]!.effects[0]!
   if(effect.kind!=='tone_curves')throw new Error('curve expected')
   expect(effect.curves[channel as keyof typeof effect.curves]).toEqual([[0,0.5],[0.5,0.5]])
  });await tick()
 }
 expect(vi.mocked(sampleToneCurve).mock.calls.some(call=>call[2]===true)).toBe(true)
 expect(document.querySelector('.identity')?.getAttribute('d')).toBe('M0 100L200 100')
})

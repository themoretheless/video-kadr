// @ts-expect-error Tests require the Svelte client entry under Vitest SSR resolution.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { evaluateNestedFrame, type NativeNestedProject } from '$lib/api'
import { closeNestedComposition, nestedComposition, nestedHistory, undoNestedComposition, redoNestedComposition } from '$lib/state/nestedComposition.svelte'
import Inspector from './NestedCreativeGradeInspector.svelte'
vi.mock('$lib/api',()=>({evaluateNestedFrame:vi.fn()}))
let component:ReturnType<typeof mount>|undefined
beforeEach(()=>{
 closeNestedComposition();vi.mocked(evaluateNestedFrame).mockResolvedValue({entries:[]})
 nestedComposition.document={schemaVersion:1,root:'root',compositions:{root:{content:{tracks:[{kind:'video',id:'video',name:'Video',locked:false,clips:[{id:'clip',placement:{timelineStartTick:100000},effects:[]}]}]},instances:[]}}} as unknown as NativeNestedProject
 nestedComposition.renderedOutputs={root:'movie'}
})
afterEach(async()=>{if(component)await unmount(component);component=undefined;closeNestedComposition();vi.clearAllMocks();document.body.innerHTML=''})
function click(name:string){const button=[...document.querySelectorAll('button')].find(b=>b.textContent?.trim()===name);expect(button).toBeDefined();button!.click()}
async function input(label:string,value:string,count:number){const el=document.querySelector<HTMLInputElement>(`input[aria-label="${label}"]`)!;el.value=value;el.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(count));await tick()}
function parameters(){const track=nestedComposition.document!.compositions.root!.content.tracks[0]!;if(track.kind!=='video')throw new Error('video expected');const effect=track.clips[0]!.effects[0]!;if(effect.kind!=='creative_grade')throw new Error('grade expected');return effect.parameters}
it('authors looks and three wheels through validated history, resets and restores output bindings',async()=>{
 component=mount(Inspector,{target:document.body});await tick();click('Добавить колёса и образ');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 const select=document.querySelector<HTMLSelectElement>('select')!;expect(select.options).toHaveLength(9)
 select.value='6';select.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));await tick()
 await input('clip интенсивность образа','150',3)
 await input('clip Тени x','0.25',4)
 await input('clip Средние тона y','-0.5',5)
 await input('clip Света lightness','-20',6)
 expect(parameters()).toEqual({look:6,intensity:150,shadows:{x:0.25},midtones:{y:-0.5},highlights:{lightness:-20}})
 expect(nestedComposition.renderedOutputs).toEqual({})
 click('Сбросить колёса и образ');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(7));expect(parameters()).toEqual({})
 undoNestedComposition();expect(parameters().look).toBe(6)
 for(let i=0;i<6;i++)undoNestedComposition();expect(nestedComposition.renderedOutputs).toEqual({root:'movie'})
 redoNestedComposition();expect(parameters()).toEqual({});expect(nestedComposition.renderedOutputs).toEqual({})
})
it('respects locks and keeps rejected edits out of history',async()=>{
 component=mount(Inspector,{target:document.body});await tick();click('Добавить колёса и образ');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 vi.mocked(evaluateNestedFrame).mockRejectedValueOnce(new Error('invalid wheel'))
 const el=document.querySelector<HTMLInputElement>('input[aria-label="clip Тени x"]')!;el.value='2';el.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.error).toBe('invalid wheel'));expect(parameters()).toEqual({});expect(nestedHistory.past).toHaveLength(1)
 const doc=nestedComposition.document!;const content=doc.compositions.root!.content
 nestedComposition.document={...doc,compositions:{...doc.compositions,root:{...doc.compositions.root!,content:{...content,tracks:content.tracks.map(t=>({...t,locked:true}))}}}}
 await tick();expect(document.querySelector('fieldset')?.disabled).toBe(true)
})

it('adds and replaces clip-local keys, preserves interpolation and removes animation with history',async()=>{
 component=mount(Inspector,{target:document.body});await tick();click('Добавить колёса и образ');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 const key=()=>document.querySelector<HTMLButtonElement>('button[aria-label="clip shadowsX ключ"]')!
 expect(key().disabled).toBe(true)
 nestedComposition.timeUs=600000;await tick();await input('clip Тени x','0.25',2);key().click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(3));await tick()
 function animation(){const track=nestedComposition.document!.compositions.root!.content.tracks[0]!;if(track.kind!=='video')throw new Error('video');const effect=track.clips[0]!.effects[0]!;if(effect.kind!=='creative_grade')throw new Error('grade');return effect.animation}
 expect(animation()?.shadowsX).toEqual({mode:'keyframes',track:{timeBase:1000000,interpolation:'linear',keyframes:[{tick:500000,value:0.25}]}})
 await input('clip Тени x','0.75',4);key().click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(5));await tick()
 const first=animation()?.shadowsX;if(first?.mode!=='keyframes')throw new Error('keyframes')
 expect(first.track.keyframes).toEqual([{tick:500000,value:0.75}])
 nestedComposition.timeUs=1100000;await tick();key().click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(6));await tick()
 const second=animation()?.shadowsX;if(second?.mode!=='keyframes')throw new Error('keyframes');expect(second.track.keyframes).toHaveLength(2)
 document.querySelector<HTMLButtonElement>('button[aria-label="clip shadowsX удалить анимацию"]')!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(7));expect(animation()?.shadowsX).toBeUndefined()
 undoNestedComposition();expect(animation()?.shadowsX?.mode).toBe('keyframes');await tick();click('Сбросить колёса и образ');await vi.waitFor(()=>expect(animation()).toBeUndefined())
})

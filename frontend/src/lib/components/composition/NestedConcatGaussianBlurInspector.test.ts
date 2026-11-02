import {writeFileSync} from 'node:fs'
// @ts-expect-error Tests require the Svelte client entry under Vitest SSR resolution.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { evaluateNestedFrame, type NativeNestedProject } from '$lib/api'
import { closeNestedComposition, nestedComposition, nestedHistory, undoNestedComposition, redoNestedComposition } from '$lib/state/nestedComposition.svelte'
import Inspector from './NestedConcatGaussianBlurInspector.svelte'
vi.mock('$lib/api',()=>({evaluateNestedFrame:vi.fn()}))
let component:ReturnType<typeof mount>|undefined
beforeEach(()=>{
 closeNestedComposition();vi.mocked(evaluateNestedFrame).mockResolvedValue({entries:[]})
 nestedComposition.document={schemaVersion:1,root:'root',compositions:{root:{content:{tracks:[{kind:'video',id:'video',name:'Video',locked:false,clips:[{id:'clip',placement:{timelineStartTick:100000},effects:[]}]}]},instances:[]}}} as unknown as NativeNestedProject
 nestedComposition.renderedOutputs={root:'movie'}
})
afterEach(async()=>{if(component)await unmount(component);component=undefined;closeNestedComposition();vi.clearAllMocks();document.body.innerHTML=''})
function click(name:string){const b=[...document.querySelectorAll('button')].find(b=>b.textContent?.trim()===name);expect(b).toBeDefined();b!.click()}
function parameters(){const t=nestedComposition.document!.compositions.root!.content.tracks[0]!;if(t.kind!=='video')throw new Error('video');const e=t.clips[0]!.effects[0]!;if(e.kind!=='concat_gaussian_blur')throw new Error('key');return e.parameters}
async function input(channel:string,value:string,count:number){const el=document.querySelector<HTMLInputElement>(`input[aria-label="clip Concat Gaussian Blur ${channel}"]`)!;el.value=value;el.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(count));await tick()}
it('edits fractional radius with persistence, cache invalidation, undo, reset and removal',async()=>{
 component=mount(Inspector,{target:document.body});await tick();click('Добавить Concat Gaussian Blur');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 expect(document.querySelector<HTMLInputElement>('input[type=number]')!.value).toBe('10')
 await input('radius','2',2);expect(parameters().radius).toBe(2)
 await input('radius','5.9',3);expect(parameters().radius).toBe(5.9);expect(nestedComposition.renderedOutputs).toEqual({})
 const saved=JSON.parse(JSON.stringify(nestedComposition.document));expect(saved.compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({kind:'concat_gaussian_blur',parameters:{radius:5.9}})
 undoNestedComposition();expect(parameters().radius).toBe(2);redoNestedComposition();expect(parameters().radius).toBe(5.9)
 await tick();click('Сбросить Concat Gaussian Blur');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(4));expect(parameters()).toEqual({})
 undoNestedComposition();expect(parameters().radius).toBe(5.9);await tick();click('Удалить Concat Gaussian Blur');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(4));expect(document.querySelector('input')).toBeNull()
})
it('rejects empty and out-of-range radii',async()=>{
 component=mount(Inspector,{target:document.body});await tick();click('Добавить Concat Gaussian Blur');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 for (const value of ['0','51','']) {await input('radius',value,1);expect(parameters()).toEqual({})}
})
it('authors at the local cursor, then edits the key without changing the base radius',async()=>{
 component=mount(Inspector,{target:document.body});await tick();click('Добавить Concat Gaussian Blur');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 const key=()=>document.querySelector<HTMLButtonElement>('button[aria-label="clip Concat Gaussian Blur radius ключ"]')!
 expect(key().disabled).toBe(true)
 nestedComposition.timeUs=600000;await tick();key().click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));await tick()
 await input('radius','5.9',3)
 const saved=JSON.parse(JSON.stringify(nestedComposition.document));const effect=saved.compositions.root.content.tracks[0].clips[0].effects[0]
 expect(effect.parameters).toEqual({});expect(effect.animation.track.keyframes).toEqual([{tick:500000,value:5.9}])
 undoNestedComposition();await tick();expect(document.querySelector<HTMLInputElement>('input')!.value).toBe('10');redoNestedComposition();await tick();expect(document.querySelector<HTMLInputElement>('input')!.value).toBe('5.9')
 const remove=document.querySelector<HTMLButtonElement>('button[aria-label="clip Concat Gaussian Blur radius удалить анимацию"]')!;remove.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(4));await tick();expect(document.querySelector<HTMLInputElement>('input')!.value).toBe('10')
})
it('retains original normalized keys and requires explicit removal before native key authoring',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));const originalKeys=[{at:0,value:1},{at:1,value:10,ease:'out'}]
 project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'concat_gaussian_blur',parameters:{radius:10},concatKeys:originalKeys}];nestedComposition.document=project;nestedComposition.timeUs=600000
 component=mount(Inspector,{target:document.body});await tick()
 expect(document.querySelector<HTMLInputElement>('input')!.disabled).toBe(true)
 expect(document.querySelector<HTMLButtonElement>('button[aria-label="clip Concat Gaussian Blur radius ключ"]')!.disabled).toBe(true)
 document.querySelector<HTMLButtonElement>('button[aria-label="clip Concat Gaussian Blur удалить ключи Concat"]')!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 expect(document.querySelector<HTMLInputElement>('input')!.disabled).toBe(false)
 undoNestedComposition();await tick();expect(document.querySelector<HTMLInputElement>('input')!.disabled).toBe(true)
 const saved=JSON.parse(JSON.stringify(nestedComposition.document));expect(saved.compositions.root.content.tracks[0].clips[0].effects[0].concatKeys).toEqual(originalKeys)
})
it('edits original keys and custom easing with undo/redo and native-preview bridge data',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'concat_gaussian_blur',parameters:{radius:10},concatKeys:[{at:0,value:1},{at:1,value:10,ease:'out'}]}];nestedComposition.document=project
 component=mount(Inspector,{target:document.body});await tick();document.querySelector('details')!.open=true
 async function source(label:string,value:string,count:number){const element=document.querySelector<HTMLInputElement|HTMLSelectElement>(`[aria-label="clip Concat source ${label}"]`)!;element.value=value;element.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(count));await tick()}
 await source('0 at','0.25',1);await source('0 value','2',2);await source('1 easing','inOut',3);await source('1 ease 1','0.25',4)
 await source('new at','0',4);await source('new value','1',4)
 document.querySelector<HTMLButtonElement>('[aria-label="clip Concat source добавить"]')!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(5));await tick()
 document.querySelector<HTMLButtonElement>('[aria-label="clip Concat source 1 удалить"]')!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(6));await tick()
 const saved=JSON.parse(JSON.stringify(nestedComposition.document));const effect=saved.compositions.root.content.tracks[0].clips[0].effects[0]
 expect(effect).toEqual({kind:'concat_gaussian_blur',parameters:{radius:10},concatKeys:[{at:0,value:1,ease:[0,0,1,1]},{at:1,value:10,ease:[0.42,0.25,0.58,1]}]})
 expect(nestedComposition.renderedOutputs).toEqual({});undoNestedComposition();await tick();redoNestedComposition();await tick()
 expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(saved)
 if(process.env.CONCAT_SOURCE_KEYS_UI_EFFECT)writeFileSync(process.env.CONCAT_SOURCE_KEYS_UI_EFFECT,JSON.stringify(effect))
})
it('does not author an old staged position when a new source key input is empty or invalid',async()=>{
 component=mount(Inspector,{target:document.body});await tick();click('Добавить Concat Gaussian Blur');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick();document.querySelector('details')!.open=true
 const input=document.querySelector<HTMLInputElement>('[aria-label="clip Concat source new at"]')!;input.value='';input.dispatchEvent(new Event('change',{bubbles:true}));await tick()
 expect(document.querySelector<HTMLButtonElement>('[aria-label="clip Concat source добавить"]')!.disabled).toBe(true)
 expect(nestedHistory.past).toHaveLength(1)
})

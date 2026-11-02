import {writeFileSync} from 'node:fs'
// @ts-expect-error Tests require the Svelte client entry under Vitest SSR resolution.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { evaluateNestedFrame, type NativeNestedProject } from '$lib/api'
import { closeNestedComposition, nestedComposition, nestedHistory, undoNestedComposition, redoNestedComposition } from '$lib/state/nestedComposition.svelte'
import Inspector from './NestedConcatChromaticAberrationInspector.svelte'
vi.mock('$lib/api',()=>({evaluateNestedFrame:vi.fn()}))
let component:ReturnType<typeof mount>|undefined
beforeEach(()=>{
 closeNestedComposition();vi.mocked(evaluateNestedFrame).mockResolvedValue({entries:[]})
 nestedComposition.document={schemaVersion:1,root:'root',compositions:{root:{content:{tracks:[{kind:'video',id:'video',name:'Video',locked:false,clips:[{id:'clip',placement:{timelineStartTick:100000},effects:[]}]}]},instances:[]}}} as unknown as NativeNestedProject
 nestedComposition.renderedOutputs={root:'movie'}
})
afterEach(async()=>{if(component)await unmount(component);component=undefined;closeNestedComposition();vi.clearAllMocks();document.body.innerHTML=''})
function click(name:string){const b=[...document.querySelectorAll('button')].find(b=>b.textContent?.trim()===name);expect(b).toBeDefined();b!.click()}
function parameters(){const t=nestedComposition.document!.compositions.root!.content.tracks[0]!;if(t.kind!=='video')throw new Error('video');const e=t.clips[0]!.effects[0]!;if(e.kind!=='concat_chromatic_aberration')throw new Error('key');return e.parameters}
async function input(channel:string,value:string,count:number){const el=document.querySelector<HTMLInputElement>(`input[aria-label="clip Concat Chromatic Aberration ${channel}"]`)!;el.value=value;el.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(count));await tick()}
it('edits radial channel shift with persistence, cache invalidation, undo, reset and removal',async()=>{
 component=mount(Inspector,{target:document.body});await tick();click('Добавить Concat Chromatic Aberration');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 expect(document.querySelector<HTMLInputElement>('input[aria-label="clip Concat Chromatic Aberration shift"]')!.value).toBe('4')
 await input('shift','2',2);expect(parameters().shift).toBe(2)
 await input('shift','24',3);expect(parameters().shift).toBe(24);expect(nestedComposition.renderedOutputs).toEqual({})
 const saved=JSON.parse(JSON.stringify(nestedComposition.document));expect(saved.compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({kind:'concat_chromatic_aberration',parameters:{shift:24}})
 undoNestedComposition();expect(parameters().shift).toBe(2);redoNestedComposition();expect(parameters().shift).toBe(24)
 await tick();click('Сбросить Concat Chromatic Aberration');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(4));expect(parameters()).toEqual({})
 undoNestedComposition();expect(parameters().shift).toBe(24);await tick();click('Удалить Concat Chromatic Aberration');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(4));expect(document.querySelector('input')).toBeNull()
})
it('rejects fractional, empty and out-of-range shifts',async()=>{
 component=mount(Inspector,{target:document.body});await tick();click('Добавить Concat Chromatic Aberration');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 for (const value of ['0','25','']) {await input('shift',value,1);expect(parameters()).toEqual({})}
})

it('authors source keys and custom easing with undo and persistence',async()=>{
 component=mount(Inspector,{target:document.body});await tick();click('Добавить Concat Chromatic Aberration');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 const field=async(name:string,value:string)=>{const el=document.querySelector<HTMLInputElement>(`input[aria-label="${name}"]`)!;expect(el).not.toBeNull();el.value=value;el.dispatchEvent(new Event('change',{bubbles:true}));await tick()}
 await field('clip Concat source new at','0');await field('clip Concat source new value','1');click('Добавить ключ Concat');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));await tick()
 await field('clip Concat source new at','1');await field('clip Concat source new value','24');click('Добавить ключ Concat');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(3));await tick()
 await field('clip Concat source 1 ease 0','0.42');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(4));await tick()
 await field('clip Concat source 1 ease 1','-0.5');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(5));await tick()
 await field('clip Concat source 1 ease 2','0.58');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(6));await tick()
 const effects=()=>{const track=nestedComposition.document!.compositions.root!.content.tracks[0]!;if(track.kind!=='video')throw new Error('track');return track.clips[0]!.effects}
 const saved=JSON.parse(JSON.stringify(effects()));if(process.env.CONCAT_CHROMATIC_UI_OUTPUT)writeFileSync(process.env.CONCAT_CHROMATIC_UI_OUTPUT,JSON.stringify(saved[0]));expect(saved[0].concatKeys[1]).toEqual({at:1,value:24,ease:[0.42,-0.5,0.58,1]})
 expect(document.querySelector<HTMLInputElement>('input[aria-label="clip Concat Chromatic Aberration shift"]')!.disabled).toBe(true)
 undoNestedComposition();expect(JSON.stringify(effects())).not.toBe(JSON.stringify(saved));redoNestedComposition();expect(JSON.parse(JSON.stringify(effects()))).toEqual(saved)
 await tick();click('Удалить ключи Concat');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(7));await tick()
 expect(document.querySelector<HTMLInputElement>('input[aria-label="clip Concat Chromatic Aberration shift"]')!.disabled).toBe(false)
})

it('keeps authored fractional shifts without integer rounding',async()=>{
 component=mount(Inspector,{target:document.body});await tick();click('Добавить Concat Chromatic Aberration');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 await input('shift','8.1875',2);expect(parameters().shift).toBe(8.1875)
})

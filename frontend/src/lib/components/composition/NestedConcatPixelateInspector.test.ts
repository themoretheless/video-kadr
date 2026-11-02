// @ts-expect-error Tests require the Svelte client entry under Vitest SSR resolution.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { evaluateNestedFrame, type NativeNestedProject } from '$lib/api'
import { closeNestedComposition, nestedComposition, nestedHistory, undoNestedComposition, redoNestedComposition } from '$lib/state/nestedComposition.svelte'
import Inspector from './NestedConcatPixelateInspector.svelte'
vi.mock('$lib/api',()=>({evaluateNestedFrame:vi.fn()}))
let component:ReturnType<typeof mount>|undefined
beforeEach(()=>{
 closeNestedComposition();vi.mocked(evaluateNestedFrame).mockResolvedValue({entries:[]})
 nestedComposition.document={schemaVersion:1,root:'root',compositions:{root:{content:{tracks:[{kind:'video',id:'video',name:'Video',locked:false,clips:[{id:'clip',placement:{timelineStartTick:100000},effects:[]}]}]},instances:[]}}} as unknown as NativeNestedProject
 nestedComposition.renderedOutputs={root:'movie'}
})
afterEach(async()=>{if(component)await unmount(component);component=undefined;closeNestedComposition();vi.clearAllMocks();document.body.innerHTML=''})
function click(name:string){const b=[...document.querySelectorAll('button')].find(b=>b.textContent?.trim()===name);expect(b).toBeDefined();b!.click()}
function parameters(){const t=nestedComposition.document!.compositions.root!.content.tracks[0]!;if(t.kind!=='video')throw new Error('video');const e=t.clips[0]!.effects[0]!;if(e.kind!=='concat_pixelate')throw new Error('key');return e.parameters}
async function input(channel:string,value:string,count:number){const el=document.querySelector<HTMLInputElement>(`input[aria-label="clip Concat Pixelate ${channel}"]`)!;el.value=value;el.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(count));await tick()}
it('edits block size with persistence, cache invalidation, undo, reset and removal',async()=>{
 component=mount(Inspector,{target:document.body});await tick();click('Добавить Concat Pixelate');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 expect(document.querySelector<HTMLInputElement>('input[type=number]')!.value).toBe('16')
 await input('size','2',2);expect(parameters().size).toBe(2)
 await input('size','64',3);expect(parameters().size).toBe(64);expect(nestedComposition.renderedOutputs).toEqual({})
 const saved=JSON.parse(JSON.stringify(nestedComposition.document));expect(saved.compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({kind:'concat_pixelate',parameters:{size:64}})
 undoNestedComposition();expect(parameters().size).toBe(2);redoNestedComposition();expect(parameters().size).toBe(64)
 await tick();click('Сбросить Concat Pixelate');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(4));expect(parameters()).toEqual({})
 undoNestedComposition();expect(parameters().size).toBe(64);await tick();click('Удалить Concat Pixelate');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(4));expect(document.querySelector('input')).toBeNull()
})
it('rejects fractional, empty and out-of-range sizes',async()=>{
 component=mount(Inspector,{target:document.body});await tick();click('Добавить Concat Pixelate');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 for (const value of ['1','65','2.5','']) {await input('size',value,1);expect(parameters()).toEqual({})}
})

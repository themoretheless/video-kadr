// @ts-expect-error Tests require the Svelte client entry under Vitest SSR resolution.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { evaluateNestedFrame, type NativeNestedProject } from '$lib/api'
import { closeNestedComposition, nestedComposition, nestedHistory, undoNestedComposition, redoNestedComposition } from '$lib/state/nestedComposition.svelte'
import Inspector from './NestedConcatLumaKeyInspector.svelte'
vi.mock('$lib/api',()=>({evaluateNestedFrame:vi.fn()}))
let component:ReturnType<typeof mount>|undefined
beforeEach(()=>{
 closeNestedComposition();vi.mocked(evaluateNestedFrame).mockResolvedValue({entries:[]})
 nestedComposition.document={schemaVersion:1,root:'root',compositions:{root:{content:{tracks:[{kind:'video',id:'video',name:'Video',locked:false,clips:[{id:'clip',placement:{timelineStartTick:100000},effects:[]}]}]},instances:[]}}} as unknown as NativeNestedProject
 nestedComposition.renderedOutputs={root:'movie'}
})
afterEach(async()=>{if(component)await unmount(component);component=undefined;closeNestedComposition();vi.clearAllMocks();document.body.innerHTML=''})
function click(name:string){const b=[...document.querySelectorAll('button')].find(b=>b.textContent?.trim()===name);expect(b).toBeDefined();b!.click()}
function parameters(){const t=nestedComposition.document!.compositions.root!.content.tracks[0]!;if(t.kind!=='video')throw new Error('video');const e=t.clips[0]!.effects[0]!;if(e.kind!=='concat_luma_key')throw new Error('key');return e.parameters}
async function input(channel:string,value:string,count:number){const el=document.querySelector<HTMLInputElement>(`input[aria-label="clip Concat Luma Key ${channel}"]`)!;el.value=value;el.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(count));await tick()}
it('edits threshold, softness and inversion with persistence and undo/redo',async()=>{
 component=mount(Inspector,{target:document.body});await tick();click('Добавить Concat Luma Key');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 await input('threshold','50',2);await input('softness','0',3)
 const invert=document.querySelector<HTMLInputElement>('input[type=checkbox]')!;invert.checked=true;invert.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(4));await tick()
 expect(parameters()).toEqual({threshold:50,softness:0,invert:1});expect(nestedComposition.renderedOutputs).toEqual({})
 const saved=JSON.parse(JSON.stringify(nestedComposition.document));expect(saved.compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({kind:'concat_luma_key',parameters:parameters()})
 undoNestedComposition();expect(parameters().invert).toBeUndefined();redoNestedComposition();expect(parameters().invert).toBe(1)
 await tick();click('Сбросить Concat Luma Key');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(5));expect(parameters()).toEqual({})
 undoNestedComposition();expect(parameters().invert).toBe(1)
})
it('rejects empty and out-of-range threshold without authoring a change',async()=>{
 component=mount(Inspector,{target:document.body});await tick();click('Добавить Concat Luma Key');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 for(const value of ['-1','101','']) {await input('threshold',value,1);expect(parameters()).toEqual({})}
 await input('threshold','100',2);await input('softness','50',3);expect(parameters()).toEqual({threshold:100,softness:50})
})

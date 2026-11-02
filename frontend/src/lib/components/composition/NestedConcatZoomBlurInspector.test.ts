import {writeFileSync} from 'node:fs'
// @ts-expect-error Tests require the Svelte client entry under Vitest SSR resolution.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { evaluateNestedFrame, type NativeNestedProject } from '$lib/api'
import { closeNestedComposition, nestedComposition, nestedHistory, undoNestedComposition, redoNestedComposition } from '$lib/state/nestedComposition.svelte'
import Inspector from './NestedConcatZoomBlurInspector.svelte'
vi.mock('$lib/api',()=>({evaluateNestedFrame:vi.fn()}))
let component:ReturnType<typeof mount>|undefined
beforeEach(()=>{
 closeNestedComposition();vi.mocked(evaluateNestedFrame).mockResolvedValue({entries:[]})
 nestedComposition.document={schemaVersion:1,root:'root',compositions:{root:{content:{tracks:[{kind:'video',id:'video',name:'Video',locked:false,clips:[{id:'clip',placement:{timelineStartTick:100000},effects:[]}]}]},instances:[]}}} as unknown as NativeNestedProject
 nestedComposition.renderedOutputs={root:'movie'}
})
afterEach(async()=>{if(component)await unmount(component);component=undefined;closeNestedComposition();vi.clearAllMocks();document.body.innerHTML=''})
function click(name:string){const b=[...document.querySelectorAll('button')].find(b=>b.textContent?.trim()===name);expect(b).toBeDefined();b!.click()}
function parameters(){const t=nestedComposition.document!.compositions.root!.content.tracks[0]!;if(t.kind!=='video')throw new Error('video');const e=t.clips[0]!.effects[0]!;if(e.kind!=='concat_zoom_blur')throw new Error('key');return e.parameters}
async function input(channel:string,value:string,count:number){const el=document.querySelector<HTMLInputElement>(`input[aria-label="clip Concat Zoom Blur ${channel}"]`)!;el.value=value;el.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(count));await tick()}
it('edits band count with persistence, cache invalidation, undo, reset and removal',async()=>{
 component=mount(Inspector,{target:document.body});await tick();click('Добавить Concat Zoom Blur');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 expect(document.querySelector<HTMLInputElement>('input[type=number]')!.value).toBe('25')
 await input('amount','2',2);expect(parameters().amount).toBe(2)
 await input('amount','100',3);expect(parameters().amount).toBe(100);expect(nestedComposition.renderedOutputs).toEqual({})
 const saved=JSON.parse(JSON.stringify(nestedComposition.document));expect(saved.compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({kind:'concat_zoom_blur',parameters:{amount:100}})
 undoNestedComposition();expect(parameters().amount).toBe(2);redoNestedComposition();expect(parameters().amount).toBe(100)
 await tick();click('Сбросить Concat Zoom Blur');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(4));expect(parameters()).toEqual({})
 undoNestedComposition();expect(parameters().amount).toBe(100);await tick();click('Удалить Concat Zoom Blur');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(4));expect(document.querySelector('input')).toBeNull()
})
it('rejects fractional, empty and out-of-range amounts',async()=>{
 component=mount(Inspector,{target:document.body});await tick();click('Добавить Concat Zoom Blur');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 for (const value of ['-1','101','']) {await input('amount',value,1);expect(parameters()).toEqual({})}
})

it('edits the two independent centres',async()=>{
 component=mount(Inspector,{target:document.body});await tick();click('Добавить Concat Zoom Blur');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 await input('x','100',2);await input('y','0',3);expect(parameters()).toEqual({x:100,y:0})
})

it('authors independent original amount and centre curves with undo and persistence',async()=>{
 component=mount(Inspector,{target:document.body});await tick();click('Добавить Concat Zoom Blur');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 let count=1
 const field=async(label:string,value:string)=>{const el=document.querySelector<HTMLInputElement>(`input[aria-label="${label}"]`)!;expect(el).not.toBeNull();el.value=value;el.dispatchEvent(new Event('change',{bubbles:true}));await tick()}
 const add=async(channel:string,at:string,value:string)=>{await field(`clip Concat zoom ${channel} new at`,at);await field(`clip Concat zoom ${channel} new value`,value);document.querySelector<HTMLButtonElement>(`button[aria-label="clip Concat zoom ${channel} добавить"]`)!.click();count++;await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(count));await tick()}
 for(const [channel,a,b] of [['amount','0','100'],['x','0','50'],['y','100','50']]){await add(channel!,'0',a!);await add(channel!,'1',b!)}
 for(const [axis,value] of [['0','0.42'],['1','-0.5'],['2','0.58']]){await field(`clip Concat zoom amount 1 ease ${axis}`,value!);count++;await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(count));await tick()}
 const effects=()=>{const track=nestedComposition.document!.compositions.root!.content.tracks[0]!;if(track.kind!=='video')throw new Error('track');return track.clips[0]!.effects}
 const saved=JSON.parse(JSON.stringify(effects()));expect(saved[0].concatKeys.amount[1]).toEqual({at:1,value:100,ease:[0.42,-0.5,0.58,1]});expect(saved[0].concatKeys.x[1].value).toBe(50);expect(saved[0].concatKeys.y[0].value).toBe(100)
 if(process.env.CONCAT_ZOOM_UI_OUTPUT)writeFileSync(process.env.CONCAT_ZOOM_UI_OUTPUT,JSON.stringify(saved[0]))
 for(const channel of ['amount','x','y'])expect(document.querySelector<HTMLInputElement>(`input[aria-label="clip Concat Zoom Blur ${channel}"]`)!.disabled).toBe(true)
 undoNestedComposition();expect(JSON.stringify(effects())).not.toBe(JSON.stringify(saved));redoNestedComposition();expect(JSON.parse(JSON.stringify(effects()))).toEqual(saved)
 await tick();click('Удалить ключи Concat');count++;await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(count));await tick();expect(document.querySelector<HTMLInputElement>('input[aria-label="clip Concat Zoom Blur amount"]')!.disabled).toBe(false)
})

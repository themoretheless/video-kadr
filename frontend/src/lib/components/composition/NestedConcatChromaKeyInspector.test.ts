import {writeFileSync} from 'node:fs'
// @ts-expect-error Tests require the Svelte client entry under Vitest SSR resolution.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { evaluateNestedFrame, type NativeNestedProject } from '$lib/api'
import { closeNestedComposition, nestedComposition, nestedHistory, undoNestedComposition, redoNestedComposition } from '$lib/state/nestedComposition.svelte'
import Inspector from './NestedConcatChromaKeyInspector.svelte'
vi.mock('$lib/api',()=>({evaluateNestedFrame:vi.fn()}))
let component:ReturnType<typeof mount>|undefined
beforeEach(()=>{
 closeNestedComposition();vi.mocked(evaluateNestedFrame).mockResolvedValue({entries:[]})
 nestedComposition.document={schemaVersion:1,root:'root',compositions:{root:{content:{tracks:[{kind:'video',id:'video',name:'Video',locked:false,clips:[{id:'clip',placement:{timelineStartTick:100000},effects:[]}]}]},instances:[]}}} as unknown as NativeNestedProject
 nestedComposition.renderedOutputs={root:'movie'}
})
afterEach(async()=>{if(component)await unmount(component);component=undefined;closeNestedComposition();vi.clearAllMocks();document.body.innerHTML=''})
function click(name:string){const b=[...document.querySelectorAll('button')].find(b=>b.textContent?.trim()===name);expect(b).toBeDefined();b!.click()}
function parameters(){const t=nestedComposition.document!.compositions.root!.content.tracks[0]!;if(t.kind!=='video')throw new Error('video');const e=t.clips[0]!.effects[0]!;if(e.kind!=='concat_chroma_key')throw new Error('key');return e.parameters}
async function input(channel:string,value:string,count:number){const el=document.querySelector<HTMLInputElement>(`input[aria-label="clip Concat Chroma Key ${channel}"]`)!;el.value=value;el.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(count));await tick()}
it('edits packed key color and matte parameters through undo/redo with project persistence',async()=>{
 component=mount(Inspector,{target:document.body});await tick();click('Добавить Concat Chroma Key');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 expect(document.querySelector<HTMLInputElement>('input[type=color]')!.value).toBe('#00b140')
 await input('color','#0000ff',2);expect(parameters().color).toBe(65535)
 await input('similarity','45',3);await input('softness','20',4);await input('spill','75',5)
 expect(parameters()).toEqual({color:65535,similarity:45,softness:20,spill:75});expect(nestedComposition.renderedOutputs).toEqual({})
 const saved=JSON.parse(JSON.stringify(nestedComposition.document));expect(saved.compositions.root.content.tracks[0].clips[0].effects[0]).toEqual({kind:'concat_chroma_key',parameters:parameters()})
 undoNestedComposition();expect(parameters().spill).toBeUndefined();redoNestedComposition();expect(parameters().spill).toBe(75)
 await tick();click('Сбросить Concat Chroma Key');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(6));expect(parameters()).toEqual({})
 undoNestedComposition();expect(parameters().color).toBe(65535)
})
it('preserves unsigned RGBA colors above signed 32-bit range',async()=>{
 component=mount(Inspector,{target:document.body});await tick();click('Добавить Concat Chroma Key');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 await input('color','#ffffff',2);expect(parameters().color).toBe(4294967295)
 await input('color','#ff0000',3);expect(parameters().color).toBe(4278190335)
 undoNestedComposition();expect(parameters().color).toBe(4294967295)
})
it('preserves original channel rides during unrelated edits and explicit undoable removal',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));const keys={similarity:[{at:0,value:1},{at:1,value:60,ease:'out'}],color:[{at:0,value:65535},{at:1,value:4294967295}]}
 project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'concat_chroma_key',parameters:{},concatKeys:keys}];nestedComposition.document=project
 component=mount(Inspector,{target:document.body});await tick()
 expect(document.querySelector<HTMLInputElement>('input[type=color]')!.disabled).toBe(true)
 expect(document.querySelector<HTMLInputElement>('input[aria-label="clip Concat Chroma Key similarity"]')!.disabled).toBe(true)
 await input('spill','75',1)
 let saved=JSON.parse(JSON.stringify(nestedComposition.document));expect(saved.compositions.root.content.tracks[0].clips[0].effects[0].concatKeys).toEqual(keys)
 document.querySelector<HTMLButtonElement>('button[aria-label="clip Concat Chroma Key удалить ключи Concat"]')!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2));await tick()
 expect(document.querySelector<HTMLInputElement>('input[type=color]')!.disabled).toBe(false)
 undoNestedComposition();await tick();saved=JSON.parse(JSON.stringify(nestedComposition.document));expect(saved.compositions.root.content.tracks[0].clips[0].effects[0].concatKeys).toEqual(keys)
})
it('authors original chroma rides and custom easing with native-preview bridge data',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'concat_chroma_key',parameters:{color:16711935,similarity:1,softness:0,spill:0},concatKeys:{similarity:[{at:0,value:50},{at:1,value:1,ease:'out'}]}}];nestedComposition.document=project
 component=mount(Inspector,{target:document.body});await tick();for(const details of document.querySelectorAll('details'))details.open=true
 async function source(label:string,value:string,count:number){const element=document.querySelector<HTMLInputElement|HTMLSelectElement>(`[aria-label="clip Concat chroma ${label}"]`)!;element.value=value;element.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(count));await tick()}
 async function button(label:string,count:number){document.querySelector<HTMLButtonElement>(`[aria-label="clip Concat chroma ${label}"]`)!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(count));await tick()}
 await source('similarity 0 value','60',1);await source('similarity 1 easing','inOut',2);await source('similarity 1 ease 1','-0.5',3)
 await source('softness new at','0',3);await source('softness new value','0',3);await button('softness добавить',4)
 await source('spill new at','0',4);await source('spill new value','0',4);await button('spill добавить',5)
 await source('similarity new at','0.5',5);await source('similarity new value','30',5);await button('similarity добавить',6);await button('similarity 1 удалить',7)
 await source('similarity 0 at','0.25',8);await source('similarity 0 at','0',9)
 const saved=JSON.parse(JSON.stringify(nestedComposition.document));const effect=saved.compositions.root.content.tracks[0].clips[0].effects[0]
 expect(effect.concatKeys).toEqual({similarity:[{at:0,value:60},{at:1,value:1,ease:[0.42,-0.5,0.58,1]}],softness:[{at:0,value:0,ease:[0,0,1,1]}],spill:[{at:0,value:0,ease:[0,0,1,1]}]})
 expect(effect.parameters).toEqual({color:16711935,similarity:1,softness:0,spill:0});expect(nestedComposition.renderedOutputs).toEqual({})
 undoNestedComposition();await tick();redoNestedComposition();await tick();expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(saved)
 if(process.env.CONCAT_CHROMA_UI_EFFECT)writeFileSync(process.env.CONCAT_CHROMA_UI_EFFECT,JSON.stringify(effect))
})
it('edits unsigned source-key RGB and alpha, then restores the source curve with undo',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document));project.compositions.root.content.tracks[0].clips[0].effects=[{kind:'concat_chroma_key',parameters:{}}];nestedComposition.document=project
 component=mount(Inspector,{target:document.body});await tick();for(const details of document.querySelectorAll('details'))details.open=true
 async function source(label:string,value:string,count:number){const el=document.querySelector<HTMLInputElement>(`[aria-label="clip Concat chroma color ${label}"]`)!;el.value=value;el.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(count));await tick()}
 function keyValue(){return JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0].concatKeys.color[0].value}
 await source('new rgb','#ffffff',0);document.querySelector<HTMLButtonElement>('[aria-label="clip Concat chroma color добавить"]')!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick();expect(keyValue()).toBe(4294967295)
 await source('0 rgb','#0000ff',2);expect(keyValue()).toBe(65535);await source('0 alpha','127',3);await source('0 rgb','#ff0000',4);expect(keyValue()).toBe(4278190207)
 await source('0 alpha','255',5);expect(keyValue()).toBe(4278190335)
 document.querySelector<HTMLButtonElement>('[aria-label="clip Concat chroma color 0 удалить"]')!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(6));await tick();expect(document.querySelector<HTMLInputElement>('input[type=color]')!.disabled).toBe(false)
 undoNestedComposition();await tick();expect(keyValue()).toBe(4278190335)
})
it('refuses empty staged source values instead of submitting the previous value',async()=>{
 component=mount(Inspector,{target:document.body});await tick();click('Добавить Concat Chroma Key');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 const el=document.querySelector<HTMLInputElement>('[aria-label="clip Concat chroma similarity new value"]')!;el.value='';el.dispatchEvent(new Event('change',{bubbles:true}));await tick()
 expect(document.querySelector<HTMLButtonElement>('[aria-label="clip Concat chroma similarity добавить"]')!.disabled).toBe(true);expect(nestedHistory.past).toHaveLength(1)
 const alpha=document.querySelector<HTMLInputElement>('[aria-label="clip Concat chroma color new alpha"]')!;alpha.value='';alpha.dispatchEvent(new Event('change',{bubbles:true}));await tick()
 expect(document.querySelector<HTMLButtonElement>('[aria-label="clip Concat chroma color добавить"]')!.disabled).toBe(true);expect(nestedHistory.past).toHaveLength(1)
})

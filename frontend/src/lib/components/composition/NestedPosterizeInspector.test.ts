// @ts-expect-error Tests require the Svelte client entry under Vitest SSR resolution.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { evaluateNestedFrame, type NativeNestedProject } from '$lib/api'
import { closeNestedComposition, nestedComposition, nestedHistory, undoNestedComposition, redoNestedComposition } from '$lib/state/nestedComposition.svelte'
import Inspector from './NestedPosterizeInspector.svelte'
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
function parameters(){const track=nestedComposition.document!.compositions.root!.content.tracks[0]!;if(track.kind!=='video')throw new Error('video expected');const effect=track.clips[0]!.effects[0]!;if(effect.kind!=='posterize')throw new Error('grade expected');return effect.parameters}
it('authors correction and clip-local animation through validated undo history',async()=>{
 component=mount(Inspector,{target:document.body});await tick();click('Добавить Posterize');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 expect(document.querySelectorAll('input[type="number"]')).toHaveLength(1)
 await input('clip Posterize levels','20',2)
 expect(parameters().levels).toBe(20)
 expect(nestedComposition.renderedOutputs).toEqual({})
 const key=document.querySelector<HTMLButtonElement>('button[aria-label="clip Posterize levels ключ"]')!
 expect(key.disabled).toBe(true)
 nestedComposition.timeUs=600000;await tick();key.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(3));await tick()
 const track=nestedComposition.document!.compositions.root!.content.tracks[0]!
 if(track.kind!=='video')throw new Error('video')
 const effect=track.clips[0]!.effects[0]!
 if(effect.kind!=='posterize')throw new Error('Posterize')
 expect(effect.animation).toEqual({mode:'keyframes',track:{timeBase:1000000,interpolation:'linear',keyframes:[{tick:500000,value:20}]}})
 await input('clip Posterize 500000 value','25',4)
 await input('clip Posterize 500000 time','0.75',5)
 const changed=nestedComposition.document!.compositions.root!.content.tracks[0]!
 if(changed.kind!=='video'||changed.clips[0]!.effects[0]!.kind!=='posterize')throw new Error('levels')
 expect(changed.clips[0]!.effects[0]!.animation).toMatchObject({track:{keyframes:[{tick:750000,value:25}]}})
 document.querySelector<HTMLButtonElement>('button[aria-label="clip Posterize 750000 delete"]')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(6));await tick()
 undoNestedComposition();await tick()
 redoNestedComposition();await tick()
 click('Сбросить Posterize');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(7));expect(parameters()).toEqual({})
 undoNestedComposition();expect(parameters().levels).toBe(20)
})

it('authors image effects with local keys, persistence and undo/redo', async () => {
 const project = nestedComposition.document!
 nestedComposition.document = { ...project, compositions: { root: { ...project.compositions.root!, content: { ...project.compositions.root!.content, tracks: [{kind:'image', id:'image',name:'Image',locked:false,clips:[{id:'still',timelineStartTick:100000}]}] } } } } as unknown as NativeNestedProject
 component=mount(Inspector,{target:document.body});await tick()
 click('Добавить Posterize');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 await input('still Posterize levels','20',2)
 nestedComposition.timeUs=600000;await tick()
 document.querySelector<HTMLButtonElement>('button[aria-label="still Posterize levels ключ"]')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(3));await tick()
 const saved=JSON.parse(JSON.stringify(nestedComposition.document))
 expect(saved.compositions.root.content.tracks[0].clips[0].effects[0]).toMatchObject({kind:'posterize',parameters:{levels:20},animation:{track:{keyframes:[{tick:500000,value:20}]}}})
 undoNestedComposition();await tick()
 expect(JSON.parse(JSON.stringify(nestedComposition.document)).compositions.root.content.tracks[0].clips[0].effects[0].animation).toBeUndefined()
 redoNestedComposition();await tick()
 expect(JSON.parse(JSON.stringify(nestedComposition.document))).toEqual(saved)
 expect(evaluateNestedFrame).toHaveBeenCalledTimes(3)
})

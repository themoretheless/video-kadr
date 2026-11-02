// @ts-expect-error Tests require the Svelte client entry under Vitest SSR resolution.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { evaluateNestedFrame, type NativeNestedProject } from '$lib/api'
import { closeNestedComposition, nestedComposition, nestedHistory, undoNestedComposition, redoNestedComposition } from '$lib/state/nestedComposition.svelte'
import Inspector from './NestedBasicGradeInspector.svelte'
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
function parameters(){const track=nestedComposition.document!.compositions.root!.content.tracks[0]!;if(track.kind!=='video')throw new Error('video expected');const effect=track.clips[0]!.effects[0]!;if(effect.kind!=='basic_grade')throw new Error('grade expected');return effect.parameters}
it('authors correction and clip-local animation through validated undo history',async()=>{
 component=mount(Inspector,{target:document.body});await tick();click('Добавить Базовая коррекция');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 expect(document.querySelectorAll('input[type="number"]')).toHaveLength(9)
 await input('clip Базовая коррекция exposure','-3',2)
 expect(parameters().exposure).toBe(-3)
 expect(nestedComposition.renderedOutputs).toEqual({})
 const key=document.querySelector<HTMLButtonElement>('button[aria-label="clip Базовая коррекция exposure ключ"]')!
 expect(key.disabled).toBe(true)
 nestedComposition.timeUs=600000;await tick();key.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(3));await tick()
 const track=nestedComposition.document!.compositions.root!.content.tracks[0]!
 if(track.kind!=='video')throw new Error('video')
 const effect=track.clips[0]!.effects[0]!
 if(effect.kind!=='basic_grade')throw new Error('Базовая коррекция')
 expect(effect.animation?.exposure).toEqual({mode:'keyframes',track:{timeBase:1000000,interpolation:'linear',keyframes:[{tick:500000,value:-3}]}})
 click('Сбросить Базовая коррекция');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(4));expect(parameters()).toEqual({})
 undoNestedComposition();expect(parameters().exposure).toBe(-3)
})

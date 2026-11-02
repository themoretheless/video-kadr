// @ts-expect-error Tests require the Svelte client entry under Vitest SSR resolution.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { evaluateNestedFrame, type NativeNestedProject } from '$lib/api'
import { closeNestedComposition, nestedComposition, nestedHistory, undoNestedComposition, redoNestedComposition } from '$lib/state/nestedComposition.svelte'
import Inspector from './NestedVideoLimiterInspector.svelte'
vi.mock('$lib/api',()=>({evaluateNestedFrame:vi.fn()}))
let component:ReturnType<typeof mount>|undefined
beforeEach(()=>{
 closeNestedComposition();vi.mocked(evaluateNestedFrame).mockResolvedValue({entries:[]})
 nestedComposition.document={schemaVersion:1,root:'root',compositions:{root:{content:{tracks:[{kind:'video',id:'video',name:'Video',locked:false,clips:[{id:'clip',placement:{timelineStartTick:100000},effects:[]}]}]},instances:[]}}} as unknown as NativeNestedProject
 nestedComposition.renderedOutputs={root:'movie'}
})
afterEach(async()=>{if(component)await unmount(component);component=undefined;closeNestedComposition();vi.clearAllMocks();document.body.innerHTML=''})
function click(name:string){const button=[...document.querySelectorAll('button')].find(b=>b.textContent?.trim()===name);expect(button).toBeDefined();button!.click()}
function effect() {
 const track=nestedComposition.document!.compositions.root!.content.tracks[0]!
 if(track.kind!=='video')throw new Error('video')
 const effect=track.clips[0]!.effects[0]!
 if(effect.kind!=='video_limiter')throw new Error('limiter')
 return effect
}
it('authors source choices, warning color and undoable removal',async()=>{
 component=mount(Inspector,{target:document.body});await tick()
 click('Добавить Video Limiter');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 for(const [key,value,count] of [['clipLevel','9',2],['compression','4',3],['axis','2',4]] as const) {
  const select=document.querySelector<HTMLSelectElement>(`select[aria-label="clip Video Limiter ${key}"]`)!
  select.value=value;select.dispatchEvent(new Event('change',{bubbles:true}))
  await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(count));await tick()
  expect(effect().parameters[key]).toBe(Number(value))
 }
 const warning=document.querySelector<HTMLInputElement>('input[aria-label="clip Video Limiter warning"]')!
 warning.checked=true;warning.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(5));await tick()
 const color=document.querySelector<HTMLInputElement>('input[aria-label="clip Video Limiter color"]')!
 color.value='#00ff00';color.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(6));await tick()
 expect(effect().parameters).toEqual({clipLevel:9,compression:4,axis:2,gamutWarning:true,warningColor:[0,1,0,1]})
 expect(nestedComposition.renderedOutputs).toEqual({})
 nestedComposition.timeUs=600000;await tick()
 const key=document.querySelector<HTMLButtonElement>('button[aria-label="clip Video Limiter color key"]')!
 key.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(7));await tick()
 expect(effect().animation?.green).toEqual({mode:'keyframes',track:{timeBase:1000000,interpolation:'linear',keyframes:[{tick:500000,value:1}]}})
 expect(Object.keys(effect().animation!)).toEqual(['red','green','blue','alpha'])
 const value=document.querySelector<HTMLInputElement>('input[aria-label="clip Video Limiter green 500000 value"]')!
 value.value='0.4';value.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(8));await tick()
 const time=document.querySelector<HTMLInputElement>('input[aria-label="clip Video Limiter green 500000 time"]')!
 time.value='0.75';time.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(9));await tick()
 expect(effect().animation?.green).toMatchObject({track:{keyframes:[{tick:750000,value:0.4}]}})
 document.querySelector<HTMLButtonElement>('button[aria-label="clip Video Limiter green 750000 delete"]')!.click()
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(10));await tick()
 expect(effect().animation?.green).toBeUndefined()
 undoNestedComposition();expect(effect().animation?.green).toMatchObject({track:{keyframes:[{tick:750000,value:0.4}]}})
 redoNestedComposition();await tick()
 click('Удалить Video Limiter');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(11))
 undoNestedComposition();expect(effect().parameters.gamutWarning).toBe(true)
 redoNestedComposition();await tick();expect(document.querySelector('select')).toBeNull()
 expect(evaluateNestedFrame).toHaveBeenCalledTimes(11)
})

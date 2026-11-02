// @ts-expect-error Tests require the Svelte client entry under Vitest SSR resolution.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { evaluateNestedFrame, type NativeNestedProject } from '$lib/api'
import { closeNestedComposition, nestedComposition, nestedHistory, undoNestedComposition, redoNestedComposition } from '$lib/state/nestedComposition.svelte'
import Inspector from './NestedLevelsInspector.svelte'
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
 if(effect.kind!=='color_levels')throw new Error('limiter')
 return effect
}
async function change(label:string,value:string) {
 const input=document.querySelector<HTMLInputElement|HTMLSelectElement>(`[aria-label="${label}"]`)!
 expect(input).toBeTruthy();const count=nestedHistory.past.length
 input.value=value;input.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(count+1));await tick()
}
it('authors Levels scalars and local keys with five channels and undo',async()=>{
 component=mount(Inspector,{target:document.body});await tick()
 click('Добавить Levels');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 for(const [key,value] of [['inBlack','20'],['inWhite','150'],['gamma','200'],['outBlack','50'],['outWhite','220']] as const) await change(`clip Levels ${key}`,value)
 expect(document.querySelector<HTMLInputElement>('input[aria-label="clip Levels gamma"]')!.getAttribute('max')).toBe('1000')
 expect(effect().parameters).toEqual({inBlack:20,inWhite:150,gamma:200,outBlack:50,outWhite:220})
 expect(nestedComposition.renderedOutputs).toEqual({})
 nestedComposition.timeUs=600000;await tick()
 for(const channel of ['inBlack','inWhite','gamma','outBlack','outWhite']) {
  const count=nestedHistory.past.length
  document.querySelector<HTMLButtonElement>(`button[aria-label="clip Levels ${channel} key"]`)!.click()
  await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(count+1));await tick()
 }
 expect(Object.keys(effect().animation!)).toHaveLength(5)
 expect(effect().animation?.gamma).toEqual({mode:'keyframes',track:{timeBase:1000000,interpolation:'linear',keyframes:[{tick:500000,value:200}]}})
 await change('clip Levels gamma 500000 value','30')
 await change('clip Levels gamma 500000 time','0.75')
 expect(effect().animation?.gamma).toMatchObject({track:{keyframes:[{tick:750000,value:30}]}})
 document.querySelector<HTMLButtonElement>('button[aria-label="clip Levels gamma 750000 delete"]')!.click()
 await vi.waitFor(()=>expect(effect().animation?.gamma).toBeUndefined());await tick()
 undoNestedComposition();expect(effect().animation?.gamma).toMatchObject({track:{keyframes:[{tick:750000,value:30}]}})
 redoNestedComposition();await tick()
 click('Сбросить Levels');await vi.waitFor(()=>expect(effect().parameters).toEqual({}));await tick()
 expect(effect().animation).toBeUndefined()
 undoNestedComposition();await tick();expect(effect().parameters.inWhite).toBe(150)
 click('Удалить Levels');await vi.waitFor(()=>expect(document.querySelector('fieldset')).toBeNull())
 undoNestedComposition();expect(effect().parameters.gamma).toBe(200)
 expect(evaluateNestedFrame).toHaveBeenCalledTimes(16)
})
it('prevents authoring on locked tracks',async()=>{
 const track=nestedComposition.document!.compositions.root!.content.tracks[0]!
 if(track.kind!=='video')throw new Error('video')
 nestedComposition.document={...nestedComposition.document!,compositions:{root:{...nestedComposition.document!.compositions.root!,content:{...nestedComposition.document!.compositions.root!.content,tracks:[{...track,locked:true}]}}}}
 component=mount(Inspector,{target:document.body});await tick()
 expect(document.querySelector<HTMLButtonElement>('button')!.disabled).toBe(true)
 expect(evaluateNestedFrame).not.toHaveBeenCalled()
})
it('disables keys before clip start and keeps rejected edits out of history',async()=>{
 component=mount(Inspector,{target:document.body});await tick()
 click('Добавить Levels');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 expect(document.querySelector<HTMLButtonElement>('button[aria-label="clip Levels gamma key"]')!.disabled).toBe(true)
 vi.mocked(evaluateNestedFrame).mockRejectedValueOnce(new Error('Levels parameter out of range'))
 const inWhite=document.querySelector<HTMLInputElement>('input[aria-label="clip Levels inWhite"]')!
 inWhite.value='256';inWhite.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.error).toBe('Levels parameter out of range'));await tick()
 expect(effect().parameters.inWhite).toBeUndefined()
 expect(nestedHistory.past).toHaveLength(1)
})

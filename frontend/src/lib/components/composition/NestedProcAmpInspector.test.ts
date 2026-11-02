// @ts-expect-error Tests require the Svelte client entry under Vitest SSR resolution.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { evaluateNestedFrame, type NativeNestedProject } from '$lib/api'
import { closeNestedComposition, nestedComposition, nestedHistory, undoNestedComposition, redoNestedComposition } from '$lib/state/nestedComposition.svelte'
import Inspector from './NestedProcAmpInspector.svelte'
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
 if(effect.kind!=='proc_amp')throw new Error('limiter')
 return effect
}
async function change(label:string,value:string) {
 const input=document.querySelector<HTMLInputElement|HTMLSelectElement>(`[aria-label="${label}"]`)!
 expect(input).toBeTruthy();const count=nestedHistory.past.length
 input.value=value;input.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(count+1));await tick()
}
it('authors ProcAmp scalars and local keys with unbounded hue and undo',async()=>{
 component=mount(Inspector,{target:document.body});await tick()
 click('Добавить ProcAmp');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 for(const [key,value] of [['brightness','20'],['contrast','150'],['hue','1080'],['saturation','50']] as const) await change(`clip ProcAmp ${key}`,value)
 expect(document.querySelector<HTMLInputElement>('input[aria-label="clip ProcAmp hue"]')!.hasAttribute('max')).toBe(false)
 expect(effect().parameters).toEqual({brightness:20,contrast:150,hue:1080,saturation:50})
 expect(nestedComposition.renderedOutputs).toEqual({})
 nestedComposition.timeUs=600000;await tick()
 for(const channel of ['brightness','contrast','hue','saturation']) {
  const count=nestedHistory.past.length
  document.querySelector<HTMLButtonElement>(`button[aria-label="clip ProcAmp ${channel} key"]`)!.click()
  await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(count+1));await tick()
 }
 expect(Object.keys(effect().animation!)).toHaveLength(4)
 expect(effect().animation?.hue).toEqual({mode:'keyframes',track:{timeBase:1000000,interpolation:'linear',keyframes:[{tick:500000,value:1080}]}})
 await change('clip ProcAmp hue 500000 value','3')
 await change('clip ProcAmp hue 500000 time','0.75')
 expect(effect().animation?.hue).toMatchObject({track:{keyframes:[{tick:750000,value:3}]}})
 document.querySelector<HTMLButtonElement>('button[aria-label="clip ProcAmp hue 750000 delete"]')!.click()
 await vi.waitFor(()=>expect(effect().animation?.hue).toBeUndefined());await tick()
 undoNestedComposition();expect(effect().animation?.hue).toMatchObject({track:{keyframes:[{tick:750000,value:3}]}})
 redoNestedComposition();await tick()
 click('Сбросить ProcAmp');await vi.waitFor(()=>expect(effect().parameters).toEqual({}));await tick()
 expect(effect().animation).toBeUndefined()
 undoNestedComposition();await tick();expect(effect().parameters.contrast).toBe(150)
 click('Удалить ProcAmp');await vi.waitFor(()=>expect(document.querySelector('fieldset')).toBeNull())
 undoNestedComposition();expect(effect().parameters.hue).toBe(1080)
 expect(evaluateNestedFrame).toHaveBeenCalledTimes(14)
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
 click('Добавить ProcAmp');await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 expect(document.querySelector<HTMLButtonElement>('button[aria-label="clip ProcAmp hue key"]')!.disabled).toBe(true)
 vi.mocked(evaluateNestedFrame).mockRejectedValueOnce(new Error('ProcAmp parameter out of range'))
 const contrast=document.querySelector<HTMLInputElement>('input[aria-label="clip ProcAmp contrast"]')!
 contrast.value='201';contrast.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.error).toBe('ProcAmp parameter out of range'));await tick()
 expect(effect().parameters.contrast).toBeUndefined()
 expect(nestedHistory.past).toHaveLength(1)
})

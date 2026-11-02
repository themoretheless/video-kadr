// @ts-expect-error Tests need the Svelte client entry.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { expect, it, vi } from 'vitest'
import * as api from '$lib/api'
import NestedTrackControls from './NestedTrackControls.svelte'
import { closeNestedComposition, nestedComposition, nestedHistory, undoNestedComposition } from '$lib/state/nestedComposition.svelte'
import { createComposition } from '$lib/composition/commands'
import { buildCompositionWireDocument } from '$lib/composition/payload'
vi.mock('$lib/api', async importOriginal => ({ ...await importOriginal<typeof api>(), evaluateNestedFrame: vi.fn(async () => ({})) }))
it('adds a lane through actual controls, validates natively, restores output with Undo and preserves lock outputs', async () => {
  closeNestedComposition()
  const content = buildCompositionWireDocument(createComposition({ width: 16, height: 16, fps: 10, backgroundColor: '#000000' }))
  const document: api.NativeNestedProject = { schemaVersion: 1, root: 'root', compositions: { root: { content, instances: [] } } }
  nestedComposition.document = document; nestedComposition.activeCompositionId = 'root'; nestedComposition.renderedOutputs = { root: 'rendered', unrelated: 'keep' }
  const target = documentGlobal().createElement('div'); documentGlobal().body.append(target)
  const app = mount(NestedTrackControls, { target })
  try {
    await tick()
    const name = target.querySelector<HTMLInputElement>('[aria-label="Название новой дорожки"]')!
    name.value = 'Overlay'; name.dispatchEvent(new Event('input', { bubbles: true }))
    const add = [...target.querySelectorAll<HTMLButtonElement>('button')].find(button => button.textContent === 'Добавить дорожку')!
    add.click()
    await vi.waitFor(() => expect(nestedComposition.document!.compositions.root!.content.tracks.some(track => track.name === 'Overlay')).toBe(true))
    expect(api.evaluateNestedFrame).toHaveBeenCalled()
    expect(nestedComposition.renderedOutputs).toEqual({ unrelated: 'keep' })
    expect(nestedHistory.past).toHaveLength(1)
    nestedComposition.renderedOutputs = { root: 'fresh', unrelated: 'keep' }
    const lane = nestedComposition.document!.compositions.root!.content.tracks.find(track => track.name === 'Overlay')!
    await tick()
    const locked = target.querySelector<HTMLInputElement>(`[aria-label="Заблокирована ${lane.id}"]`)!
    locked.checked = true; locked.dispatchEvent(new Event('change', { bubbles: true }))
    await vi.waitFor(() => expect(nestedHistory.past).toHaveLength(2))
    expect(nestedComposition.renderedOutputs).toEqual({ root: 'fresh', unrelated: 'keep' })
    undoNestedComposition()
    undoNestedComposition()
    expect(nestedComposition.document).toEqual(document)
    expect(nestedComposition.renderedOutputs).toEqual({ root: 'rendered', unrelated: 'keep' })
  } finally { await unmount(app); target.remove(); closeNestedComposition() }
})
function documentGlobal(): Document { return globalThis.document }
it('controls the whole source lane and requires deletion confirmation for a still on its sibling',async()=>{
  const {addClip,addTrack,registerSource}=await import('$lib/composition/commands')
  closeNestedComposition()
  let editor=registerSource(createComposition({width:16,height:16,fps:25,backgroundColor:'#000000'}),{id:'still',kind:'image',durationTicks:0,width:16,height:16,hasAudio:false})
  editor=addTrack(editor,{id:'image',kind:'image',name:'Images',hidden:false,locked:false,clips:[]})
  editor=addClip(editor,'image',{id:'hold',kind:'image',sourceId:'still',timelineStartTicks:1000000,durationTicks:1000000,transform:{x:0,y:0,width:16,height:16,fit:'contain'},opacity:1})
  editor=addTrack(editor,{id:'video',kind:'video',name:'Video',hidden:false,muted:false,locked:false,clips:[]})
  const content=buildCompositionWireDocument(editor),image=content.tracks.find(track=>track.id==='image')!,video=content.tracks.find(track=>track.id==='video')!
  const document:api.NativeNestedProject={schemaVersion:1,root:'root',compositions:{root:{content:{...content,tracks:[image,video],metadata:{concatLaneGroups:{video:['video','image']}}},instances:[]}}}
  nestedComposition.document=document;nestedComposition.activeCompositionId='root'
  const target=documentGlobal().createElement('div');documentGlobal().body.append(target);const app=mount(NestedTrackControls,{target})
  try{
    await tick()
    const remove=[...target.querySelectorAll<HTMLButtonElement>('button')].find(button=>button.textContent==='Удалить дорожку Video')!
    expect(remove.disabled).toBe(true)
    const hidden=target.querySelector<HTMLInputElement>('[aria-label="Скрыта video"]')!;hidden.checked=true;hidden.dispatchEvent(new Event('change',{bubbles:true}))
    await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
    expect(target.querySelector<HTMLInputElement>('[aria-label="Скрыта image"]')!.checked).toBe(true)
    undoNestedComposition();await tick();expect(nestedComposition.document).toEqual(document)
    const confirm=[...target.querySelectorAll<HTMLLabelElement>('label')].find(label=>label.textContent?.includes('Удалять дорожку вместе'))!.querySelector<HTMLInputElement>('input')!
    confirm.checked=true;confirm.dispatchEvent(new Event('change',{bubbles:true}));await tick();expect(remove.disabled).toBe(false)
    remove.click();await vi.waitFor(()=>expect(nestedComposition.document!.compositions.root!.content.tracks).toHaveLength(0))
    expect(nestedComposition.document!.compositions.root!.content.metadata?.concatLaneGroups).toEqual({})
    undoNestedComposition();expect(nestedComposition.document).toEqual(document)
  }finally{unmount(app);target.remove();closeNestedComposition()}
})

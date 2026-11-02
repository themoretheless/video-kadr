// @ts-expect-error Tests require the Svelte client entry under Vitest SSR resolution.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { evaluateNestedFrame, type NativeNestedProject } from '$lib/api'
import { closeNestedComposition, nestedComposition, nestedHistory, undoNestedComposition, redoNestedComposition } from '$lib/state/nestedComposition.svelte'
import NestedChromaInspector from './NestedChromaInspector.svelte'
vi.mock('$lib/api', () => ({ evaluateNestedFrame: vi.fn() }))
let component: ReturnType<typeof mount> | undefined
beforeEach(() => {
  closeNestedComposition()
  vi.mocked(evaluateNestedFrame).mockResolvedValue({ entries: [] })
  // Transport/UI fixture: native model validation is tested by the API suite.
  nestedComposition.document = { schemaVersion:1,root:'root',compositions:{root:{content:{tracks:[{kind:'video',id:'track',name:'Video',locked:false,clips:[{id:'clip',placement:{timelineStartTick:500000},effects:[]}]}]},instances:[]}} } as unknown as NativeNestedProject
  nestedComposition.renderedOutputs = {root:'movie'}
  nestedComposition.timeUs = 750000
})
afterEach(async () => { if (component) await unmount(component); component=undefined; closeNestedComposition(); vi.clearAllMocks(); document.body.innerHTML='' })
function click(text: string): void { const button=[...document.querySelectorAll('button')].find((button) => button.textContent?.trim()===text); expect(button).toBeDefined(); button!.click() }
function effect() { const track=nestedComposition.document!.compositions.root!.content.tracks[0]!; if(track.kind!=='video') throw new Error('video expected'); return track.clips[0]!.effects[0]! }
it('validates edits, authors clip-local keyframes and restores output overrides on Undo', async () => {
  component=mount(NestedChromaInspector,{target:document.body})
  await tick(); click('Добавить chroma matte')
  await vi.waitFor(() => expect(nestedHistory.past).toHaveLength(1)); await tick()
  expect(evaluateNestedFrame).toHaveBeenCalled()
  expect(nestedComposition.renderedOutputs).toEqual({})
  const input=document.querySelector<HTMLInputElement>('input[aria-label="clip Баланс экрана база"]')!
  input.value='0.7'; input.dispatchEvent(new Event('change',{bubbles:true}))
  await vi.waitFor(() => expect(nestedHistory.past).toHaveLength(2)); await tick()
  input.closest('.channel-row')!.querySelector<HTMLButtonElement>('button')!.click()
  await vi.waitFor(() => expect(nestedHistory.past).toHaveLength(3))
  const current=effect(); expect(current.kind).toBe('chroma_matte')
  if(current.kind!=='chroma_matte') throw new Error('matte expected')
  expect(current.animation?.balance).toEqual({mode:'keyframes',track:{timeBase:1000000,interpolation:'linear',keyframes:[{tick:250000,value:0.7}]}})
  undoNestedComposition(); undoNestedComposition(); undoNestedComposition()
  expect(nestedComposition.renderedOutputs).toEqual({root:'movie'})
  redoNestedComposition(); expect(effect().kind).toBe('chroma_matte')
})
it('retains the original document and outputs when native validation rejects an edit', async () => {
  vi.mocked(evaluateNestedFrame).mockRejectedValue(new Error('invalid effect'))
  const before=JSON.stringify(nestedComposition.document)
  component=mount(NestedChromaInspector,{target:document.body})
  await tick(); click('Добавить chroma matte')
  await vi.waitFor(() => expect(nestedHistory.error).toBe('invalid effect'))
  expect(JSON.stringify(nestedComposition.document)).toBe(before)
  expect(nestedHistory.past).toHaveLength(0)
  expect(nestedComposition.renderedOutputs).toEqual({root:'movie'})
})

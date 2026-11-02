// @ts-expect-error Tests require the Svelte client entry.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, expect, it, vi } from 'vitest'
import * as api from '$lib/api'
import { closeNestedComposition, nestedComposition, nestedHistory, setNestedMediaPlaceholder, removeNestedMedia, undoNestedComposition, redoNestedComposition } from '$lib/state/nestedComposition.svelte'
import { createComposition } from '$lib/composition/commands'
import { buildCompositionWireDocument } from '$lib/composition/payload'
import Controls from './NestedProjectMedia.svelte'
vi.mock('$lib/api', async original => ({ ...await original<typeof api>(), setNativeProjectMediaPlaceholder: vi.fn(), removeNativeProjectMedia: vi.fn() }))
const source = { id: 'source-a', kind: 'image' as const, durationTicks: 0, width: 16, height: 16, hasAudio: false }
function project(): api.NativeNestedProject {
  const content = { ...buildCompositionWireDocument(createComposition({ width: 16, height: 16, fps: 24, backgroundColor: '#000000' })), sources: { [source.id]: source } }
  return { schemaVersion: 1, root: 'root', compositions: { root: { content, instances: [] }, other: { content, instances: [] } } }
}
let component: ReturnType<typeof mount> | undefined
afterEach(async () => { if (component) await unmount(component); component = undefined; closeNestedComposition(); vi.resetAllMocks(); document.body.innerHTML = '' })
it('marks shared media once through Rust and restores the registry with one Undo/Redo without invalidating output', async () => {
  const initial = project(); nestedComposition.document = initial; nestedComposition.activeCompositionId = 'other'; nestedComposition.timeUs = 123; nestedComposition.renderedOutputs = { root: 'movie' }
  vi.mocked(api.setNativeProjectMediaPlaceholder).mockImplementation(async (p, id, placeholder) => p.projectMedia?.[id]?.placeholder === placeholder ? p : { ...p, projectMedia: { ...p.projectMedia, [id]: { source, name: 'Shared source', placeholder } } })
  component = mount(Controls, { target: document.body }); await tick()
  expect(document.querySelectorAll('input[type=checkbox]')).toHaveLength(1); expect(document.body.textContent).toContain('Таймлайнов: 2')
  let check = document.querySelector<HTMLInputElement>('input[type=checkbox]')!; check.checked = true; check.dispatchEvent(new Event('change', { bubbles: true })); await tick(); await vi.waitFor(() => expect(nestedHistory.past).toHaveLength(1))
  expect(api.setNativeProjectMediaPlaceholder).toHaveBeenCalledExactlyOnceWith(initial, source.id, true); expect(nestedComposition.renderedOutputs).toEqual({ root: 'movie' }); expect(nestedComposition.activeCompositionId).toBe('other'); expect(nestedComposition.timeUs).toBe(123)
  expect(nestedComposition.document?.compositions).toEqual(initial.compositions)
  await setNestedMediaPlaceholder(source.id, true); expect(nestedHistory.past).toHaveLength(1)
  undoNestedComposition(); await tick(); expect(nestedComposition.document).toEqual(initial); expect(nestedComposition.renderedOutputs).toEqual({ root: 'movie' })
  redoNestedComposition(); await tick(); check = document.querySelector<HTMLInputElement>('input[type=checkbox]')!; expect(check.checked).toBe(true)
})
it('retains media with no timeline placement and drops a late result after project close', async () => {
  const initial = project(); initial.projectMedia = { [source.id]: { source, name: 'Retained', placeholder: true } }; initial.compositions = { root: { ...initial.compositions.root, content: { ...initial.compositions.root.content, sources: {} } } }
  nestedComposition.document = initial; component = mount(Controls, { target: document.body }); await tick(); expect(document.body.textContent).toContain('Таймлайнов: 0')
  let resolve!: (p: api.NativeNestedProject) => void; vi.mocked(api.setNativeProjectMediaPlaceholder).mockImplementation(() => new Promise(done => { resolve = done }))
  const pending = setNestedMediaPlaceholder(source.id, false); await vi.waitFor(() => expect(api.setNativeProjectMediaPlaceholder).toHaveBeenCalledTimes(1)); closeNestedComposition(); resolve({ ...initial, projectMedia: { [source.id]: { source, name: 'Retained' } } }); await pending; await tick()
  expect(nestedComposition.document).toBeNull(); expect(nestedHistory.past).toHaveLength(0); expect(api.setNativeProjectMediaPlaceholder).toHaveBeenCalledTimes(1)
})

it('removes media project-wide through Rust in one history step and invalidates rendered outputs', async () => {
  const initial = project(); nestedComposition.document = initial; nestedComposition.activeCompositionId = 'other'; nestedComposition.timeUs = 123; nestedComposition.renderedOutputs = { root: 'old movie' }
  const removed = { ...initial, compositions: Object.fromEntries(Object.entries(initial.compositions).map(([id,node]) => [id,{...node,content:{...node.content,sources:{}}}])) }
  vi.mocked(api.removeNativeProjectMedia).mockResolvedValue(removed)
  component = mount(Controls,{target:document.body}); await tick()
  document.querySelector<HTMLButtonElement>('button')!.click()
  await vi.waitFor(() => expect(nestedHistory.past).toHaveLength(1)); await tick()
  expect(api.removeNativeProjectMedia).toHaveBeenCalledExactlyOnceWith(initial,source.id)
  expect(nestedComposition.document).toEqual(removed); expect(nestedComposition.renderedOutputs).toEqual({})
  expect(nestedComposition.activeCompositionId).toBe('other'); expect(nestedComposition.timeUs).toBe(123)
  undoNestedComposition(); expect(nestedComposition.document).toEqual(initial); expect(nestedComposition.renderedOutputs).toEqual({root:'old movie'})
  redoNestedComposition(); expect(nestedComposition.document).toEqual(removed); expect(nestedComposition.renderedOutputs).toEqual({})
})
it('keeps the project and history on refusal and ignores a removal after close', async () => {
  const initial=project(); nestedComposition.document=initial
  vi.mocked(api.removeNativeProjectMedia).mockRejectedValue(new Error('dependent mask'))
  await removeNestedMedia(source.id)
  expect(nestedComposition.document).toEqual(initial); expect(nestedHistory.past).toHaveLength(0); expect(nestedHistory.error).toBe('dependent mask')
  let resolve!:(value:api.NativeNestedProject)=>void
  vi.mocked(api.removeNativeProjectMedia).mockImplementation(()=>new Promise(done=>{resolve=done}))
  const pending=removeNestedMedia(source.id); await vi.waitFor(()=>expect(api.removeNativeProjectMedia).toHaveBeenCalledTimes(2))
  closeNestedComposition(); resolve(initial); await pending
  expect(nestedComposition.document).toBeNull(); expect(nestedHistory.past).toHaveLength(0)
})

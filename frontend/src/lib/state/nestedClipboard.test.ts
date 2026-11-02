import { serializeNativeClipboard } from '$lib/composition/nativeClipboardText'
import { beforeEach,expect,it,vi } from 'vitest'
import fixture from '../../../../backend/tests/fixtures/concat-merge-preview.json'
import * as api from '$lib/api'
import { closeNestedComposition,nestedComposition,nestedClipboard,nestedHistory,cutNestedSelectionToClipboard,undoNestedComposition,redoNestedComposition,importNestedClipboardText } from './nestedComposition.svelte'
vi.mock('$lib/api',()=>({evaluateNestedFrame:vi.fn()}))
beforeEach(()=>{closeNestedComposition();nestedClipboard.value=null;vi.clearAllMocks();vi.mocked(api.evaluateNestedFrame).mockResolvedValue({entries:[]})})
function open() {
  const project=structuredClone(fixture.nativeCaptionPastes[4]!.reference) as unknown as api.NativeNestedProject
  nestedComposition.document=project;nestedComposition.activeCompositionId='new-timeline';nestedComposition.timeUs=1250000
  nestedComposition.renderedOutputs={'new-timeline':'old-output',unrelated:'keep'}
  return project
}
it('cuts linked media and captions together, stores independent content only after validation and undoes as one edit',async()=>{
  const original=open()
  await cutNestedSelectionToClipboard('new-timeline',['auto-first','cue-first','cue-second'])
  expect(nestedHistory.past).toHaveLength(1)
  const node=nestedComposition.document!.compositions['new-timeline']!
  expect(node.content.tracks.reduce((n,track)=>n+track.clips.length,0)).toBe(0)
  expect(node.content.captionTracks![0]!.clips).toEqual([])
  expect(nestedComposition.renderedOutputs).toEqual({unrelated:'keep'})
  expect(nestedComposition.timeUs).toBe(1250000)
  const saved=JSON.stringify(nestedClipboard.value), captured=nestedClipboard.value!.compositions.clipboard!
  expect(captured.content.tracks.reduce((n,track)=>n+track.clips.length,0)).toBe(2)
  expect(captured.content.captionTracks![0]!.clips).toHaveLength(2)
  undoNestedComposition();expect(nestedComposition.document).toEqual(original)
  expect(nestedComposition.renderedOutputs['new-timeline']).toBe('old-output')
  expect(JSON.stringify(nestedClipboard.value)).toBe(saved)
  redoNestedComposition();expect(nestedComposition.document!.compositions['new-timeline']!.content.captionTracks![0]!.clips).toEqual([])
  expect(JSON.stringify(nestedClipboard.value)).toBe(saved)
})
it('keeps document, history, outputs and the previous clipboard on native rejection or a locked participant',async()=>{
  const original=open();nestedClipboard.value=structuredClone(original)
  const saved=JSON.stringify(nestedClipboard.value)
  vi.mocked(api.evaluateNestedFrame).mockRejectedValueOnce(new Error('Native refusal'))
  await cutNestedSelectionToClipboard('new-timeline',['auto-first','cue-first'])
  expect(nestedHistory.error).toBe('Native refusal');expect(nestedHistory.past).toHaveLength(0)
  expect(nestedComposition.document).toEqual(original);expect(JSON.stringify(nestedClipboard.value)).toBe(saved)
  const node=original.compositions['new-timeline']!
  nestedComposition.document={...original,compositions:{...original.compositions,'new-timeline':{...node,content:{...node.content,captionTracks:node.content.captionTracks!.map(track=>({...track,locked:true}))}}}}
  const locked=JSON.stringify(nestedComposition.document)
  await cutNestedSelectionToClipboard('new-timeline',['auto-first','cue-first'])
  expect(nestedHistory.error).toContain('заблокирована');expect(JSON.stringify(nestedComposition.document)).toBe(locked)
  expect(JSON.stringify(nestedClipboard.value)).toBe(saved);expect(nestedHistory.past).toHaveLength(0)
})
it('refuses a pending cut after source or clipboard changes, without overwriting the newer content',async()=>{
  const original=open();let release!:(value:api.NativeNestedFrameResponse)=>void
  vi.mocked(api.evaluateNestedFrame).mockImplementationOnce(()=>new Promise(resolve=>{release=resolve}))
  const cutting=cutNestedSelectionToClipboard('new-timeline',['auto-first','cue-first'])
  await vi.waitFor(()=>expect(release).toBeDefined())
  const newer={...original,root:'new-timeline'};nestedClipboard.value=newer
  release({entries:[]});await cutting
  expect(nestedHistory.error).toContain('Буфер изменился');expect(nestedComposition.document).toEqual(original)
  expect(nestedClipboard.value).toEqual(newer);expect(nestedHistory.past).toHaveLength(0)
  vi.mocked(api.evaluateNestedFrame).mockImplementationOnce(()=>new Promise(resolve=>{release=resolve}));release=undefined as unknown as typeof release
  const again=cutNestedSelectionToClipboard('new-timeline',['auto-first','cue-first'])
  await vi.waitFor(()=>expect(release).toBeDefined())
  nestedComposition.document={...original,compositions:{...original.compositions,'new-timeline':{...original.compositions['new-timeline']!,durationUs:6000000}}}
  const changed=JSON.stringify(nestedComposition.document);release({entries:[]});await again
  expect(JSON.stringify(nestedComposition.document)).toBe(changed);expect(nestedClipboard.value).toEqual(newer)
  expect(nestedHistory.past).toHaveLength(0)
})
it('does not publish a pending cut after closing and reopening an identical project',async()=>{
  const original=open();nestedClipboard.value=structuredClone(original)
  const saved=JSON.stringify(nestedClipboard.value);let release!:(value:api.NativeNestedFrameResponse)=>void
  vi.mocked(api.evaluateNestedFrame).mockImplementationOnce(()=>new Promise(resolve=>{release=resolve}))
  const cutting=cutNestedSelectionToClipboard('new-timeline',['auto-first','cue-first'])
  await vi.waitFor(()=>expect(release).toBeDefined())
  closeNestedComposition();nestedComposition.document=structuredClone(original)
  release({entries:[]});await cutting
  expect(nestedComposition.document).toEqual(original);expect(nestedHistory.past).toHaveLength(0)
  expect(JSON.stringify(nestedClipboard.value)).toBe(saved)
})

it('validates imported text before replacing the session buffer without authoring the project',async()=>{
  const original=open(),captured=fixture.nativeCaptionPastes[0]!.clipboard as unknown as api.NativeNestedProject
  expect(await importNestedClipboardText(serializeNativeClipboard(captured))).toBe(true)
  expect(api.evaluateNestedFrame).toHaveBeenCalledWith(captured,0)
  expect(nestedClipboard.value).toEqual(captured);expect(nestedComposition.document).toEqual(original)
  expect(nestedHistory.past).toHaveLength(0)
  const saved=JSON.stringify(nestedClipboard.value)
  expect(await importNestedClipboardText('unrelated text')).toBe(false);expect(JSON.stringify(nestedClipboard.value)).toBe(saved)
  vi.mocked(api.evaluateNestedFrame).mockRejectedValueOnce(new Error('Invalid caption style'))
  expect(await importNestedClipboardText(serializeNativeClipboard(captured))).toBe(false)
  expect(nestedHistory.error).toBe('Invalid caption style');expect(JSON.stringify(nestedClipboard.value)).toBe(saved)
})
it('does not overwrite a newer buffer or closed project when text validation finishes late',async()=>{
  const original=open(),captured=fixture.nativeCaptionPastes[0]!.clipboard as unknown as api.NativeNestedProject
  let release!:(value:api.NativeNestedFrameResponse)=>void
  vi.mocked(api.evaluateNestedFrame).mockImplementationOnce(()=>new Promise(resolve=>{release=resolve}))
  const importing=importNestedClipboardText(serializeNativeClipboard(captured))
  await vi.waitFor(()=>expect(release).toBeDefined())
  nestedClipboard.value=structuredClone(original);const saved=JSON.stringify(nestedClipboard.value)
  release({entries:[]});expect(await importing).toBe(false);expect(JSON.stringify(nestedClipboard.value)).toBe(saved)
  release=undefined as unknown as typeof release
  vi.mocked(api.evaluateNestedFrame).mockImplementationOnce(()=>new Promise(resolve=>{release=resolve}))
  const closed=importNestedClipboardText(serializeNativeClipboard(captured))
  await vi.waitFor(()=>expect(release).toBeDefined());closeNestedComposition();release({entries:[]})
  expect(await closed).toBe(false);expect(JSON.stringify(nestedClipboard.value)).toBe(saved)
})

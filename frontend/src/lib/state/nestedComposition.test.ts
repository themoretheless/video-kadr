import { beforeEach, expect, it, vi } from 'vitest'
import * as api from '$lib/api'
import { closeNestedComposition, groupNestedDocument, nestedComposition, nestedHistory, redoNestedComposition, undoNestedComposition, splitNestedInstance, replaceNestedFonts,adoptImportedConcatProject } from './nestedComposition.svelte'
vi.mock('$lib/api', () => ({ groupNestedComposition: vi.fn(), splitNestedComposition: vi.fn(), replaceProjectFonts: vi.fn(), evaluateNestedFrame: vi.fn(), moveNativeNestedClips: vi.fn() }))
beforeEach(() => { closeNestedComposition(); vi.clearAllMocks() })

it('groups once through native API and restores document/output bindings through Undo/Redo', async () => {
  const document: api.NativeNestedProject = { schemaVersion: 1, root: 'root', compositions: {} }
  const grouped: api.NativeNestedProject = { ...document, root: 'grouped' }
  nestedComposition.document = document
  nestedComposition.renderedOutputs = { root: 'movie' }
  vi.mocked(api.groupNestedComposition).mockResolvedValue(grouped)
  await groupNestedDocument('root', 'child', 'instance')
  expect(api.groupNestedComposition).toHaveBeenCalledWith(document, 'root', 'child', 'instance')
  expect(nestedComposition.document).toEqual(grouped)
  expect(nestedComposition.renderedOutputs).toEqual({})
  expect(nestedHistory.past).toHaveLength(1)
  undoNestedComposition()
  expect(nestedComposition.document).toEqual(document)
  expect(nestedComposition.renderedOutputs).toEqual({ root: 'movie' })
  redoNestedComposition()
  expect(nestedComposition.document).toEqual(grouped)
  expect(nestedComposition.renderedOutputs).toEqual({})
})

it('discards native results after closing the session and retains failed edits', async () => {
  const document: api.NativeNestedProject = { schemaVersion: 1, root: 'root', compositions: {} }
  nestedComposition.document = document
  let resolve!: (value: api.NativeNestedProject) => void
  vi.mocked(api.groupNestedComposition).mockReturnValue(new Promise((done) => { resolve = done }))
  const pending = groupNestedDocument('root', 'child', 'instance')
  await vi.waitFor(() => expect(api.groupNestedComposition).toHaveBeenCalled())
  closeNestedComposition()
  resolve({ ...document, root: 'stale' })
  await pending
  expect(nestedComposition.document).toBeNull()
  expect(nestedHistory.past).toEqual([])
  nestedComposition.document = document
  vi.mocked(api.groupNestedComposition).mockRejectedValue(new Error('occupied child'))
  await groupNestedDocument('root', 'child', 'instance')
  expect(nestedComposition.document).toEqual(document)
  expect(nestedHistory.error).toBe('occupied child')
  expect(nestedHistory.busy).toBe(false)
})

it('records native split as one undoable edit while preserving rendered movie bindings', async () => {
  const document: api.NativeNestedProject = { schemaVersion: 1, root: 'root', compositions: {} }
  const split = { ...document, root: 'split-fixture' }
  nestedComposition.document = document
  nestedComposition.renderedOutputs = { root: 'movie' }
  vi.mocked(api.splitNestedComposition).mockResolvedValue(split)
  await splitNestedInstance('root', 'left', 'right', 800000)
  expect(api.splitNestedComposition).toHaveBeenCalledWith(document, 'root', 'left', 'right', 800000)
  expect(nestedComposition.document).toEqual(split)
  expect(nestedComposition.renderedOutputs).toEqual({ root: 'movie' })
  expect(nestedHistory.past).toHaveLength(1)
  undoNestedComposition()
  expect(nestedComposition.document).toEqual(document)
  redoNestedComposition()
  expect(nestedComposition.document).toEqual(split)
})

it('font replacement invalidates rendered outputs and shares project Undo/Redo',async()=>{
 const before:api.NativeNestedProject={schemaVersion:1,root:'root',compositions:{}}
 const after:api.NativeNestedProject={...before,luts:[]}
 nestedComposition.document=before;nestedComposition.renderedOutputs={root:'old-render'}
 vi.mocked(api.replaceProjectFonts).mockResolvedValue({project:after,replaced:3})
 await replaceNestedFonts('Inter',{family:'External',style:'Bold'})
 expect(api.replaceProjectFonts).toHaveBeenCalledWith(before,'Inter',{family:'External',style:'Bold'})
 expect(nestedComposition.document).toEqual(after);expect(nestedComposition.renderedOutputs).toEqual({})
 undoNestedComposition();expect(nestedComposition.document).toEqual(before);expect(nestedComposition.renderedOutputs).toEqual({root:'old-render'})
 redoNestedComposition();expect(nestedComposition.document).toEqual(after);expect(nestedComposition.renderedOutputs).toEqual({})
})

it('no-op font substitution preserves renders and history',async()=>{
 const document:api.NativeNestedProject={schemaVersion:1,root:'root',compositions:{}}
 nestedComposition.document=document;nestedComposition.renderedOutputs={root:'render'}
 vi.mocked(api.replaceProjectFonts).mockResolvedValue({project:document,replaced:0})
 await replaceNestedFonts('Missing','External')
 expect(nestedHistory.past).toHaveLength(0);expect(nestedComposition.renderedOutputs).toEqual({root:'render'})
})


import { moveNestedTimeline, renameNestedTimeline } from './nestedComposition.svelte'
import { buildCompositionWireDocument } from '$lib/composition/payload'
import { createComposition } from '$lib/composition/commands'
it('moves tabs with one Undo without changing selection or rendered outputs, and renames through native validation', async () => {
  const content = buildCompositionWireDocument(createComposition({ width: 16, height: 16, fps: 10, backgroundColor: '#000000' }))
  const document: api.NativeNestedProject = { schemaVersion: 1, root: 'root', compositions: { root: { content, instances: [] }, child: { content, instances: [] } } }
  nestedComposition.document = document; nestedComposition.activeCompositionId = 'root'; nestedComposition.renderedOutputs = { root: 'movie' }
  vi.mocked(api.evaluateNestedFrame).mockResolvedValue({ entries: [] })
  await moveNestedTimeline('root', 0)
  expect(nestedComposition.document?.timelineOrder).toEqual(['root', 'child'])
  expect(nestedComposition.activeCompositionId).toBe('root')
  expect(nestedComposition.renderedOutputs).toEqual({ root: 'movie' })
  expect(nestedHistory.past).toHaveLength(1)
  undoNestedComposition(); expect(nestedComposition.document).toEqual(document)
  redoNestedComposition(); expect(nestedComposition.document?.timelineOrder).toEqual(['root', 'child'])
  await moveNestedTimeline('root', 0); expect(nestedHistory.past).toHaveLength(1)
  await renameNestedTimeline('root', '  Main cut  ')
  expect(nestedComposition.document?.compositions.root?.content.metadata?.sequenceName).toBe('Main cut')
  expect(nestedComposition.renderedOutputs).toEqual({})
  expect(nestedHistory.past).toHaveLength(2)
  undoNestedComposition(); expect(nestedComposition.renderedOutputs).toEqual({ root: 'movie' })
  const before = JSON.stringify(nestedComposition.document)
  await renameNestedTimeline('root', '  ')
  expect(JSON.stringify(nestedComposition.document)).toBe(before)
  expect(nestedHistory.error).toContain('Имя таймлайна')
})

import { addNestedTimeline } from './nestedComposition.svelte'
it('creates and selects a timeline with one Undo/Redo, retaining previous view and outputs', async () => {
  const content = buildCompositionWireDocument(createComposition({ width: 16, height: 16, fps: 10, backgroundColor: '#000000' }))
  const document: api.NativeNestedProject = { schemaVersion: 1, root: 'root', compositions: { root: { content, instances: [] } } }
  nestedComposition.document = document; nestedComposition.activeCompositionId = 'root'; nestedComposition.timeUs = 123456; nestedComposition.renderedOutputs = { root: 'movie' }
  vi.mocked(api.evaluateNestedFrame).mockResolvedValue({ entries: [] })
  await addNestedTimeline('new-timeline')
  expect(nestedComposition.activeCompositionId).toBe('new-timeline')
  expect(nestedComposition.timeUs).toBe(0)
  expect(nestedHistory.past).toHaveLength(1)
  expect(nestedComposition.renderedOutputs).toEqual({ root: 'movie' })
  undoNestedComposition()
  expect(nestedComposition.document).toEqual(document)
  expect(nestedComposition.activeCompositionId).toBe('root')
  expect(nestedComposition.timeUs).toBe(123456)
  redoNestedComposition()
  expect(nestedComposition.activeCompositionId).toBe('new-timeline')
  expect(nestedComposition.timeUs).toBe(0)
  expect(nestedComposition.document?.compositions['new-timeline']?.content.tracks).toHaveLength(4)
  const before = JSON.stringify(nestedComposition.document)
  vi.mocked(api.evaluateNestedFrame).mockRejectedValue(new Error('native refusal'))
  await addNestedTimeline('refused-timeline')
  expect(JSON.stringify(nestedComposition.document)).toBe(before)
  expect(nestedComposition.activeCompositionId).toBe('new-timeline')
  expect(nestedHistory.past).toHaveLength(1)
  expect(nestedHistory.error).toBe('native refusal')
})

import { removeNestedTimeline } from './nestedComposition.svelte'
it('removes the active timeline and only its output binding, with selection/time restored through Undo/Redo', async () => {
  const content = buildCompositionWireDocument(createComposition({ width: 16, height: 16, fps: 10, backgroundColor: '#000000' }))
  const document: api.NativeNestedProject = { schemaVersion: 1, root: 'root', timelineOrder: ['root', 'child'], compositions: { root: { content, instances: [] }, child: { content, instances: [] } } }
  nestedComposition.document = document; nestedComposition.activeCompositionId = 'child'; nestedComposition.timeUs = 123456; nestedComposition.renderedOutputs = { root: 'root-movie', child: 'child-movie' }
  vi.mocked(api.evaluateNestedFrame).mockResolvedValue({ entries: [] })
  await removeNestedTimeline('child')
  expect(nestedComposition.document?.compositions.child).toBeUndefined()
  expect(nestedComposition.activeCompositionId).toBe('root')
  expect(nestedComposition.timeUs).toBe(0)
  expect(nestedComposition.renderedOutputs).toEqual({ root: 'root-movie' })
  expect(nestedHistory.past).toHaveLength(1)
  undoNestedComposition()
  expect(nestedComposition.document).toEqual(document)
  expect(nestedComposition.activeCompositionId).toBe('child')
  expect(nestedComposition.timeUs).toBe(123456)
  expect(nestedComposition.renderedOutputs).toEqual({ root: 'root-movie', child: 'child-movie' })
  redoNestedComposition()
  expect(nestedComposition.activeCompositionId).toBe('root')
  expect(nestedComposition.renderedOutputs).toEqual({ root: 'root-movie' })
  await removeNestedTimeline('root')
  expect(nestedComposition.document?.compositions.root).toBeDefined()
  expect(nestedHistory.error).toContain('последний')
  expect(nestedHistory.past).toHaveLength(1)
})
it('requires placement opt-in and selectively invalidates ancestors while preserving an unrelated active view', async () => {
  const content = buildCompositionWireDocument(createComposition({ width: 16, height: 16, fps: 10, backgroundColor: '#000000' }))
  const placed = (id: string, composition: string): api.NativeCompositionInstance => ({ id, timing: { composition, timelineStartUs: 0, sourceInUs: 0, sourceOutUs: 1_000_000 }, transform: { x: { mode: 'constant', value: 0 }, y: { mode: 'constant', value: 0 }, scaleX: { mode: 'constant', value: 1 }, scaleY: { mode: 'constant', value: 1 }, rotationDegrees: { mode: 'constant', value: 0 }, anchorX: 0.5, anchorY: 0.5 }, opacity: { mode: 'constant', value: 1 }, blendMode: 'normal' })
  const document: api.NativeNestedProject = { schemaVersion: 1, root: 'root', compositions: { root: { content, instances: [placed('outer', 'parent')] }, parent: { content, instances: [placed('inner', 'child')] }, child: { content, instances: [] }, other: { content, instances: [] } } }
  nestedComposition.document = document; nestedComposition.activeCompositionId = 'other'; nestedComposition.timeUs = 123456
  nestedComposition.renderedOutputs = { root: 'r', parent: 'p', child: 'c', other: 'o' }
  vi.mocked(api.evaluateNestedFrame).mockResolvedValue({ entries: [] })
  await removeNestedTimeline('child')
  expect(nestedComposition.document).toEqual(document)
  expect(nestedHistory.past).toHaveLength(0)
  vi.mocked(api.evaluateNestedFrame).mockRejectedValueOnce(new Error('native refusal'))
  await removeNestedTimeline('child', true)
  expect(nestedComposition.document).toEqual(document)
  expect(nestedComposition.renderedOutputs).toEqual({ root: 'r', parent: 'p', child: 'c', other: 'o' })
  await removeNestedTimeline('child', true)
  expect(nestedComposition.document?.compositions.parent?.instances).toEqual([])
  expect(nestedComposition.renderedOutputs).toEqual({ other: 'o' })
  expect(nestedComposition.activeCompositionId).toBe('other')
  expect(nestedComposition.timeUs).toBe(123456)
  expect(nestedHistory.past).toHaveLength(1)
  undoNestedComposition()
  expect(nestedComposition.document).toEqual(document)
  expect(nestedComposition.renderedOutputs).toEqual({ root: 'r', parent: 'p', child: 'c', other: 'o' })
})

import { setNestedTimelineVideo } from './nestedComposition.svelte'
it('validates video settings natively, preserves selection/time, and invalidates only the changed timeline and ancestors', async () => {
  const content = buildCompositionWireDocument(createComposition({ width: 16, height: 16, fps: 10, backgroundColor: '#000000' }))
  const placement: api.NativeCompositionInstance = { id: 'placed', timing: { composition: 'child', timelineStartUs: 0, sourceInUs: 0, sourceOutUs: 1_000_000 }, transform: { x: { mode: 'constant', value: 0 }, y: { mode: 'constant', value: 0 }, scaleX: { mode: 'constant', value: 1 }, scaleY: { mode: 'constant', value: 1 }, rotationDegrees: { mode: 'constant', value: 0 }, anchorX: 0.5, anchorY: 0.5 }, opacity: { mode: 'constant', value: 1 }, blendMode: 'normal' }
  const document: api.NativeNestedProject = { schemaVersion: 1, root: 'root', compositions: { root: { content, instances: [placement] }, child: { content, instances: [] }, other: { content, instances: [] } } }
  nestedComposition.document = document; nestedComposition.activeCompositionId = 'child'; nestedComposition.timeUs = 123456
  nestedComposition.renderedOutputs = { root: 'r', child: 'c', other: 'o' }
  vi.mocked(api.evaluateNestedFrame).mockResolvedValue({ entries: [] })
  const settings = { width: 32, height: 24, numerator: 30000, denominator: 1001 }
  await setNestedTimelineVideo('child', settings)
  expect(nestedComposition.document?.compositions.child?.content.canvas).toMatchObject({ width: 32, height: 24, fpsRate: { numerator: 30000, denominator: 1001 } })
  expect(nestedComposition.renderedOutputs).toEqual({ other: 'o' })
  expect(nestedComposition.activeCompositionId).toBe('child')
  expect(nestedComposition.timeUs).toBe(123456)
  expect(nestedHistory.past).toHaveLength(1)
  await setNestedTimelineVideo('child', { ...settings, numerator: 60000, denominator: 2002 })
  expect(nestedHistory.past).toHaveLength(1)
  expect(api.evaluateNestedFrame).toHaveBeenCalledTimes(1)
  undoNestedComposition()
  expect(nestedComposition.document).toEqual(document)
  expect(nestedComposition.renderedOutputs).toEqual({ root: 'r', child: 'c', other: 'o' })
  redoNestedComposition()
  const before = JSON.stringify(nestedComposition.document)
  await setNestedTimelineVideo('child', { ...settings, width: 33 })
  expect(JSON.stringify(nestedComposition.document)).toBe(before)
  vi.mocked(api.evaluateNestedFrame).mockRejectedValueOnce(new Error('native refusal'))
  await setNestedTimelineVideo('child', { ...settings, width: 64 })
  expect(JSON.stringify(nestedComposition.document)).toBe(before)
  expect(nestedHistory.past).toHaveLength(1)
  expect(nestedComposition.renderedOutputs).toEqual({ other: 'o' })
})


import { moveNestedClips } from './nestedComposition.svelte'
it('commits a native transfer with destination focus/cursor and selective render invalidation in one Undo', async () => {
  const content = buildCompositionWireDocument(createComposition({ width: 16, height: 16, fps: 10, backgroundColor: '#000000' }))
  const document: api.NativeNestedProject = { schemaVersion: 1, root: 'root', compositions: { root: { content, instances: [] }, target: { content, instances: [] }, other: { content, instances: [] } } }
  const changed = { ...document, compositions: { ...document.compositions, target: { content, instances: [], durationUs: 3_000_000 } } }
  nestedComposition.document = document; nestedComposition.activeCompositionId = 'root'; nestedComposition.timeUs = 750000; nestedComposition.renderedOutputs = { root: 'r', target: 't', other: 'o' }
  const moves: api.NativeClipMove[] = [{ fromComposition: 'root', fromTrackId: 'v', clipId: 'clip', toComposition: 'target', toTrackId: 'v-target', startUs: 2_000_000 }]
  vi.mocked(api.moveNativeNestedClips).mockResolvedValue({ project: changed, changed: true, affectedCompositions: ['root', 'target'], movedClipIds: ['clip'] })
  await moveNestedClips(moves)
  expect(api.moveNativeNestedClips).toHaveBeenCalledWith(document, moves)
  expect(nestedComposition.document).toEqual(changed)
  expect(nestedComposition.activeCompositionId).toBe('target')
  expect(nestedComposition.timeUs).toBe(2_000_000)
  expect(nestedComposition.renderedOutputs).toEqual({ other: 'o' })
  expect(nestedHistory.past).toHaveLength(1)
  undoNestedComposition()
  expect(nestedComposition.document).toEqual(document)
  expect(nestedComposition.activeCompositionId).toBe('root')
  expect(nestedComposition.timeUs).toBe(750000)
  expect(nestedComposition.renderedOutputs).toEqual({ root: 'r', target: 't', other: 'o' })
  vi.mocked(api.moveNativeNestedClips).mockResolvedValueOnce({ project: document, changed: false, affectedCompositions: [], movedClipIds: [] })
  await moveNestedClips(moves)
  expect(nestedHistory.past).toHaveLength(0)
  expect(nestedHistory.future).toHaveLength(1)
  expect(nestedComposition.activeCompositionId).toBe('root')
  vi.mocked(api.moveNativeNestedClips).mockRejectedValueOnce(new Error('move refusal'))
  await moveNestedClips(moves)
  expect(nestedComposition.document).toEqual(document)
  expect(nestedComposition.renderedOutputs).toEqual({ root: 'r', target: 't', other: 'o' })
  redoNestedComposition()
  expect(nestedComposition.activeCompositionId).toBe('target')
  expect(nestedComposition.timeUs).toBe(2_000_000)
})

it('native track edits refuse failed validation atomically and preserve redo', async () => {
  const { createComposition } = await import('$lib/composition/commands')
  const { buildCompositionWireDocument } = await import('$lib/composition/payload')
  const { editNestedTrack } = await import('./nestedComposition.svelte')
  const document: api.NativeNestedProject = { schemaVersion: 1, root: 'root', compositions: { root: { content: buildCompositionWireDocument(createComposition({ width: 16, height: 16, fps: 10, backgroundColor: '#000000' })), instances: [] } } }
  nestedComposition.document = document; nestedComposition.activeCompositionId = 'root'; nestedComposition.renderedOutputs = { root: 'old', other: 'keep' }
  vi.mocked(api.evaluateNestedFrame).mockResolvedValue({} as never)
  await editNestedTrack('root', { kind: 'add', id: 'new', trackKind: 'audio', name: 'Sound' })
  expect(nestedHistory.past).toHaveLength(1)
  undoNestedComposition()
  expect(nestedHistory.future).toHaveLength(1)
  vi.mocked(api.evaluateNestedFrame).mockRejectedValueOnce(new Error('native refusal'))
  await editNestedTrack('root', { kind: 'add', id: 'rejected', trackKind: 'video', name: 'Rejected' })
  expect(nestedComposition.document).toEqual(document)
  expect(nestedComposition.renderedOutputs).toEqual({ root: 'old', other: 'keep' })
  expect(nestedHistory.past).toHaveLength(0); expect(nestedHistory.future).toHaveLength(1)
  expect(nestedHistory.error).toBe('native refusal')
  redoNestedComposition()
  expect(nestedComposition.document!.compositions.root!.content.tracks.some(track => track.id === 'new')).toBe(true)
})

it('track deletion rebinds external layer effects in one Undo and restores source bindings on redo', async () => {
  const fixture = await import('../../../../backend/tests/fixtures/concat-merge-preview.json')
  const { editNestedTrack } = await import('./nestedComposition.svelte')
  const scenario = fixture.nativeRebinds[0]!
  const document = scenario.before as unknown as api.NativeNestedProject
  nestedComposition.document = document; nestedComposition.activeCompositionId = 'root'; nestedComposition.renderedOutputs = { root: 'old', 'new-timeline': 'keep' }
  vi.mocked(api.evaluateNestedFrame).mockResolvedValue({} as never)
  await editNestedTrack('root', { kind: 'remove', id: 'empty', removeClips: false })
  expect(nestedComposition.document).toEqual(scenario.after)
  expect(nestedHistory.past).toHaveLength(1)
  expect(nestedComposition.renderedOutputs).toEqual({ 'new-timeline': 'keep' })
  undoNestedComposition()
  expect(nestedComposition.document).toEqual(document)
  expect(nestedComposition.renderedOutputs).toEqual({ root: 'old', 'new-timeline': 'keep' })
  redoNestedComposition()
  expect(nestedComposition.document).toEqual(scenario.after)
})

it('native clip deletion is atomic on refusal and restores clip data, outputs and cursor with Undo', async () => {
  const fixture = await import('../../../../backend/tests/fixtures/concat-merge-preview.json')
  const { removeNestedClips } = await import('./nestedComposition.svelte')
  const document = fixture.nativeDeletions[0]!.before as unknown as api.NativeNestedProject
  nestedComposition.document = document; nestedComposition.activeCompositionId = 'root'; nestedComposition.timeUs = 750000
  nestedComposition.renderedOutputs = { root: 'old', 'new-timeline': 'keep' }
  vi.mocked(api.evaluateNestedFrame).mockRejectedValueOnce(new Error('native refusal'))
  await removeNestedClips('root', ['picture', 'sound'])
  expect(nestedComposition.document).toEqual(document); expect(nestedHistory.past).toHaveLength(0)
  expect(nestedComposition.renderedOutputs).toEqual({ root: 'old', 'new-timeline': 'keep' })
  vi.mocked(api.evaluateNestedFrame).mockResolvedValue({} as never)
  await removeNestedClips('root', ['picture', 'sound'])
  expect(nestedComposition.document).toEqual(fixture.nativeDeletions[0]!.after)
  expect(nestedComposition.timeUs).toBe(750000)
  expect(nestedHistory.past).toHaveLength(1)
  undoNestedComposition(); expect(nestedComposition.document).toEqual(document)
  expect(nestedComposition.renderedOutputs).toEqual({ root: 'old', 'new-timeline': 'keep' })
  redoNestedComposition(); expect(nestedComposition.document).toEqual(fixture.nativeDeletions[0]!.after)
})

it('native media insertion registers source and clip in one validated Undo and refuses stale source edits atomically', async () => {
  const fixture = await import('../../../../backend/tests/fixtures/concat-merge-preview.json')
  const { insertNestedMedia } = await import('./nestedComposition.svelte')
  const document = fixture.createdTimeline as unknown as api.NativeNestedProject
  const source = document.compositions.root!.content.sources.source!
  const request = { trackId: 'video-1', clipId: 'inserted', startUs: 2000000, sourceInUs: 1000000, durationUs: 3000000 }
  nestedComposition.document = document; nestedComposition.activeCompositionId = 'new-timeline'; nestedComposition.timeUs = 750000
  nestedComposition.renderedOutputs = { root: 'keep', 'new-timeline': 'old' }
  vi.mocked(api.evaluateNestedFrame).mockRejectedValueOnce(new Error('native refusal'))
  await insertNestedMedia('new-timeline', source, request)
  expect(nestedComposition.document).toEqual(document); expect(nestedHistory.past).toHaveLength(0)
  vi.mocked(api.evaluateNestedFrame).mockResolvedValue({} as never)
  await insertNestedMedia('new-timeline', source, request)
  expect(nestedHistory.past).toHaveLength(1)
  expect(nestedComposition.document!.compositions['new-timeline']!.content.sources.source).toEqual(source)
  expect(nestedComposition.renderedOutputs).toEqual({ root: 'keep' }); expect(nestedComposition.timeUs).toBe(750000)
  undoNestedComposition(); expect(nestedComposition.document).toEqual(document)
  expect(nestedComposition.renderedOutputs).toEqual({ root: 'keep', 'new-timeline': 'old' })
  redoNestedComposition(); expect(nestedComposition.document!.compositions['new-timeline']!.content.tracks[0]!.clips).toHaveLength(1)
})

it('adopts an import as a new unsaved project with complete project-context undo/redo',async()=>{
 const previous:api.NativeNestedProject={schemaVersion:1,root:'old',compositions:{}}
 const imported:api.NativeNestedProject={schemaVersion:1,root:'new',compositions:{}}
 nestedComposition.document=previous;nestedComposition.projectId='saved-old';nestedComposition.revision=7;nestedComposition.name='Old';nestedComposition.activeCompositionId='old';nestedComposition.timeUs=900000;nestedComposition.renderedOutputs={old:'movie'}
 vi.mocked(api.evaluateNestedFrame).mockResolvedValue({entries:[]})
 expect(await adoptImportedConcatProject(imported,'New')).toBe(true);expect(nestedComposition.projectId).toBeNull();expect(nestedComposition.document).toEqual(imported);expect(nestedComposition.renderedOutputs).toEqual({})
 undoNestedComposition();expect(nestedComposition.projectId).toBe('saved-old');expect(nestedComposition.revision).toBe(7);expect(nestedComposition.name).toBe('Old');expect(nestedComposition.document).toEqual(previous);expect(nestedComposition.timeUs).toBe(900000);expect(nestedComposition.renderedOutputs).toEqual({old:'movie'})
 redoNestedComposition();expect(nestedComposition.projectId).toBeNull();expect(nestedComposition.name).toBe('New');expect(nestedComposition.document).toEqual(imported)
})
it('restores the original flat editor when undoing the first import and rejects stale adoption',async()=>{
 const imported:api.NativeNestedProject={schemaVersion:1,root:'new',compositions:{}};vi.mocked(api.evaluateNestedFrame).mockResolvedValue({entries:[]})
 expect(await adoptImportedConcatProject(imported,'New')).toBe(true);undoNestedComposition();expect(nestedComposition.document).toBeNull();redoNestedComposition();expect(nestedComposition.document).toEqual(imported)
 const before=JSON.stringify(nestedComposition.document),count=nestedHistory.past.length
 expect(await adoptImportedConcatProject({...imported,root:'other'},'Other',()=>{throw new Error('stale import')})).toBe(false);expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedHistory.past).toHaveLength(count);expect(nestedHistory.error).toBe('stale import')
})

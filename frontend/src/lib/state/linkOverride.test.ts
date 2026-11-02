import { beforeEach, describe, expect, it } from 'vitest'
import { addClip, addTrack, createComposition } from '../composition/commands.js'
import { createCompositionLink, removeCompositionLink } from '../composition/links.js'
import { normalizeComposition } from '../composition/validation.js'
import { compositionState, resetCompositionForTests, setCompositionStorageForTests, toggleCompositionLinkOverride, selectCompositionClip, toggleSelectedCompositionLink } from './composition/core.svelte.js'
import { compositionCommandState, executeCompositionCommand } from '../composition/commandRegistry.js'
import { setCompositionClipLocked, moveCompositionClip, deleteSelectedCompositionClip, duplicateSelectedCompositionClip } from './composition/timeline.svelte.js'
import { undoComposition } from './composition/transport.svelte.js'
import { agentSession } from '../agent/session.svelte.js'
import { executeAgentTool } from '../agent/runtime.svelte.js'

beforeEach(() => {
  setCompositionStorageForTests(null)
  resetCompositionForTests()
  compositionState.ui.linkOverride = false
  let document = createComposition({ width: 1280, height: 720, fps: 30, backgroundColor: '#000000' })
  for (const id of ['a', 'b']) {
    document = addTrack(document, { id: `track-${id}`, kind: 'text', name: id, locked: false, hidden: false, clips: [] })
    document = addClip(document, `track-${id}`, { id, kind: 'text', timelineStartTicks: 0, durationTicks: 2_000_000, text: id, x: 0, y: 0, opacity: 1, style: { fontSizePx: 48, color: '#FFFFFFFF', align: 'center' } })
  }
  compositionState.document = normalizeComposition(createCompositionLink(document, 'link', ['a', 'b']))
})

describe('session link override', () => {
  it('pastes a linked selection at an exact drag destination with one Undo and rejects sibling collisions atomically', () => {
    selectCompositionClip('track-a', 'a')
    const before = JSON.stringify(compositionState.document)
    const id = duplicateSelectedCompositionClip({ trackId: 'track-a', timelineStartTicks: 3_000_000 })!
    expect(compositionState.document.tracks.map((track) => track.clips[1]!.timelineStartTicks)).toEqual([3_000_000, 3_000_000])
    expect(compositionState.document.links).toHaveLength(2)
    expect(compositionState.ui.selectedClipId).toBe(id)
    expect(compositionState.ui.selectedClipIds).toHaveLength(2)
    expect(compositionState.history.past).toHaveLength(1)
    undoComposition()
    expect(JSON.stringify(compositionState.document)).toBe(before)
    selectCompositionClip('track-a', 'a')
    compositionState.document = addClip(compositionState.document, 'track-b', { ...compositionState.document.tracks[1]!.clips[0]!, id: 'obstacle', timelineStartTicks: 3_000_000 })
    const collisionDocument = JSON.stringify(compositionState.document)
    const depth = compositionState.history.past.length
    expect(() => duplicateSelectedCompositionClip({ trackId: 'track-a', timelineStartTicks: 3_000_000 })).toThrow()
    expect(JSON.stringify(compositionState.document)).toBe(collisionDocument)
    expect(compositionState.history.past).toHaveLength(depth)
  })

  it('restores a checkpoint over later locked content and makes the restore undoable', () => {
    const original = JSON.stringify(compositionState.document)
    const checkpoint = executeAgentTool('agent', 'create_checkpoint', { label: 'Before edits' }) as { id: string }
    executeAgentTool('agent', 'apply_batch', { expectedRevision: agentSession.revision, operations: [{ op: 'update_layer', clipId: 'b', patch: { text: 'changed' } }, { op: 'set_layer_locked', clipId: 'b', locked: true }] })
    const protectedDocument = JSON.stringify(compositionState.document)
    executeAgentTool('agent', 'restore_checkpoint', { id: checkpoint.id })
    expect(JSON.stringify(compositionState.document)).toBe(original)
    undoComposition()
    expect(JSON.stringify(compositionState.document)).toBe(protectedDocument)
  })

  it('protects locked linked content and permits explicit lock-only changes', () => {
    setCompositionClipLocked('b', true)
    selectCompositionClip('track-a', 'a')
    const before = JSON.stringify(compositionState.document)
    const depth = compositionState.history.past.length
    expect(() => moveCompositionClip('a', 'track-a', 1_000_000, false)).toThrow('locked')
    expect(() => executeAgentTool('agent', 'apply_batch', { expectedRevision: agentSession.revision, operations: [{ op: 'update_layer', clipId: 'b', patch: { text: 'changed' } }] })).toThrow('locked')
    expect(() => executeAgentTool('agent', 'apply_batch', { expectedRevision: agentSession.revision, operations: [{ op: 'remove_link', id: 'link' }] })).toThrow('locked')
    expect(JSON.stringify(compositionState.document)).toBe(before)
    expect(compositionState.history.past).toHaveLength(depth)
    executeAgentTool('agent', 'apply_batch', { expectedRevision: agentSession.revision, operations: [{ op: 'set_layer_locked', clipId: 'b', locked: false }] })
    expect(compositionState.document.tracks[1]!.clips[0]!.locked).toBe(false)
    undoComposition()
    expect(compositionState.document.tracks[1]!.clips[0]!.locked).toBe(true)
    executeAgentTool('agent', 'apply_batch', { expectedRevision: agentSession.revision, operations: [
      { op: 'set_layer_locked', clipId: 'b', locked: false },
      { op: 'update_layer', clipId: 'b', patch: { text: 'unlocked edit' } },
    ] })
    expect(compositionState.document.tracks[1]!.clips[0]).toMatchObject({ locked: false, text: 'unlocked edit' })
    undoComposition()
    expect(compositionState.document.tracks[1]!.clips[0]).toMatchObject({ locked: true, text: 'b' })
  })

  it('moves an arbitrary selection with one delta and one Undo', () => {
    compositionState.document = removeCompositionLink(compositionState.document, 'link')
    selectCompositionClip('track-a', 'a')
    selectCompositionClip('track-b', 'b', { toggle: true })
    moveCompositionClip('b', 'track-b', 1_000_000, false)
    expect(compositionState.document.tracks.map((track) => track.clips[0]!.timelineStartTicks)).toEqual([1_000_000, 1_000_000])
    expect(compositionState.ui.selectedClipIds).toEqual(['a', 'b'])
    expect(compositionState.history.past).toHaveLength(1)
    undoComposition()
    expect(compositionState.document.tracks.map((track) => track.clips[0]!.timelineStartTicks)).toEqual([0, 0])
  })
  it('duplicates an arbitrary unlinked cross-track selection and selects all copies in one Undo', () => {
    compositionState.document = removeCompositionLink(compositionState.document, 'link')
    selectCompositionClip('track-a', 'a')
    selectCompositionClip('track-b', 'b', { toggle: true })
    moveCompositionClip('b', 'track-b', 0, false)
    expect(compositionState.ui.selectedClipIds).toEqual(['a', 'b'])
    duplicateSelectedCompositionClip()
    expect(compositionState.document.tracks.map((track) => track.clips.length)).toEqual([2, 2])
    expect(compositionState.document.links).toEqual([])
    expect(compositionState.ui.selectedClipIds).toHaveLength(2)
    expect(compositionState.history.past).toHaveLength(1)
    undoComposition()
    expect(compositionState.document.tracks.map((track) => track.clips.length)).toEqual([1, 1])
  })
  it('toggles the common link and recreates a link from selected unlinked clips with one Undo per action', () => {
    selectCompositionClip('track-a', 'a')
    expect(compositionCommandState('composition.linkSelection').enabled).toBe(true)
    toggleSelectedCompositionLink()
    expect(compositionState.document.links).toEqual([])
    expect(compositionState.history.past).toHaveLength(1)
    toggleSelectedCompositionLink()
    expect(compositionState.document.links?.[0]?.memberIds).toEqual(['a', 'b'])
    expect(compositionState.history.past).toHaveLength(2)
    undoComposition()
    expect(compositionState.document.links).toEqual([])
  })
  it('refuses unlinking when an unselected member is locked', () => {
    compositionState.document = { ...compositionState.document, tracks: compositionState.document.tracks.map((track) => ({ ...track, locked: track.id === 'track-b' })) }
    selectCompositionClip('track-a', 'a', { single: true })
    expect(compositionCommandState('composition.linkSelection')).toMatchObject({ enabled: false, disabledReason: 'Дорожка участника заблокирована' })
    expect(executeCompositionCommand('composition.linkSelection')).toBe(false)
    expect(() => toggleSelectedCompositionLink()).toThrow('заблокирована')
    expect(compositionState.history.past).toHaveLength(0)
    expect(compositionState.document.links).toHaveLength(1)
  })
  it('selects and toggles a whole link, while explicit single selection escapes UI fan-out', () => {
    selectCompositionClip('track-a', 'a')
    expect(compositionState.ui.selectedClipIds).toEqual(['a', 'b'])
    expect(selectCompositionClip('track-b', 'b', { toggle: true })).toBe(false)
    expect(compositionState.ui.selectedClipIds).toEqual([])
    selectCompositionClip('track-a', 'a', { single: true })
    moveCompositionClip('a', 'track-a', 1_000_000, false)
    expect(compositionState.document.tracks.map((track) => track.clips[0]!.timelineStartTicks)).toEqual([1_000_000, 0])
  })
  it('deletes exactly the selection in one Undo and atomically rejects a locked partner', () => {
    selectCompositionClip('track-a', 'a')
    deleteSelectedCompositionClip()
    expect(compositionState.document.tracks.map((track) => track.clips.length)).toEqual([0, 0])
    expect(compositionState.history.past).toHaveLength(1)
    undoComposition()
    selectCompositionClip('track-a', 'a')
    compositionState.document = { ...compositionState.document, tracks: compositionState.document.tracks.map((track) => ({ ...track, locked: track.id === 'track-b' })) }
    const snapshot = JSON.stringify(compositionState.document)
    expect(compositionCommandState('composition.nudgeNext').enabled).toBe(false)
    expect(compositionCommandState('composition.duplicate').enabled).toBe(false)
    expect(() => deleteSelectedCompositionClip()).toThrow('locked')
    expect(JSON.stringify(compositionState.document)).toBe(snapshot)
    selectCompositionClip('track-a', 'a', { single: true })
    expect(compositionCommandState('composition.nudgeNext').enabled).toBe(true)
    deleteSelectedCompositionClip()
    expect(compositionState.document.tracks[1]!.clips).toHaveLength(1)
    expect(compositionState.document.links).toEqual([])
  })
  it('creates no history or revision and is retained across Undo without entering the project', () => {
    const revision = agentSession.revision
    const snapshot = JSON.stringify(compositionState.document)
    toggleCompositionLinkOverride()
    expect(agentSession.revision).toBe(revision)
    expect(compositionState.history.past).toHaveLength(0)
    expect(JSON.stringify(compositionState.document)).toBe(snapshot)
    moveCompositionClip('a', 'track-a', 1_000_000, false)
    expect(compositionState.document.tracks.map((track) => track.clips[0]!.timelineStartTicks)).toEqual([1_000_000, 0])
    undoComposition()
    expect(compositionState.ui.linkOverride).toBe(true)
    expect(JSON.stringify(compositionState.document)).toBe(snapshot)
  })
  it('does not affect an agent structural operation', () => {
    toggleCompositionLinkOverride()
    executeAgentTool('test-agent', 'apply_batch', { expectedRevision: agentSession.revision, operations: [{ op: 'move_layer', clipId: 'a', trackId: 'track-a', startTicks: 1_000_000 }] })
    expect(compositionState.document.tracks.map((track) => track.clips[0]!.timelineStartTicks)).toEqual([1_000_000, 1_000_000])
    expect(compositionState.ui.linkOverride).toBe(true)
  })
})

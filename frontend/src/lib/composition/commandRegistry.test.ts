import { addColorToComposition } from '$lib/state/composition/timeline.svelte.js'
import { commitDocument } from '$lib/state/composition/core.svelte.js'
import { setTextColorKey } from './colorCommands.js'
import { beforeEach, describe, expect, it } from 'vitest'
import { addMediaInfoToComposition, compositionState, resetCompositionForTests, setCompositionStorageForTests, updateCompositionCanvas, undoComposition, selectCompositionClip, setCompositionPlayhead, setCompositionAutomationKeyframe, selectCompositionKeyframe, updateCompositionClipOpacity, deleteSelectedCompositionClip } from '$lib/state/composition.svelte.js'
import { addClip, addTrack, setClipLocked, duplicateClip } from './commands.js'
import { compositionCommandState, executeCompositionCommand } from './commandRegistry.js'
import { COMPOSITION_TIME_BASE, type CompositionClip } from './types.js'

describe('composition command registry', () => {
  beforeEach(() => { setCompositionStorageForTests(null); resetCompositionForTests() })

  it('steps the playhead and nudges a linked AV selection on exact fractional boundaries', () => {
    const clipId = addMediaInfoToComposition({ id: 'fractional-source', url: '/fractional.mp4', filename: 'fractional.mp4', mediaType: 'video', duration: 60, width: 640, height: 360, acodec: 'aac' }, { pairAudio: true })
    updateCompositionCanvas({ fps: 30000 / 1001, fpsRate: { numerator: 30000, denominator: 1001 } })
    const depth = compositionState.history.past.length
    for (let index = 0; index < 10; index++) executeCompositionCommand('composition.frameNext')
    expect(compositionState.transport.playheadTicks).toBe(333_667)
    expect(compositionState.history.past).toHaveLength(depth)
    executeCompositionCommand('composition.framePrevious')
    expect(compositionState.transport.playheadTicks).toBe(300_300)
    expect(compositionState.ui.selectedClipId).toBe(clipId)
    executeCompositionCommand('composition.nudgeNext')
    executeCompositionCommand('composition.nudgeNext')
    const starts = () => compositionState.document.tracks.flatMap((track) => track.clips.map((clip) => clip.timelineStartTicks))
    expect(starts()).toEqual([66_733, 66_750])
    expect(compositionState.ui.selectedClipIds).toHaveLength(2)
    expect(compositionState.history.past).toHaveLength(depth + 2)
    undoComposition()
    expect(starts()).toEqual([33_367, 33_375])
    undoComposition()
    expect(starts()).toEqual([0, 0])
    expect(compositionState.transport.playheadTicks).toBe(300_300)
  })

  it('shares availability and execution across every UI entry point', () => {
    expect(compositionCommandState('composition.split')).toEqual({ enabled: false, disabledReason: 'Сначала выберите клип' })
    addMediaInfoToComposition({ id: 'source', url: '/source.mp4', filename: 'source.mp4', mediaType: 'video', duration: 4, width: 640, height: 360 })
    compositionState.document = addTrack(compositionState.document, { id: 'track', kind: 'video', name: 'Video', clips: [], muted: false, hidden: false, locked: false })
    compositionState.document = addClip(compositionState.document, 'track', {
      id: 'clip', kind: 'video', sourceId: 'source', timelineStartTicks: 0,
      sourceInTicks: 0, sourceOutTicks: 4 * COMPOSITION_TIME_BASE,
      transform: { x: 0, y: 0, width: 640, height: 360, fit: 'contain' }, opacity: 1, sourceAudioEnabled: false, audioGain: 1,
    })
    selectCompositionClip('track', 'clip')
    setCompositionPlayhead(2 * COMPOSITION_TIME_BASE)
    expect(compositionCommandState('composition.split')).toEqual({ enabled: true })
    expect(executeCompositionCommand('composition.split')).toBe(true)
    expect(compositionState.document.tracks.find((track) => track.id === 'track')!.clips).toHaveLength(2)
    expect(executeCompositionCommand('composition.undo')).toBe(true)
    expect(compositionState.document.tracks.find((track) => track.id === 'track')!.clips).toHaveLength(1)
  })
})

it('routes key copy and paste through the global command boundary with one Undo', () => {
  resetCompositionForTests()
  expect(compositionCommandState('composition.copy').enabled).toBe(false)
  expect(compositionCommandState('composition.paste').enabled).toBe(false)
  const id = addMediaInfoToComposition({ id: 'copy-source', url: '/copy.mp4', filename: 'copy.mp4', mediaType: 'video', duration: 4, width: 640, height: 360 }, { pairAudio: false })
  setCompositionAutomationKeyframe({ kind: 'visual', clipId: id }, 'opacity', 0.5, 0)
  selectCompositionKeyframe({ kind: 'visual', clipId: id }, 'opacity', 0)
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  expect(executeCompositionCommand('composition.copy')).toBe(true)
  setCompositionPlayhead(1_000_000)
  expect(compositionCommandState('composition.paste').enabled).toBe(true)
  expect(executeCompositionCommand('composition.paste')).toBe(true)
  expect(compositionState.history.past).toHaveLength(depth + 1)
  expect(compositionState.ui.selectedKeyframes).toMatchObject([{ property: 'opacity', ticks: [1_000_000] }])
  undoComposition()
  expect(JSON.stringify(compositionState.document)).toBe(before)
})

it('uses one slot for key and clip copies and pastes only the primary clip at the playhead', () => {
  resetCompositionForTests()
  const id = addMediaInfoToComposition({ id: 'clip-slot-source', url: '/clip.mp4', filename: 'clip.mp4', mediaType: 'video', duration: 2, width: 640, height: 360 }, { pairAudio: false })
  setCompositionAutomationKeyframe({ kind: 'visual', clipId: id }, 'opacity', 0.5, 0)
  selectCompositionKeyframe({ kind: 'visual', clipId: id }, 'opacity', 0)
  executeCompositionCommand('composition.copy')
  expect(compositionState.ui.clipboard?.kind).toBe('keyframes')
  compositionState.ui.selectedKeyframes = []
  executeCompositionCommand('composition.copy')
  expect(compositionState.ui.clipboard).toMatchObject({ kind: 'layer', clipId: id, insertSnapshot: { clipIds: [id] } })
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  setCompositionPlayhead(2_000_000)
  expect(executeCompositionCommand('composition.paste')).toBe(true)
  const copies = compositionState.document.tracks.flatMap((track): readonly CompositionClip[] => track.clips)
  expect(copies).toHaveLength(2)
  expect(copies[1]).toMatchObject({ timelineStartTicks: 2_000_000, animation: { opacity: { track: { keyframes: [{ value: 0.5 }] } } } })
  expect(copies[1]!.id).not.toBe(id)
  expect(compositionState.ui.selectedClipId).toBe(copies[1]!.id)
  expect(compositionState.history.past).toHaveLength(depth + 1)
  undoComposition()
  expect(JSON.stringify(compositionState.document)).toBe(before)
})

it('creates a free lane when clipboard paste overlaps and removes it with one Undo', () => {
  resetCompositionForTests()
  const id = addMediaInfoToComposition({ id: 'overlap-copy', url: '/copy.mp4', filename: 'copy.mp4', mediaType: 'video', duration: 2, width: 640, height: 360 }, { pairAudio: false })
  executeCompositionCommand('composition.copy')
  const before = JSON.stringify(compositionState.document)
  const count = compositionState.document.tracks.length
  const depth = compositionState.history.past.length
  executeCompositionCommand('composition.paste')
  expect(compositionState.document.tracks).toHaveLength(count + 1)
  expect(compositionState.ui.selectedClipId).not.toBe(id)
  expect(compositionState.document.tracks[0]!.clips[0]).toMatchObject({ timelineStartTicks: 0 })
  expect(compositionState.history.past).toHaveLength(depth + 1)
  undoComposition()
  expect(JSON.stringify(compositionState.document)).toBe(before)
})

it('keeps a live clip reference and declines paste after its source is deleted', () => {
  resetCompositionForTests()
  const id = addMediaInfoToComposition({ id: 'live-copy', url: '/copy.mp4', filename: 'copy.mp4', mediaType: 'video', duration: 2, width: 640, height: 360 }, { pairAudio: false })
  executeCompositionCommand('composition.copy')
  updateCompositionClipOpacity(id, 0.4)
  setCompositionPlayhead(2_000_000)
  executeCompositionCommand('composition.paste')
  const pasted = compositionState.document.tracks.flatMap((track): readonly CompositionClip[] => track.clips).find((clip) => clip.id === compositionState.ui.selectedClipId)!
  expect(pasted).toMatchObject({ opacity: 0.4 })
  undoComposition()
  const sourceTrack = compositionState.document.tracks.find((track) => track.clips.some((clip) => clip.id === id))!
  selectCompositionClip(sourceTrack.id, id, { single: true })
  deleteSelectedCompositionClip()
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  expect(compositionCommandState('composition.paste').enabled).toBe(false)
  expect(executeCompositionCommand('composition.paste')).toBe(false)
  expect(JSON.stringify(compositionState.document)).toBe(before)
  expect(compositionState.history.past).toHaveLength(depth)
})

it('clipboard reads a locked source on a locked lane and leaves it untouched', () => {
  resetCompositionForTests()
  const id = addMediaInfoToComposition({ id: 'locked-copy', url: '/copy.mp4', filename: 'copy.mp4', mediaType: 'video', duration: 2, width: 640, height: 360 }, { pairAudio: false })
  executeCompositionCommand('composition.copy')
  compositionState.document = setClipLocked(compositionState.document, id, true)
  compositionState.document = { ...compositionState.document, tracks: compositionState.document.tracks.map((track) => ({ ...track, locked: true })) }
  expect(() => duplicateClip(compositionState.document, id, { id: 'forbidden-copy', timelineStartTicks: 2_000_000, escapeLink: true, allowLockedSource: true })).toThrow('locked')
  const originalTrack = JSON.stringify(compositionState.document.tracks[0])
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  expect(compositionCommandState('composition.paste').enabled).toBe(true)
  executeCompositionCommand('composition.paste')
  expect(compositionState.history.past).toHaveLength(depth + 1)
  expect(JSON.stringify(compositionState.document.tracks.at(-1))).toBe(originalTrack)
  expect(compositionState.document.tracks[0]!.clips[0]).toMatchObject({ locked: true })
  undoComposition()
  expect(JSON.stringify(compositionState.document)).toBe(before)
})

it('admits solid color-key copy and paste through the command registry', () => {
  const id = addColorToComposition()
  commitDocument(setTextColorKey(compositionState.document,id,500000,'#ff000080'))
  compositionState.ui.selectedColorKeys = [{ clipId: id,tick: 500000 }]
  expect(executeCompositionCommand('composition.copy')).toBe(true)
  setCompositionPlayhead(1000000)
  expect(compositionCommandState('composition.paste').enabled).toBe(true)
  expect(executeCompositionCommand('composition.paste')).toBe(true)
  expect(compositionState.ui.selectedColorKeys).toEqual([{ clipId: id,tick: 1000000 }])
  undoComposition()
  const colorTrack = compositionState.document.tracks.find(track => track.kind === 'color')!
  expect(colorTrack.clips[0]).toMatchObject({ colorAnimation: { track: { keyframes: [{ tick: 0 },{ tick: 500000 }] } } })
})

it('routes Paste Insert for copied AV snapshots with one Undo and end playhead',()=>{
 resetCompositionForTests()
 expect(compositionCommandState('composition.pasteInsert').enabled).toBe(false)
 addMediaInfoToComposition({id:'insert-av',url:'/av.mp4',filename:'av.mp4',mediaType:'video',duration:4,width:640,height:360,acodec:'aac'},{pairAudio:true})
 expect(compositionState.ui.selectedClipIds.length).toBe(2)
 expect(executeCompositionCommand('composition.copy')).toBe(true)
 // Copy is a snapshot: subsequent source-clip edits must not rewrite it.
 const originalIds=[...compositionState.ui.selectedClipIds]
 updateCompositionClipOpacity(originalIds[0]!,0.25)
 setCompositionPlayhead(1_000_000)
 const before=JSON.stringify(compositionState.document)
 const depth=compositionState.history.past.length
 expect(compositionCommandState('composition.pasteInsert').enabled).toBe(true)
 expect(executeCompositionCommand('composition.pasteInsert')).toBe(true)
 expect(compositionState.history.past.length).toBe(depth+1)
 expect(compositionState.ui.selectedClipIds.length).toBe(2)
 expect(compositionState.transport.playheadTicks).toBe(5_000_000)
 const inserted=compositionState.document.tracks.flatMap<CompositionClip>(track=>track.clips).filter(clip=>compositionState.ui.selectedClipIds.includes(clip.id))
 expect(inserted.map(clip=>clip.timelineStartTicks)).toEqual([1_000_000,1_000_000])
 expect(inserted.find(clip=>clip.kind==='video')).toMatchObject({opacity:1})
 expect(compositionState.document.links?.some(link=>link.memberIds.length===2&&link.memberIds.every(id=>compositionState.ui.selectedClipIds.includes(id)))).toBe(true)
 undoComposition()
 expect(JSON.stringify(compositionState.document)).toBe(before)
})

it('splits every lane atomically with linked AV, one Undo, and no edit at boundaries', () => {
  setCompositionStorageForTests(null); resetCompositionForTests()
  addMediaInfoToComposition({ id: 'all-source', url: '/all.mp4', filename: 'all.mp4', mediaType: 'video', duration: 4, width: 640, height: 360, acodec: 'aac' }, { pairAudio: true })
  const videoTrack = compositionState.document.tracks.find(t => t.kind === 'video')!
  const video = videoTrack.clips[0]!
  compositionState.document = addTrack(compositionState.document, { ...videoTrack, id: 'independent-overlay', clips: [] })
  compositionState.document = addClip(compositionState.document, 'independent-overlay', { ...video, id: 'independent-picture' })
  compositionState.ui.selectedClipId = null
  compositionState.ui.selectedClipIds = []
  const original = compositionState.document
  const depth = compositionState.history.past.length
  setCompositionPlayhead(2 * COMPOSITION_TIME_BASE)
  expect(compositionCommandState('composition.splitAll').enabled).toBe(true)
  expect(executeCompositionCommand('composition.splitAll')).toBe(true)
  expect(compositionState.document.tracks.flatMap<CompositionClip>(t => t.clips)).toHaveLength(6)
  expect(compositionState.document.links).toHaveLength(2)
  expect(compositionState.history.past).toHaveLength(depth + 1)
  expect(compositionCommandState('composition.splitAll').enabled).toBe(false)
  undoComposition()
  expect(compositionState.document).toEqual(original)
  const audio = compositionState.document.tracks.find(t => t.kind === 'audio')!
  compositionState.document = { ...compositionState.document, tracks: compositionState.document.tracks.map(t => t.id === audio.id ? { ...t, locked: true } : t) }
  const locked = compositionState.document
  expect(compositionCommandState('composition.splitAll').enabled).toBe(false)
  expect(executeCompositionCommand('composition.splitAll')).toBe(false)
  expect(compositionState.document).toBe(locked)
  expect(compositionState.history.past).toHaveLength(depth)
})

it('joins a split AV selection with one history step and exact Undo/Redo', () => {
  setCompositionStorageForTests(null); resetCompositionForTests()
  addMediaInfoToComposition({ id: 'join-source', url: '/join.mp4', filename: 'join.mp4', mediaType: 'video', duration: 4, width: 640, height: 360, acodec: 'aac' }, { pairAudio: true })
  setCompositionPlayhead(2 * COMPOSITION_TIME_BASE)
  executeCompositionCommand('composition.splitAll')
  const split = JSON.stringify(compositionState.document)
  compositionState.ui.selectedClipIds = compositionState.document.tracks.flatMap(t => t.clips.map(c => c.id))
  const depth = compositionState.history.past.length
  expect(compositionCommandState('composition.merge').enabled).toBe(true)
  expect(executeCompositionCommand('composition.merge')).toBe(true)
  expect(compositionState.history.past).toHaveLength(depth + 1)
  expect(compositionState.document.tracks.map(t => t.clips.length)).toEqual([1, 1])
  const joined = JSON.stringify(compositionState.document)
  expect(executeCompositionCommand('composition.undo')).toBe(true)
  expect(JSON.stringify(compositionState.document)).toBe(split)
  expect(executeCompositionCommand('composition.redo')).toBe(true)
  expect(JSON.stringify(compositionState.document)).toBe(joined)
})

it('inserts a held picture via the shared command with one Undo', () => {
  resetCompositionForTests()
  expect(compositionCommandState('composition.freezeInsert').enabled).toBe(false)
  addMediaInfoToComposition({ id: 'hold-source', url: '/hold.mp4', filename: 'hold.mp4', mediaType: 'video', duration: 4, width: 640, height: 360, acodec: 'aac' }, { pairAudio: true })
  setCompositionPlayhead(1_500_000)
  const before = JSON.stringify(compositionState.document), depth = compositionState.history.past.length
  expect(compositionCommandState('composition.freezeInsert').enabled).toBe(true)
  expect(executeCompositionCommand('composition.freezeInsert')).toBe(true)
  expect(compositionState.history.past).toHaveLength(depth + 1)
  const held = compositionState.document.tracks.flatMap((t): readonly CompositionClip[] => t.clips).find(c => c.id === compositionState.ui.selectedClipId)
  expect(held).toMatchObject({ kind: 'video', timelineStartTicks: 1_500_000, timelineDurationTicks: 1_000_000, playbackMode: { mode: 'freeze' }, sourceAudioEnabled: false })
  undoComposition()
  expect(JSON.stringify(compositionState.document)).toBe(before)
})

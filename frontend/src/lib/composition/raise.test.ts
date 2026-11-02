import { describe, expect, it } from 'vitest'
import { addClip, addTrack, createComposition, moveClipsToNewTrack, registerSource, setClipLocked, upsertTransition } from './commands'
import { createCompositionLink } from './links'
import { compositionState, raiseSelectedCompositionClips, resetCompositionForTests, undoComposition } from '../state/composition.svelte'
import { evaluateAgentBatch } from '../agent/operations'

function fixture() {
  let document = createComposition({ width: 1280, height: 720, fps: 30, backgroundColor: '#000000' })
  for (const [trackId, id, start] of [['first', 'a', 11], ['second', 'b', 1_000_017], ['second', 'c', 2_000_003]] as const) {
    if (!document.tracks.some((track) => track.id === trackId)) document = addTrack(document, { id: trackId, kind: 'text', name: trackId, locked: false, hidden: false, clips: [] })
    document = addClip(document, trackId, { id, kind: 'text', timelineStartTicks: start, durationTicks: 100_001, text: id, x: 0, y: 0, opacity: 1, style: { fontSizePx: 48, color: '#FFFFFFFF', align: 'center' } })
  }
  return createCompositionLink(document, 'link', ['b', 'c'])
}

describe('raising exact subjects to one new typed lane', () => {
  it('raises a complete transition pair, retains untouched transitions and restores the source lane with Undo', () => {
    let document = createComposition({ width: 1280, height: 720, fps: 30, backgroundColor: '#000000' })
    document = registerSource(document, { id: 'video-source', kind: 'video', durationTicks: 20_000_000, width: 1280, height: 720, hasAudio: false })
    document = addTrack(document, { id: 'video', kind: 'video', name: 'Video', muted: false, hidden: false, locked: false, clips: [] })
    for (const [id, start, sourceIn] of [['a', 0, 0], ['b', 4_000_000, 2_000_000], ['c', 10_000_000, 0], ['d', 14_000_000, 2_000_000]] as const) document = addClip(document, 'video', { id, kind: 'video', timelineStartTicks: start, sourceId: 'video-source', sourceInTicks: sourceIn, sourceOutTicks: sourceIn + 4_000_000, speed: 1, transform: { x: 0, y: 0, width: 1280, height: 720, fit: 'contain' }, opacity: 1, sourceAudioEnabled: false, audioGain: 1 })
    const transition = { id: 'moving', fromClipId: 'a', toClipId: 'b', durationTicks: 1_000_000, kind: 'dissolve' as const }
    const untouched = { ...transition, id: 'untouched', fromClipId: 'c', toClipId: 'd' }
    document = upsertTransition(upsertTransition(document, 'video', transition), 'video', untouched)
    const before = JSON.stringify(document)
    const raised = moveClipsToNewTrack(document, ['b', 'a'], 'fresh', { clipId: 'a', startTicks: 2_000_000 })
    expect(raised.tracks[0]).toMatchObject({ transitions: [transition], clips: [{ id: 'a', timelineStartTicks: 2_000_000 }, { id: 'b', timelineStartTicks: 6_000_000 }] })
    expect(raised.tracks[1]).toMatchObject({ transitions: [untouched], clips: [{ id: 'c' }, { id: 'd' }] })
    expect(() => moveClipsToNewTrack(document, ['a'], 'fresh')).toThrow('together')
    const all = moveClipsToNewTrack(document, ['a', 'b', 'c', 'd'], 'fresh')
    expect(all.tracks).toHaveLength(1)
    expect(all.tracks[0]).toMatchObject({ transitions: [transition, untouched] })
    expect(JSON.stringify(document)).toBe(before)
    resetCompositionForTests()
    try {
      compositionState.document = document
      compositionState.ui.selectedClipIds = ['a', 'b']
      compositionState.ui.selectedClipId = 'a'
      compositionState.ui.selectedTrackId = 'video'
      const depth = compositionState.history.past.length
      raiseSelectedCompositionClips({ clipId: 'a', startTicks: 2_000_000 })
      expect(compositionState.history.past).toHaveLength(depth + 1)
      expect(compositionState.document.tracks[0]).toMatchObject({ transitions: [transition] })
      undoComposition()
      expect(JSON.stringify(compositionState.document)).toBe(before)
    } finally { resetCompositionForTests() }
  })

  it('snaps the anchor before shifting all endpoints and floors the whole set at zero', () => {
    const document = fixture()
    const landed = moveClipsToNewTrack(document, ['b', 'a'], 'fresh', { clipId: 'a', startTicks: 2_016_000 })
    expect(landed.tracks[0]!.clips.map((clip) => [clip.id, clip.timelineStartTicks])).toEqual([['a', 2_000_000], ['b', 3_000_000]])
    const floored = moveClipsToNewTrack(document, ['a', 'b'], 'fresh', { clipId: 'b', startTicks: 0 })
    expect(floored.tracks[0]!.clips.map((clip) => clip.timelineStartTicks)).toEqual([0, 1_000_000])
    expect(floored.tracks[0]!.clips.map((clip) => clip.kind === 'text' && clip.durationTicks)).toEqual([100_000, 100_000])
    const before = JSON.stringify(document)
    expect(() => moveClipsToNewTrack(document, ['a'], 'fresh', { clipId: 'b', startTicks: 0 })).toThrow('anchor')
    expect(() => moveClipsToNewTrack(document, ['a'], 'fresh', { clipId: 'a', startTicks: NaN })).toThrow('safe')
    expect(JSON.stringify(document)).toBe(before)
  })

  it('preserves exact moments and links, sorts subjects and removes only emptied source lanes', () => {
    const document = fixture()
    const before = JSON.stringify(document)
    const next = moveClipsToNewTrack(document, ['b', 'a'], 'fresh')
    expect(next.tracks.map((track) => track.id)).toEqual(['fresh', 'second'])
    expect(next.tracks[0]!.clips).toEqual([document.tracks[0]!.clips[0], document.tracks[1]!.clips[0]])
    expect(next.tracks[1]!.clips.map((clip) => clip.id)).toEqual(['c'])
    expect(next.links).toEqual(document.links)
    expect(JSON.stringify(document)).toBe(before)
  })

  it('refuses locked participants, duplicate subjects, occupied ids and mixed kinds atomically', () => {
    let document = fixture()
    document = addTrack(document, { id: 'empty-audio', kind: 'audio', name: 'Audio', muted: false, locked: false, clips: [] })
    document = registerSource(document, { id: 'sound', kind: 'audio', durationTicks: 1_000_000, width: 0, height: 0, hasAudio: true })
    document = addClip(document, 'empty-audio', { id: 'sound-clip', kind: 'audio', sourceId: 'sound', timelineStartTicks: 0, sourceInTicks: 0, sourceOutTicks: 100_000, speed: 1, gain: 1, pan: 0 })
    expect(moveClipsToNewTrack(document, ['sound-clip'], 'sound-raised', { clipId: 'sound-clip', startTicks: 33_367 }).tracks[0]!.clips[0]!.timelineStartTicks).toBe(33_375)
    const before = JSON.stringify(document)
    expect(() => moveClipsToNewTrack(document, ['a', 'sound-clip'], 'fresh')).toThrow('Mixed')
    const offGrid = { ...document, tracks: document.tracks.map((track) => track.kind === 'audio' ? { ...track, clips: track.clips.map((clip) => ({ ...clip, sourceOutTicks: 100_001 })) } : track) }
    expect(moveClipsToNewTrack(offGrid, ['sound-clip'], 'fresh', { clipId: 'sound-clip', startTicks: 33_367 }).tracks[0]!.clips[0]).toMatchObject({ timelineStartTicks: 33_375, timelineDurationTicks: 100_000, sourceOutTicks: 100_001 })
    expect(() => moveClipsToNewTrack(setClipLocked(document, 'b', true), ['a', 'b'], 'fresh')).toThrow('locked')
    expect(() => moveClipsToNewTrack(document, ['a', 'a'], 'fresh')).toThrow('distinct')
    expect(() => moveClipsToNewTrack(document, ['a'], 'second')).toThrow('already exists')
    const mixed = addTrack(document, { id: 'image', kind: 'image', name: 'Image', hidden: false, locked: false, clips: [] })
    expect(moveClipsToNewTrack(mixed, ['a'], 'fresh').tracks.some((track) => track.id === 'empty-audio')).toBe(true)
    expect(JSON.stringify(document)).toBe(before)
  })

  it('rolls back a failed later agent operation and keeps the input immutable', () => {
    const document = fixture()
    const before = JSON.stringify(document)
    const operation = { op: 'move_layers_to_new_track' as const, clipIds: ['a', 'b'], trackId: 'fresh' }
    expect(evaluateAgentBatch(document, [operation]).tracks[0]!.id).toBe('fresh')
    expect(evaluateAgentBatch(document, [{ ...operation, anchor: { clipId: 'a', startTicks: 2_016_000 } }]).tracks[0]!.clips[0]!.timelineStartTicks).toBe(2_000_000)
    expect(() => evaluateAgentBatch(document, [operation, { op: 'delete_layer', clipId: 'missing' }])).toThrow()
    expect(JSON.stringify(document)).toBe(before)
  })
})

import { describe, expect, it } from 'vitest'
import { addClip, addTrack, createComposition, moveClip, moveClips, moveClipPlacements, moveClipsToNewTrack, pasteClips, registerSource, splitClip, trimClip } from './commands'
import { evaluateAgentBatch } from '../agent/operations'
import { pairImportedVideoAudio } from './importPair'
import { clipDurationTicks, clipEndTicks, type CompositionClip } from './types'
import { buildCompositionRenderRequest } from './payload'
import { normalizeComposition, validateComposition } from './validation'
import { compositionState, resetCompositionForTests, updateCompositionClipSpeed, undoComposition } from '../state/composition.svelte'

function fixture() {
  let document = createComposition({ width: 1280, height: 720, fps: 30000 / 1001, fpsRate: { numerator: 30000, denominator: 1001 }, backgroundColor: '#000000' })
  document = registerSource(document, { id: 'source', kind: 'video', durationTicks: 2_000_000, width: 1280, height: 720, hasAudio: true })
  document = addTrack(document, { id: 'video', kind: 'video', name: 'Video', hidden: false, muted: false, locked: false, clips: [] })
  return addClip(document, 'video', { id: 'clip', kind: 'video', timelineStartTicks: 0, sourceId: 'source', sourceInTicks: 0, sourceOutTicks: 33_367, speed: 1, transform: { x: 0, y: 0, width: 1280, height: 720, fit: 'contain' }, opacity: 1, sourceAudioEnabled: true, audioGain: 1 })
}

describe('authored media extent', () => {
  it('reconciles the explicit mover at zero delta while keeping link partners and ripple nonmovers verbatim', () => {
    const base = fixture()
    const document = { ...base, tracks: base.tracks.map((track) => track.kind === 'video' ? { ...track, clips: track.clips.map((clip) => ({ ...clip, sourceOutTicks: 33_366 })) } : track) }
    const paired = pairImportedVideoAudio(document, 'clip', { audioTrackId: 'audio', audioClipId: 'sound', linkId: 'pair' })
    const moved = moveClip(paired, 'clip', 'video', 0)
    expect(clipDurationTicks(moved.tracks[0]!.clips[0]!)).toBe(33_367)
    expect(moved.tracks[1]!.clips[0]).toEqual(paired.tracks[1]!.clips[0])
    expect(moveClip(moved, 'clip', 'video', 0)).toEqual(moved)
    const exact = moveClips(paired, ['clip', 'sound'], 'video', 0)
    expect(clipDurationTicks(exact.tracks[1]!.clips[0]!)).toBe(33_375)
  })
  it('moves explicit mixed AV subjects to separate destinations and rejects a bad batch atomically', () => {
    let document = pairImportedVideoAudio(fixture(), 'clip', { audioTrackId: 'audio', audioClipId: 'sound', linkId: 'pair' })
    document = addTrack(document, { id: 'video-two', kind: 'video', name: 'Video 2', hidden: false, muted: false, locked: false, clips: [] })
    document = addTrack(document, { id: 'audio-two', kind: 'audio', name: 'Audio 2', muted: false, locked: false, clips: [] })
    const before = JSON.stringify(document)
    const placements = [{ clipId: 'sound', trackId: 'audio-two' }, { clipId: 'clip', trackId: 'video-two' }]
    const moved = moveClipPlacements(document, placements, 'clip', 1_016_000)
    expect(moved.tracks[0]!.clips).toEqual([])
    expect(moved.tracks[1]!.clips).toEqual([])
    expect(moved.tracks[2]!.clips[0]).toMatchObject({ id: 'clip', timelineStartTicks: 1_001_000 })
    expect(moved.tracks[3]!.clips[0]).toMatchObject({ id: 'sound', timelineStartTicks: 1_001_000 })
    expect(moved.links).toEqual(document.links)
    const operation = { op: 'move_layer_placements' as const, placements, anchorId: 'clip', startTicks: 1_016_000 }
    expect(evaluateAgentBatch(document, [operation])).toEqual(moved)
    expect(() => evaluateAgentBatch(document, [operation, { op: 'delete_layer', clipId: 'missing' }])).toThrow()
    expect(() => moveClipPlacements(document, [{ clipId: 'clip', trackId: 'audio-two' }], 'clip', 0)).toThrow('cannot be placed')
    expect(() => moveClipPlacements(document, placements, 'missing', 0)).toThrow('anchor')
    expect(() => moveClipPlacements(document, [...placements, placements[0]!], 'clip', 0)).toThrow('distinct')
    expect(() => moveClipPlacements(document, [{ clipId: 'clip', trackId: 'missing' }], 'clip', 0)).toThrow('does not exist')
    expect(JSON.stringify(document)).toBe(before)
  })
  it('moves and pastes linked media on individual grids after snapping the anchor', () => {
    const document = pairImportedVideoAudio(fixture(), 'clip', { audioTrackId: 'audio', audioClipId: 'sound', linkId: 'pair' })
    const before = JSON.stringify(document)
    const moved = moveClip(document, 'clip', 'video', 33_367)
    const clips = moved.tracks.flatMap<CompositionClip>((track) => track.clips)
    expect(clips.map((clip) => [clip.timelineStartTicks, clipEndTicks(clip)])).toEqual([[33_367, 66_733], [33_375, 66_729]])
    expect(clips.every((clip) => 'sourceOutTicks' in clip && clip.sourceOutTicks === 33_367 && clip.sourceInTicks === 0)).toBe(true)
    const subjects = [{ clipId: 'clip', id: 'copy-picture' }, { clipId: 'sound', id: 'copy-sound' }]
    const pasted = pasteClips(document, subjects, { timelineStartTicks: 1_016_000 })
    const copies = pasted.tracks.flatMap<CompositionClip>((track) => track.clips).filter((clip) => clip.id.startsWith('copy-'))
    // Frame 30 at 30000/1001 is 1.001 s; audio follows that snapped head.
    expect(copies.map((clip) => clip.timelineStartTicks)).toEqual([1_001_000, 1_001_000])
    expect(pasted.links).toHaveLength(2)
    expect(JSON.stringify(document)).toBe(before)
  })
  it('retimes authored extent proportionally and restores it with Undo', () => {
    resetCompositionForTests()
    try {
      const document = moveClipsToNewTrack(fixture(), ['clip'], 'fresh', { clipId: 'clip', startTicks: 33_367 })
      compositionState.document = document
      updateCompositionClipSpeed('clip', 2)
      expect(clipDurationTicks(compositionState.document.tracks[0]!.clips[0]!)).toBe(16_683)
      undoComposition()
      expect(compositionState.document).toEqual(document)
    } finally { resetCompositionForTests() }
  })
  it('lands both fractional endpoints while preserving source content and roundtrips the render wire', () => {
    const document = fixture()
    const before = JSON.stringify(document)
    const raised = moveClipsToNewTrack(document, ['clip'], 'fresh', { clipId: 'clip', startTicks: 33_367 })
    const clip = raised.tracks[0]!.clips[0]!
    expect(clip).toMatchObject({ timelineStartTicks: 33_367, timelineDurationTicks: 33_366, sourceInTicks: 0, sourceOutTicks: 33_367 })
    expect(clipEndTicks(clip)).toBe(66_733)
    expect(validateComposition(raised)).toEqual([])
    expect(normalizeComposition(JSON.parse(JSON.stringify(raised)))).toEqual(normalizeComposition(raised))
    const wire = buildCompositionRenderRequest(raised).composition.tracks[0]!
    if (wire.kind !== 'video') throw new Error('Expected video wire')
    expect(wire.clips[0]!.placement).toMatchObject({ timelineDurationTick: 33_366, sourceInTick: 0, sourceOutTick: 33_367 })
    expect(JSON.stringify(document)).toBe(before)
  })

  it('splits and trims explicit extents rather than copying the old full duration to every result', () => {
    const document = moveClipsToNewTrack(fixture(), ['clip'], 'fresh', { clipId: 'clip', startTicks: 33_367 })
    const split = splitClip(document, 'clip', 50_000, 'right')
    expect(split.tracks[0]!.clips.map(clipDurationTicks)).toEqual([16_633, 16_733])
    expect(clipEndTicks(split.tracks[0]!.clips[1]!)).toBe(66_733)
    const trimmed = trimClip(document, 'clip', 40_000, 60_000)
    expect(clipDurationTicks(trimmed.tracks[0]!.clips[0]!)).toBe(20_000)
    expect(clipEndTicks(trimmed.tracks[0]!.clips[0]!)).toBe(60_000)
  })

  it('rejects zero, negative, unsafe and out-of-project authored extents', () => {
    const document = fixture()
    for (const duration of [0, -1, NaN, Number.MAX_SAFE_INTEGER, 86_400_000_001]) {
      const malformed = { ...document, tracks: document.tracks.map((track) => ({ ...track, clips: track.clips.map((clip) => ({ ...clip, timelineDurationTicks: duration })) })) }
      expect(validateComposition(malformed).length).toBeGreaterThan(0)
    }
  })
})

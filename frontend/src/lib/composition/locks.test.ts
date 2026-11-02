import { createCompositionLink } from './links'
import { magnetizeTrack } from './timelineProductivity'
import { upsertCompositionMarker } from './markers'
import { describe, expect, it } from 'vitest'
import { addClip, addTrack, createComposition, registerSource, removeSilenceFromClip, deleteTrack, deleteTransition, setClipLocked, upsertTransition } from './commands'
import { assertCompositionLocksPreserved } from './locks'
import { evaluateAgentBatch } from '../agent/operations'
import type { Composition, VideoClip } from './types'

function fixture(): Composition {
  let document = createComposition({ width: 1280, height: 720, fps: 30, backgroundColor: '#000000' })
  document = registerSource(document, { id: 'source', kind: 'video', durationTicks: 10_000_000, width: 1280, height: 720, hasAudio: true })
  document = addTrack(document, { id: 'video', kind: 'video', name: 'Video', locked: false, hidden: false, muted: false, clips: [], transitions: [] })
  for (const [id, start] of [['a', 0], ['b', 2_000_000]] as const) {
    const clip: VideoClip = { id, kind: 'video', sourceId: 'source', timelineStartTicks: start, sourceInTicks: 1_000_000, sourceOutTicks: 3_000_000, speed: 1, transform: { x: 0, y: 0, width: 1280, height: 720, fit: 'contain' }, opacity: 1, sourceAudioEnabled: true, audioGain: 1, audioPan: 0 }
    document = addClip(document, 'video', clip)
  }
  document = upsertTransition(document, 'video', { id: 'fade', fromClipId: 'a', toClipId: 'b', kind: 'dissolve', durationTicks: 500_000 })
  return setClipLocked(document, 'a', true)
}

describe('protected clip indirect edits', () => {
  it('guards direct transition commands and track deletion before returning a document', () => {
    const document = fixture()
    expect(() => deleteTransition(document, 'video', 'fade')).toThrow('locked')
    expect(() => upsertTransition(document, 'video', { id: 'fade', fromClipId: 'a', toClipId: 'b', kind: 'wipe_left', durationTicks: 500_000 })).toThrow('locked')
    expect(() => deleteTrack(document, 'video')).toThrow('locked')
  })

  it('magnet rejects moving a locked clip but permits an unchanged protected head', () => {
    let document = fixture()
    document = setClipLocked(document, 'a', false)
    document = deleteTransition(document, 'video', 'fade')
    document = { ...document, tracks: document.tracks.map((track) => ({ ...track, clips: track.clips.map((clip) => clip.id === 'b' ? { ...clip, timelineStartTicks: 3_000_000, locked: true } : clip) })) } as Composition
    expect(() => magnetizeTrack(document, 'video')).toThrow('locked')
    const onlyHeadLocked = { ...document, tracks: document.tracks.map((track) => ({ ...track, clips: track.clips.map((clip) => ({ ...clip, locked: clip.id === 'a' })) })) } as Composition
    const next = magnetizeTrack(onlyHeadLocked, 'video')
    expect(next.tracks[0]!.clips[0]!.timelineStartTicks).toBe(0)
    expect(next.tracks[0]!.clips[1]!.timelineStartTicks).toBe(2_000_000)
  })

  it('rejects link reassignment that would dissolve an unselected protected member link', () => {
    let document = setClipLocked(fixture(), 'a', false)
    const template = document.tracks[0]!.clips[0] as VideoClip
    document = addClip(document, 'video', { ...template, id: 'c', timelineStartTicks: 5_000_000 })
    document = createCompositionLink(document, 'old', ['a', 'b'])
    document = setClipLocked(document, 'a', true)
    expect(() => createCompositionLink(document, 'new', ['b', 'c'], true)).toThrow('locked')
  })

  it('refuses silence removal when it would shift an unselected protected downstream clip', () => {
    let document = setClipLocked(fixture(), 'a', false)
    document = deleteTransition(document, 'video', 'fade')
    document = setClipLocked(document, 'b', true)
    const before = JSON.stringify(document)
    expect(() => removeSilenceFromClip(document, 'a', [{ start: 1_000_000, end: 2_000_000 }], [])).toThrow('locked')
    expect(JSON.stringify(document)).toBe(before)
  })

  it('rejects removing or changing a transition through MCP without altering the input', () => {
    const document = fixture()
    const before = JSON.stringify(document)
    expect(() => evaluateAgentBatch(document, [{ op: 'remove_transition', trackId: 'video', transitionId: 'fade' }])).toThrow('locked')
    expect(() => evaluateAgentBatch(document, [{ op: 'add_transition', trackId: 'video', transition: { id: 'fade', fromClipId: 'a', toClipId: 'b', kind: 'wipe_left', durationTicks: 500_000 } }])).toThrow('locked')
    expect(JSON.stringify(document)).toBe(before)
  })

  it('rejects deletion of an unlocked transition partner because the locked endpoint would change', () => {
    expect(() => evaluateAgentBatch(fixture(), [{ op: 'delete_layer', clipId: 'b' }])).toThrow('locked')
  })

  it('protects referenced source metadata while permitting unrelated project metadata', () => {
    const document = fixture()
    const changed = { ...document, sources: { ...document.sources, source: { ...document.sources.source!, durationTicks: 11_000_000 } } }
    expect(() => assertCompositionLocksPreserved(document, changed)).toThrow('source')
    expect(() => assertCompositionLocksPreserved(document, upsertCompositionMarker(document, { id: 'free', tick: 1_000_000, label: 'Free marker' }))).not.toThrow()
  })

  it('requires a lock-only operation before content changes, even if an update also requests unlock', () => {
    const document = fixture()
    expect(() => evaluateAgentBatch(document, [{ op: 'update_layer', clipId: 'a', patch: { locked: false, opacity: 0.5 } }])).toThrow('locked')
    const next = evaluateAgentBatch(document, [{ op: 'set_layer_locked', clipId: 'a', locked: false }, { op: 'update_layer', clipId: 'a', patch: { opacity: 0.5 } }])
    expect(next.tracks[0]!.clips[0]).toMatchObject({ locked: false, opacity: 0.5 })
  })
})

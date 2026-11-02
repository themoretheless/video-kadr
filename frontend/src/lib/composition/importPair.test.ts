import { describe, expect, it } from 'vitest'
import { addClip, addTrack, createComposition, registerSource, setClipLocked } from './commands'
import { pairImportedVideoAudio } from './importPair'
import type { VideoClip } from './types'

const ids = { audioTrackId: 'audio', audioClipId: 'sound', linkId: 'pair' }
function fixture(hasAudio = true) {
  let document = createComposition({ width: 1280, height: 720, fps: 30, backgroundColor: '#000000' })
  document = registerSource(document, { id: 'source', kind: 'video', durationTicks: 10_000_000, width: 1280, height: 720, hasAudio })
  document = addTrack(document, { id: 'video', kind: 'video', name: 'Video', locked: false, hidden: false, muted: false, clips: [], transitions: [] })
  const clip: VideoClip = { id: 'picture', kind: 'video', sourceId: 'source', timelineStartTicks: 3_000_000, sourceInTicks: 1_000_000, sourceOutTicks: 5_000_000, speed: 2, playbackMode: { mode: 'reverse' }, transform: { x: 0, y: 0, width: 1280, height: 720, fit: 'contain' }, opacity: 1, sourceAudioEnabled: hasAudio, audioGain: 0.7, audioPan: -0.25, enabled: false }
  return addClip(document, 'video', clip)
}

describe('atomic imported audio pairing', () => {
  it('preserves offset, reverse, speed, gain and disabled state without modifying the input', () => {
    const document = fixture()
    const before = JSON.stringify(document)
    const next = pairImportedVideoAudio(document, 'picture', ids)
    expect(next.tracks[1]!.clips[0]).toMatchObject({ kind: 'audio', timelineStartTicks: 3_000_000, sourceInTicks: 1_000_000, sourceOutTicks: 5_000_000, speed: 2, reversed: true, gain: 0.7, pan: -0.25, enabled: false })
    expect(next.tracks[0]!.clips[0]).toMatchObject({ sourceAudioEnabled: false })
    expect(next.links).toEqual([{ id: 'pair', memberIds: ['picture', 'sound'] }])
    expect(JSON.stringify(document)).toBe(before)
  })

  it('rejects missing audio, locked video and an occupied destination atomically', () => {
    for (const document of [fixture(false), setClipLocked(fixture(), 'picture', true), pairImportedVideoAudio(fixture(), 'picture', ids)]) {
      const before = JSON.stringify(document)
      expect(() => pairImportedVideoAudio(document, 'picture', ids)).toThrow()
      expect(JSON.stringify(document)).toBe(before)
    }
  })
})

import { expect, it } from 'vitest'
import { addClip, addTrack, createComposition, registerSource } from './commands'
import { pairImportedVideoAudio } from './importPair'
import { insertFreezeFrame } from './freezeInsert'
import { videoSourceTickAtTimelineTick } from './playback'
import { sampleAnimatableValue } from './keyframes'
import type { VideoClip, VideoTrack } from './types'
function fixture(reverse: boolean) {
  let d = registerSource(createComposition({ width: 16, height: 16, fps: 10, backgroundColor: '#000000' }), { id: 'source', kind: 'video', durationTicks: 10_000_000, width: 4, height: 4, hasAudio: true })
  d = addTrack(d, { id: 'v', kind: 'video', name: 'V', locked: false, hidden: false, muted: false, clips: [] })
  const clip: VideoClip = { id: 'picture', kind: 'video', sourceId: 'source', timelineStartTicks: 0, sourceInTicks: 0, sourceOutTicks: 8_000_000, speed: 2, playbackMode: { mode: reverse ? 'reverse' : 'forward' }, transform: { x: 0, y: 0, width: 16, height: 16, fit: 'contain' }, opacity: 1, sourceAudioEnabled: true, audioGain: 1, animation: { opacity: { mode: 'keyframes', track: { timeBase: 1_000_000, interpolation: 'linear', keyframes: [{ tick: 0, value: 0 }, { tick: 4_000_000, value: 1 }] } } } }
  return pairImportedVideoAudio(addClip(d, 'v', clip), 'picture', { audioTrackId: 'a', audioClipId: 'sound', linkId: 'pair' })
}
it.each([false, true])('inserts silent hold and ripples linked AV with reverse=%s', reverse => {
  const d = fixture(reverse), before = JSON.stringify(d)
  let next = 0
  const result = insertFreezeFrame(d, 'picture', 1_500_000, 1_000_000, () => `new-${next++}`)
  const video = result.document.tracks[0] as VideoTrack
  const hold = video.clips.find(c => c.id === result.clipId)!
  const tail = video.clips.find(c => c.timelineStartTicks === 2_500_000)!
  const original = (d.tracks[0] as VideoTrack).clips[0]!
  expect(video.clips).toHaveLength(3)
  expect(hold.sourceAudioEnabled).toBe(false)
  for (const at of [1_500_000, 1_750_000, 2_499_999]) expect(videoSourceTickAtTimelineTick(hold, at)).toBe(videoSourceTickAtTimelineTick(original, 1_500_000))
  expect(videoSourceTickAtTimelineTick(tail, 2_500_000)).toBe(videoSourceTickAtTimelineTick(original, 1_500_000))
  expect(sampleAnimatableValue(hold.animation!.opacity!, 0)).toBe(sampleAnimatableValue(original.animation!.opacity!, 1_500_000))
  expect(result.document.tracks[1]!.clips.map(c => c.timelineStartTicks)).toEqual([0, 2_500_000])
  expect(result.document.links?.some(link => link.memberIds.includes(tail.id) && link.memberIds.length === 2)).toBe(true)
  expect(JSON.stringify(d)).toBe(before)
})
it('refuses boundaries and locked participants without changing the document', () => {
  const d = fixture(false), before = JSON.stringify(d)
  for (const at of [0, 4_000_000]) expect(() => insertFreezeFrame(d, 'picture', at, 1_000_000, () => 'new')).toThrow()
  const locked = { ...d, tracks: d.tracks.map(t => ({ ...t, locked: true })) }
  expect(() => insertFreezeFrame(locked, 'picture', 1_000_000, 1_000_000, () => 'new')).toThrow()
  expect(JSON.stringify(d)).toBe(before)
})

it('refuses a locked or sync-excluded linked audio partner atomically', () => {
  const original = fixture(false)
  for (const patch of [{ locked: true }, { syncLock: false }]) {
    const d = { ...original, tracks: original.tracks.map(t => t.kind === 'audio' ? { ...t, ...patch } : t) }
    const before = JSON.stringify(d)
    expect(() => insertFreezeFrame(d, 'picture', 1_500_000, 1_000_000, () => 'new')).toThrow()
    expect(JSON.stringify(d)).toBe(before)
  }
})

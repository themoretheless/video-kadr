import { describe, expect, it } from 'vitest'
import { addClip, addTrack, createComposition, registerSource, splitClip } from './commands'
import { pairImportedVideoAudio } from './importPair'
import { mergeClips } from './mergeClips'
import { sampleAnimatableValue } from './keyframes'
import type { Composition, VideoClip, VideoTrack } from './types'
function fixture(reverse = false): Composition {
  let d = registerSource(createComposition({ width: 640, height: 360, fps: 30, backgroundColor: '#000000' }), { id: 'source', kind: 'video', durationTicks: 10_000_000, width: 640, height: 360, hasAudio: true })
  d = addTrack(d, { id: 'video', kind: 'video', name: 'Video', locked: false, hidden: false, muted: false, clips: [] })
  const clip: VideoClip = { id: 'picture', kind: 'video', sourceId: 'source', timelineStartTicks: 0, sourceInTicks: 0, sourceOutTicks: 8_000_000, speed: 2, playbackMode: { mode: reverse ? 'reverse' : 'forward' }, transform: { x: 0, y: 0, width: 640, height: 360, fit: 'contain' }, opacity: 1, sourceAudioEnabled: true, audioGain: 0.7, animation: { opacity: { mode: 'keyframes', track: { timeBase: 1_000_000, interpolation: 'ease_in_out', keyframes: [{ tick: 0, value: 0 }, { tick: 4_000_000, value: 1 }] } } } }
  return pairImportedVideoAudio(addClip(d, 'video', clip), 'picture', { audioTrackId: 'audio', audioClipId: 'sound', linkId: 'pair' })
}
describe('lossless fragment joining', () => {
  it.each([false, true])('joins linked AV with reverse=%s, preserving source spans and crossing curves', reverse => {
    const original = fixture(reverse)
    const split = splitClip(original, 'picture', 1_500_000, 'tail')
    const before = JSON.stringify(split)
    const merged = mergeClips(split, ['picture', 'tail']).document
    expect(merged.tracks.map(t => t.clips.length)).toEqual([1, 1])
    expect(merged.links).toEqual([{ id: 'pair', memberIds: ['picture', 'sound'] }])
    const clip = (merged.tracks[0] as VideoTrack).clips[0]!
    expect(clip).toMatchObject({ sourceInTicks: 0, sourceOutTicks: 8_000_000, timelineDurationTicks: 4_000_000 })
    const source = (original.tracks[0] as VideoTrack).clips[0]!
    for (const at of [0, 100_000, 1_499_999, 1_500_000, 2_500_000, 3_999_999]) expect(sampleAnimatableValue(clip.animation!.opacity!, at)).toBe(sampleAnimatableValue(source.animation!.opacity!, at))
    expect(JSON.stringify(split)).toBe(before)
  })
  it('refuses source discontinuity, changed settings, locks and gaps atomically', () => {
    const split = splitClip(fixture(), 'picture', 2_000_000, 'tail')
    const cases = [
      { ...split, tracks: split.tracks.map(t => t.kind === 'video' ? { ...t, clips: t.clips.map(c => c.id === 'tail' ? { ...c, sourceInTicks: c.sourceInTicks + 1_000_000 } : c) } : t) },
      { ...split, tracks: split.tracks.map(t => t.kind === 'video' ? { ...t, locked: true } : t) },
      { ...split, tracks: split.tracks.map(t => t.kind === 'video' ? { ...t, clips: t.clips.map(c => c.id === 'tail' ? { ...c, opacity: 0.5 } : c) } : t) },
      { ...split, tracks: split.tracks.map(t => t.kind === 'video' ? { ...t, clips: t.clips.map(c => c.id === 'tail' ? { ...c, timelineStartTicks: c.timelineStartTicks + 1_000_000 } : c) } : t) },
    ]
    for (const d of cases) {
      const before = JSON.stringify(d)
      expect(() => mergeClips(d, ['picture', 'tail'])).toThrow()
      expect(JSON.stringify(d)).toBe(before)
    }
  })
})

import { videoSourceTickAtTimelineTick } from './playback'
import { transitionWindowProgress } from './transitionTimeline'
it.each([false, true])('preserves outer transition clocks and media frames (explicit=%s)', explicit => {
  let d = splitClip(fixture(), 'picture', 1_500_000, 'tail')
  const first = (d.tracks[0] as VideoTrack).clips[0]!
  d = addClip(d, 'video', { ...first, id: 'next', timelineStartTicks: 4_000_000, sourceInTicks: 2_000_000, sourceOutTicks: 6_000_000, speed: 1, animation: undefined })
  const transition = { id: 'outer', fromClipId: 'tail', toClipId: 'next', durationTicks: explicit ? 0 : 1_000_000, kind: 'dissolve' as const, ...(explicit ? { timeline: { startTick: '889056000000', durationTick: '254016000000' } } : {}) }
  d = { ...d, tracks: d.tracks.map(t => t.kind === 'video' ? { ...t, transitions: [transition] } : t) }
  const merged = mergeClips(d, ['picture', 'tail']).document
  const lane = merged.tracks[0] as VideoTrack
  expect(lane.transitions?.[0]).toEqual({ ...transition, fromClipId: 'picture' })
  const oldTail = (d.tracks[0] as VideoTrack).clips.find(c => c.id === 'tail')!
  const joined = lane.clips.find(c => c.id === 'picture')!
  for (const at of [3_500_000, 3_750_000, 3_999_999, 4_000_000, 4_250_000]) {
    expect(videoSourceTickAtTimelineTick(joined, at)).toBe(videoSourceTickAtTimelineTick(oldTail, at))
    if (explicit) expect(transitionWindowProgress(lane.transitions![0]!.timeline!, at)).toBe(transitionWindowProgress(transition.timeline!, at))
  }
})

it('refuses internal transitions and outer windows crossing the old fragment boundary atomically', () => {
  let d = splitClip(fixture(), 'picture', 1_500_000, 'tail')
  const first = (d.tracks[0] as VideoTrack).clips[0]!
  d = addClip(d, 'video', { ...first, id: 'next', timelineStartTicks: 4_000_000, sourceInTicks: 2_000_000, sourceOutTicks: 6_000_000, speed: 1, animation: undefined })
  for (const [transition, reason] of [
    [{ id: 'inner', fromClipId: 'picture', toClipId: 'tail', durationTicks: 0, kind: 'dissolve', timeline: { startTick: '254016000000', durationTick: '254016000000' } }, /внутри объединения/],
    [{ id: 'outer', fromClipId: 'tail', toClipId: 'next', durationTicks: 0, kind: 'dissolve', timeline: { startTick: '254016000000', durationTick: '762048000000' } }, /Окно перехода/],
  ] as const) {
    const input = { ...d, tracks: d.tracks.map(t => t.kind === 'video' ? { ...t, transitions: [transition] } : t) }
    const before = JSON.stringify(input)
    expect(() => mergeClips(input, ['picture', 'tail'])).toThrow(reason)
    expect(JSON.stringify(input)).toBe(before)
  }
})


it.each([500_000, 9_500_000])('rejoins split freeze at source tick %s without changing sampled frames or animation', sourceTick => {
  const base = fixture()
  const original: Composition = { ...base, links: [], tracks: base.tracks.filter(t => t.kind === 'video').map(t => ({ ...t, clips: t.clips.map(c => ({ ...c, playbackMode: { mode: 'freeze' as const, sourceTick }, sourceInTicks: sourceTick < 1_000_000 ? 0 : 2_000_000, sourceOutTicks: sourceTick < 1_000_000 ? 8_000_000 : 10_000_000, sourceAudioEnabled: false })) })) }
  const split = splitClip(splitClip(original, 'picture', 1_500_000, 'middle'), 'middle', 3_000_000, 'tail')
  const before = JSON.stringify(split)
  const joined = mergeClips(split, ['picture', 'middle', 'tail']).document
  const clip = (joined.tracks[0] as VideoTrack).clips[0]!
  const initial = (original.tracks[0] as VideoTrack).clips[0]!
  expect(clip.timelineDurationTicks).toBe(4_000_000)
  expect(clip.sourceInTicks).toBeLessThanOrEqual(sourceTick)
  expect(clip.sourceOutTicks).toBeGreaterThan(sourceTick)
  for (const at of [0, 1_499_999, 1_500_000, 2_000_000, 2_999_999, 3_000_000, 3_999_999]) {
    expect(videoSourceTickAtTimelineTick(clip, at)).toBe(sourceTick)
    expect(sampleAnimatableValue(clip.animation!.opacity!, at)).toBe(sampleAnimatableValue(initial.animation!.opacity!, at))
  }
  expect(JSON.stringify(split)).toBe(before)
  const changed = { ...split, tracks: split.tracks.map(t => t.kind === 'video' ? { ...t, clips: t.clips.map(c => c.id === 'tail' ? { ...c, playbackMode: { mode: 'freeze' as const, sourceTick: sourceTick + 1 } } : c) } : t) }
  expect(() => mergeClips(changed, ['picture', 'middle', 'tail'])).toThrow()
})

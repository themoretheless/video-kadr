import { describe, expect, it } from 'vitest'
import { addClip, addTrack, createComposition, registerSource } from '../composition/commands.js'
import { clipEndTicks, type VideoClip } from '../composition/types.js'
import { applyShotReview, reviewShotRows, shotCoverPosition, validateShotScan, type ShotScan } from './shots.js'
import { compositionMarkers } from '../composition/markers.js'

const scan: ShotScan = { version: 1, sourceHash: 'a'.repeat(64), durationTicks: 4_000_000, floor: 0.05, cuts: [{ tick: 1_000_000, score: 0.7 }, { tick: 2_000_000, score: 0.2 }, { tick: 3_000_000, score: 0.8 }] }
function fixture(patch: Partial<VideoClip> = {}) {
  let document = createComposition({ width: 1280, height: 720, fps: 30, backgroundColor: '#000000' })
  document = registerSource(document, { id: 'source', kind: 'video', durationTicks: 4_000_000, width: 1280, height: 720, hasAudio: true })
  document = addTrack(document, { id: 'video', kind: 'video', name: 'Video', locked: false, hidden: false, muted: false, transitions: [], clips: [] })
  const clip: VideoClip = { id: 'clip', kind: 'video', sourceId: 'source', sourceInTicks: 0, sourceOutTicks: 4_000_000, timelineStartTicks: 5_000_000, transform: { x: 0, y: 0, width: 1280, height: 720, fit: 'contain' }, opacity: 1, sourceAudioEnabled: true, audioGain: 1, ...patch }
  return addClip(document, 'video', clip)
}

describe('source scan review', () => {
  it('changes threshold and granularity by reduction and retains source-absolute boundaries', () => {
    const document = fixture()
    expect(reviewShotRows(document, 'clip', scan, 0.3).map((row) => [row.startTicks, row.endTicks])).toEqual([[0, 1_000_000], [1_000_000, 3_000_000], [3_000_000, 4_000_000]])
    expect(reviewShotRows(document, 'clip', scan, 0.1).map((row) => row.startTicks)).toEqual([0, 1_000_000, 2_000_000, 3_000_000])
    expect(reviewShotRows(document, 'clip', scan, 0.3, 500_000, [1_000_000]).map((row) => row.startTicks)).toEqual([0, 3_000_000])
    expect(reviewShotRows(document, 'clip', scan, 0.1, 1_500_000).map((row) => [row.startTicks, row.endTicks])).toEqual([[0, 2_000_000], [2_000_000, 4_000_000]])
  })
  it('maps reverse and constant/ramped speed to the composition clock', () => {
    const reverse = fixture({ playbackMode: { mode: 'reverse' }, speed: 2 })
    const rows = reviewShotRows(reverse, 'clip', scan)
    expect(rows.map((row) => row.startTicks)).toEqual([3_000_000, 1_000_000, 0])
    expect(rows.map((row) => row.timelineStartTicks)).toEqual([5_000_000, 5_500_000, 6_500_000])
    const ramped = fixture({ speedRamp: { interpolation: 'hold', audioPolicy: 'preserve_pitch', points: [{ sourceProgressTick: 0, speed: 1 }, { sourceProgressTick: 2_000_000, speed: 2 }, { sourceProgressTick: 4_000_000, speed: 2 }] } })
    const rampRows = reviewShotRows(ramped, 'clip', scan)
    expect(rampRows[1]?.timelineStartTicks).toBe(6_000_000)
    expect(rampRows[2]?.timelineStartTicks).toBe(7_500_000)
    expect(rampRows.at(-1)?.timelineEndTicks).toBe(8_000_000)
  })
  it('seeks to the displayed source frame through reverse, ramped and frozen placement', () => {
    const reverse = fixture({ playbackMode: { mode: 'reverse' }, speed: 2 })
    const reversed = reverse.tracks[0]!.clips[0] as VideoClip
    expect(shotCoverPosition(reversed, reviewShotRows(reverse, 'clip', scan)[0]!))
      .toEqual({ sourceTick: 3_499_999, timelineTick: 5_250_000 })
    const ramped = fixture({ speedRamp: { interpolation: 'hold', audioPolicy: 'preserve_pitch', points: [
      { sourceProgressTick: 0, speed: 1 }, { sourceProgressTick: 2_000_000, speed: 2 }, { sourceProgressTick: 4_000_000, speed: 2 },
    ] } })
    const rampClip = ramped.tracks[0]!.clips[0] as VideoClip
    expect(shotCoverPosition(rampClip, reviewShotRows(ramped, 'clip', scan)[2]!))
      .toEqual({ sourceTick: 3_500_000, timelineTick: 7_750_000 })
    const frozen = fixture({ playbackMode: { mode: 'freeze', sourceTick: 2_500_000 }, sourceAudioEnabled: false })
    const still = frozen.tracks[0]!.clips[0] as VideoClip
    expect(shotCoverPosition(still, reviewShotRows(frozen, 'clip', scan)[0]!))
      .toEqual({ sourceTick: 2_500_000, timelineTick: 5_000_000 })
  })
  it('split, markers and discard use the same frame-snapped boundaries', () => {
    const document = fixture()
    const rows = reviewShotRows(document, 'clip', scan)
    const id = (index: number) => `segment-${index}`
    const split = applyShotReview(document, 'clip', rows, 'split', [], id)
    const clips = split.document.tracks[0]!.clips
    expect(clips.map((clip) => clip.timelineStartTicks)).toEqual([5_000_000, 6_000_000, 8_000_000])
    expect(clips.map(clipEndTicks)).toEqual([6_000_000, 8_000_000, 9_000_000])
    const marked = applyShotReview(document, 'clip', rows, 'mark', [], id)
    expect(compositionMarkers(marked.document).map((marker) => marker.tick)).toEqual(clips.slice(1).map((clip) => clip.timelineStartTicks))
    const discarded = applyShotReview(document, 'clip', rows, 'discard', [rows[0]!.id], id)
    expect(discarded.selectedClipId).toBe('segment-0')
    expect(discarded.document.tracks[0]?.clips.map((clip) => clip.id)).toEqual(['segment-0', 'segment-1'])
    expect(document.tracks[0]?.clips).toHaveLength(1)
    expect(() => applyShotReview(document, 'clip', rows, 'discard', rows.map((row) => row.id), id)).toThrow('Нельзя исключить все')
  })
  it('cuts linked audio at the same boundaries and removes only the audio inside discarded scenes', () => {
    let document = fixture()
    document = registerSource(document, { id: 'audio', kind: 'audio', durationTicks: 5_000_000, width: 0, height: 0, hasAudio: true })
    document = addTrack(document, { id: 'sound', kind: 'audio', name: 'Sound', locked: false, muted: false, clips: [] })
    document = addClip(document, 'sound', {
      id: 'partner', kind: 'audio', sourceId: 'audio', timelineStartTicks: 4_500_000,
      sourceInTicks: 0, sourceOutTicks: 5_000_000, gain: 1,
    })
    document = { ...document, links: [{ id: 'av', memberIds: ['partner', 'clip'] }] }
    const rows = reviewShotRows(document, 'clip', scan)
    const id = (index: number) => `shot-${index}`
    const spans = (doc: typeof document, lane: number) =>
      doc.tracks[lane]!.clips.map(clip => [clip.timelineStartTicks, clipEndTicks(clip)])
    const split = applyShotReview(document, 'clip', rows, 'split', [], id)
    expect(spans(split.document, 1)).toEqual([[4_500_000, 6_000_000], [6_000_000, 8_000_000], [8_000_000, 9_500_000]])
    expect(split.document.links?.map(link => link.memberIds)).toEqual([
      ['partner', 'clip'], ['shot-2', 'shot-0'], ['shot-3', 'shot-1'],
    ])
    const discarded = applyShotReview(document, 'clip', rows, 'discard', [rows[0]!.id, rows[2]!.id], id)
    expect(spans(discarded.document, 0)).toEqual([[6_000_000, 8_000_000]])
    expect(spans(discarded.document, 1)).toEqual([[4_500_000, 5_000_000], [6_000_000, 8_000_000], [9_000_000, 9_500_000]])
    expect(discarded.selectedClipId).toBe('shot-0')
    expect(discarded.document.links).toEqual([{ id: 'av', memberIds: ['shot-3', 'shot-0'] }])
    const locked = { ...document, tracks: document.tracks.map(track => track.id === 'sound' ? { ...track, locked: true } : track) }
    expect(() => applyShotReview(locked, 'clip', rows, 'discard', [rows[0]!.id], id)).toThrow()
    expect(spans(document, 1)).toEqual([[4_500_000, 9_500_000]])
  })
  it('rejects malformed scans and frame-collapsed boundaries before producing an edit', () => {
    for (const bad of [{ ...scan, floor: 0 }, { ...scan, cuts: [{ tick: 1, score: NaN }] }, { ...scan, cuts: [...scan.cuts].reverse() }]) expect(() => validateShotScan(bad)).toThrow()
    const document = fixture()
    const tiny: ShotScan = { ...scan, cuts: [{ tick: 20_000, score: 0.7 }, { tick: 22_000, score: 0.7 }] }
    const rows = reviewShotRows(document, 'clip', tiny, 0.3, 1)
    expect(() => applyShotReview(document, 'clip', rows, 'discard', [rows[0]!.id], (index) => `new-${index}`)).toThrow('сетке кадров')
    const locked = { ...document, tracks: document.tracks.map((track) => ({ ...track, locked: true })) }
    expect(() => applyShotReview(locked, 'clip', reviewShotRows(document, 'clip', scan), 'split', [], (index) => `new-${index}`)).toThrow('заблокирована')
  })
})

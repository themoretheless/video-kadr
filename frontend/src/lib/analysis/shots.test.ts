import { describe, expect, it } from 'vitest'
import { addClip, addTrack, createComposition, registerSource } from '../composition/commands.js'
import { clipEndTicks, type VideoClip } from '../composition/types.js'
import { applyShotReview, reviewShotRows, validateShotScan, type ShotScan } from './shots.js'
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

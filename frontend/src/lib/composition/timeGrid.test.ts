import { addClip, addTrack, pasteClips, moveClips, createComposition } from './commands'
import { normalizeComposition, validateComposition } from './validation'
import { describe, expect, it } from 'vitest'
import { shiftTimeGridMembers, stepTimeGridTicks, canvasTimeGrid, AUDIO_TIME_GRID, gridBoundaryTicks, shiftTimeGridSpan, snapTimeGridTicks } from './timeGrid'

describe('rational frame and sample boundaries', () => {
  it('preserves authored animation at a shortened fractional landing without changing the original', () => {
    let document = createComposition({ width: 1280, height: 720, fps: 30000 / 1001, fpsRate: { numerator: 30000, denominator: 1001 }, backgroundColor: '#000000' })
    document = addTrack(document, { id: 'titles', kind: 'text', name: 'Titles', hidden: false, locked: false, clips: [] })
    document = addClip(document, 'titles', { id: 'animated', kind: 'text', timelineStartTicks: 0, durationTicks: 33_367, text: 'A', x: 0, y: 0, opacity: 1, style: { fontSizePx: 48, color: '#FFFFFFFF', align: 'center' }, animation: { opacity: { mode: 'keyframes', track: { timeBase: 1_000_000, interpolation: 'linear', keyframes: [{ tick: 0, value: 0 }, { tick: 33_367, value: 1 }] } } } })
    const before = JSON.stringify(document)
    const moved = moveClips(document, ['animated'], 'titles', 33_367)
    const pasted = pasteClips(document, [{ clipId: 'animated', id: 'copy' }], { timelineStartTicks: 33_367 })
    for (const next of [moved, pasted]) {
      expect(validateComposition(next)).toEqual([])
      const clip = next.tracks[0]!.clips.at(-1)!
      if (clip.kind !== 'text') throw new Error('Expected text')
      const opacity = clip.animation!.opacity!
      if (opacity.mode !== 'keyframes') throw new Error('Expected curve')
      expect(opacity.track.keyframes.at(-1)!.tick).toBe(33_367)
      expect(opacity.track.keyframes.at(-1)!.value).toBe(1)
    }
    expect(JSON.stringify(document)).toBe(before)
  })

  it('applies both fractional endpoints when moving static visual clips', () => {
    let document = createComposition({ width: 1280, height: 720, fps: 30000 / 1001, fpsRate: { numerator: 30000, denominator: 1001 }, backgroundColor: '#000000' })
    document = addTrack(document, { id: 'titles', kind: 'text', name: 'Titles', hidden: false, locked: false, clips: [] })
    document = addClip(document, 'titles', { id: 'title', kind: 'text', timelineStartTicks: 0, durationTicks: 33_367, text: 'A', x: 0, y: 0, opacity: 1, style: { fontSizePx: 48, color: '#FFFFFFFF', align: 'center' } })
    const before = JSON.stringify(document)
    const moved = moveClips(document, ['title'], 'titles', 33_367)
    expect(moved.tracks[0]!.clips[0]).toMatchObject({ timelineStartTicks: 33_367, durationTicks: 33_366 })
    expect(JSON.stringify(document)).toBe(before)
    expect(validateComposition(moved)).toEqual([])
    const pasted = pasteClips(document, [{ clipId: 'title', id: 'copy' }], { timelineStartTicks: 33_367 })
    expect(pasted.tracks[0]!.clips[1]).toMatchObject({ id: 'copy', timelineStartTicks: 33_367, durationTicks: 33_366 })
    expect(pasted.tracks[0]!.clips[0]).toEqual(document.tracks[0]!.clips[0])
    expect(validateComposition(pasted)).toEqual([])
  })

  it('refuses a collapsed member or unsafe landing without returning a partial plan', () => {
    const rate = { numerator: 30, denominator: 1 }
    const members = [{ id: 'valid', kind: 'audio' as const, start: 0, end: 1_000_000 }, { id: 'tiny', kind: 'video' as const, start: 0, end: 1 }]
    const before = JSON.stringify(members)
    expect(() => shiftTimeGridMembers(members, 1_000_000, rate)).toThrow('collapses')
    expect(JSON.stringify(members)).toBe(before)
    expect(() => shiftTimeGridSpan(Number.MAX_SAFE_INTEGER - 1, Number.MAX_SAFE_INTEGER, 2, AUDIO_TIME_GRID)).toThrow()
    expect(() => shiftTimeGridMembers([{ id: 'bad', kind: 'unknown' as 'video', start: 0, end: 1_000_000 }], 0, rate)).toThrow('member')
    expect(() => shiftTimeGridSpan(0, 1, 0, AUDIO_TIME_GRID)).toThrow('collapses')
  })

  it('plans mixed AV endpoints on their own grids while preserving phase at the origin', () => {
    const rate = { numerator: 30_000, denominator: 1001 }
    const members = [{ id: 'video', kind: 'video' as const, start: 0, end: 1_001_000 }, { id: 'audio', kind: 'audio' as const, start: 0, end: 1_001_000 }]
    const moved = shiftTimeGridMembers(members, 33_367, rate)
    expect(moved.get('video')).toEqual({ start: 33_367, end: 1_034_367 })
    expect(moved.get('audio')).toEqual({ start: 33_375, end: 1_034_375 })
    const phased = [{ id: 'early', kind: 'audio' as const, start: 10_000, end: 30_000 }, { id: 'late', kind: 'audio' as const, start: 20_000, end: 40_000 }]
    expect([...shiftTimeGridMembers(phased, -1_000_000, rate).values()]).toEqual([{ start: 0, end: 20_000 }, { start: 10_000, end: 30_000 }])
    expect(() => shiftTimeGridMembers([...members, members[0]!], 0, rate)).toThrow()
  })

  it('steps through fractional boundaries without accumulating rounded-frame drift', () => {
    const rate = { numerator: 30_000, denominator: 1001 }
    let tick = 0
    for (let index = 0; index < 30_000; index++) tick = stepTimeGridTicks(tick, 1, rate)
    expect(tick).toBe(1_001_000_000)
    expect(stepTimeGridTicks(66_733, -1, rate)).toBe(33_367)
    expect(stepTimeGridTicks(0, -1, rate)).toBe(0)
  })

  it('persists an explicit rational rate and refuses conflicting or malformed frame metadata', () => {
    const document = createComposition({ width: 1280, height: 720, fps: 30_000 / 1001, fpsRate: { numerator: 30_000, denominator: 1001 }, backgroundColor: '#000000' })
    const restored = normalizeComposition(JSON.parse(JSON.stringify(document)))
    expect(canvasTimeGrid(restored.canvas)).toEqual({ numerator: 30_000, denominator: 1001 })
    expect(validateComposition({ ...document, canvas: { ...document.canvas, fps: 30 } }).some((issue) => issue.code === 'canvas-fps-rate')).toBe(true)
    expect(validateComposition({ ...document, canvas: { ...document.canvas, fpsRate: { numerator: 30_000, denominator: 0 } } }).some((issue) => issue.code === 'canvas-fps-rate')).toBe(true)
    expect(canvasTimeGrid({ fps: 29.97 })).toEqual({ numerator: 29_970_000, denominator: 1_000_000 })
  })

  it('measures fractional frame lengths by position instead of carrying a rounded duration', () => {
    const rate = { numerator: 30_000, denominator: 1001 }
    expect(gridBoundaryTicks(1, rate)).toBe(33_367)
    expect(gridBoundaryTicks(2, rate)).toBe(66_733)
    expect(shiftTimeGridSpan(0, 33_367, 33_367, rate)).toEqual({ start: 33_367, end: 66_733 })
    expect(gridBoundaryTicks(30_000, rate)).toBe(1_001_000_000)
  })
  it('lands the same microsecond shift on each member own lattice', () => {
    expect(shiftTimeGridSpan(1_000_000, 2_000_000, -21, AUDIO_TIME_GRID)).toEqual({ start: 999_979, end: 1_999_979 })
    expect(shiftTimeGridSpan(1_000_000, 2_000_000, -21, { numerator: 30, denominator: 1 })).toEqual({ start: 1_000_000, end: 2_000_000 })
  })
  it('keeps long-timeline arithmetic exact and rejects invalid rates or unsafe ticks', () => {
    const rate = { numerator: 30_000, denominator: 1001 }
    const boundary = gridBoundaryTicks(10_000_000, rate)
    expect(snapTimeGridTicks(boundary, rate)).toBe(boundary)
    expect(() => snapTimeGridTicks(-1, rate)).toThrow()
    expect(() => gridBoundaryTicks(1, { numerator: 0, denominator: 1 })).toThrow()
    expect(() => gridBoundaryTicks(Number.MAX_SAFE_INTEGER, rate)).toThrow()
  })
})

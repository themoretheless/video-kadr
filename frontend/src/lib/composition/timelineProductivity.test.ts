import rippleOracle from '../../../../backend/tests/fixtures/filmcraft-caption-ripple.json'
import { createCompositionLink } from './links'
import { describe, expect, it } from 'vitest'

import { addClip, addTrack, createComposition, registerSource } from './commands'
import { COMPOSITION_TIME_BASE, type Composition, type VideoClip } from './types'
import { extractCaptionRanges, compositionGaps, compositionGapAt, rippleDeleteGap, magnetizeTrack, rippleDeleteClip, rippleDeleteClips } from './timelineProductivity'

const second = COMPOSITION_TIME_BASE

function clip(id: string, start: number, duration: number): VideoClip {
  return {
    id,
    kind: 'video',
    sourceId: 'source',
    timelineStartTicks: start,
    sourceInTicks: 0,
    sourceOutTicks: duration,
    speed: 1,
    transform: { x: 0, y: 0, width: 1280, height: 720, fit: 'contain' },
    opacity: 1,
    sourceAudioEnabled: true,
    audioGain: 1,
    audioPan: 0,
  }
}

function document(): Composition {
  let value = createComposition({ width: 1280, height: 720, fps: 30, backgroundColor: '#000000' })
  value = registerSource(value, {
    id: 'source',
    kind: 'video',
    durationTicks: 20 * second,
    width: 1280,
    height: 720,
    hasAudio: true,
  })
  value = addTrack(value, {
    id: 'main',
    kind: 'video',
    name: 'Main',
    locked: false,
    hidden: false,
    muted: false,
    transitions: [],
    clips: [],
  })
  value = addTrack(value, {
    id: 'overlay',
    kind: 'video',
    name: 'Overlay',
    locked: false,
    hidden: false,
    muted: true,
    transitions: [],
    clips: [],
  })
  value = addClip(value, 'main', clip('a', 0, 2 * second))
  value = addClip(value, 'main', clip('b', 3 * second, 2 * second))
  value = addClip(value, 'main', clip('c', 7 * second, second))
  return addClip(value, 'overlay', clip('overlay-clip', 5 * second, second))
}

describe('timeline productivity commands', () => {
  it('ripple-deletes the selected interval across all tracks and preserves source timing', () => {
    const next = rippleDeleteClip(document(), 'b')
    const main = next.tracks.find((track) => track.id === 'main')!
    const overlay = next.tracks.find((track) => track.id === 'overlay')!

    expect(main.clips.map((item) => [item.id, item.timelineStartTicks])).toEqual([
      ['a', 0],
      ['c', 5 * second],
    ])
    expect(main.clips[1]).toMatchObject({ sourceInTicks: 0, sourceOutTicks: second })
    expect(overlay.clips[0]!.timelineStartTicks).toBe(3 * second)
  })

  it('merges overlapping selections instead of closing the interval twice', () => {
    const value = addClip(document(), 'overlay', clip('pair', 3 * second, 2 * second))
    const next = rippleDeleteClips(value, ['b', 'pair'])
    expect(next.tracks[0]!.clips.find((item) => item.id === 'c')!.timelineStartTicks).toBe(5 * second)
  })

  it('refuses surviving clips inside a hole without changing the input', () => {
    const value = addClip(document(), 'overlay', clip('inside', 4 * second, second))
    const before = JSON.stringify(value)
    expect(() => rippleDeleteClips(value, ['b'])).toThrow('inside')
    expect(JSON.stringify(value)).toBe(before)
  })

  it('blocks downstream locked lanes but permits locked content ahead of the cut', () => {
    const value = document()
    const locked = { ...value, tracks: value.tracks.map((track) => track.id === 'overlay' ? { ...track, locked: true } : track) } as Composition
    expect(() => rippleDeleteClips(locked, ['b'])).toThrow('locked')
    const ahead = { ...locked, tracks: locked.tracks.map((track) => track.id === 'overlay' ? { ...track, clips: [clip('head', 0, second)] } : track) } as Composition
    expect(rippleDeleteClips(ahead, ['b']).tracks[1]!.clips[0]!.timelineStartTicks).toBe(0)
  })

  it('refuses a link reaching across the cut while another member would move', () => {
    const value = createCompositionLink(addClip(document(), 'overlay', clip('reaching', 0, 4 * second)), 'sync', ['reaching', 'c'])
    expect(() => rippleDeleteClips(value, ['b'])).toThrow('straddles')
  })

  it('permits linked split pieces wholly ahead of and after the cut', () => {
    const value = createCompositionLink(document(), 'pieces', ['a', 'b', 'c'])
    const next = rippleDeleteClips(value, ['b'])
    expect(next.links?.[0]?.memberIds).toEqual(['a', 'c'])
    expect(next.tracks[0]!.clips[1]!.timelineStartTicks).toBe(5 * second)
  })

  it('refuses a landing collision with an overlay anchored before the cut', () => {
    const value = addClip(document(), 'overlay', clip('anchor', 0, 4 * second))
    expect(() => rippleDeleteClips(value, ['b'])).toThrow()
  })

  it('enumerates bounded gaps independent of clip ordering and overlapping tails', () => {
    expect(compositionGaps([clip('last', 7 * second, second), clip('overlap', 2 * second, 3 * second), clip('first', second, 3 * second)])).toEqual([
      { start: 0, end: second }, { start: 5 * second, end: 7 * second },
    ])
    expect(compositionGaps([])).toEqual([])
  })

  it('resolves leading and interior gaps with half-open boundaries', () => {
    const clips = [clip('first', second, second), clip('last', 4 * second, second)]
    expect(compositionGapAt(clips, 0)).toEqual({ start: 0, end: second })
    expect(compositionGapAt(clips, second)).toBeNull()
    expect(compositionGapAt(clips, 2 * second)).toEqual({ start: 2 * second, end: 4 * second })
    expect(compositionGapAt(clips, 5 * second)).toBeNull()
    expect(compositionGapAt([], 0)).toBeNull()
    expect(compositionGapAt(clips, -1)).toBeNull()
  })

  it('closes one exact gap on all tracks while retaining clips', () => {
    const value = document()
    const next = rippleDeleteGap(value, 'main', 2 * second, 3 * second)
    expect(next.tracks[0]!.clips.map((item) => item.timelineStartTicks)).toEqual([0, 2 * second, 6 * second])
    expect(next.tracks[1]!.clips[0]!.timelineStartTicks).toBe(4 * second)
    expect(next.tracks.reduce((count, track) => count + track.clips.length, 0)).toBe(4)
  })

  it('rejects stale, partial, trailing and missing-track gap requests', () => {
    const value = document()
    expect(() => rippleDeleteGap(value, 'main', 2 * second, 2.5 * second)).toThrow('exact')
    expect(() => rippleDeleteGap(value, 'main', 2.5 * second, 3 * second)).toThrow('exact')
    expect(() => rippleDeleteGap(value, 'main', 8 * second, 9 * second)).toThrow('exact')
    expect(() => rippleDeleteGap(value, 'missing', 0, second)).toThrow('does not exist')
  })

  it('gap closing uses the same lock and inside-hole refusals as clip deletion', () => {
    const value = addClip(document(), 'overlay', clip('inside-gap', 2 * second, second))
    expect(() => rippleDeleteGap(value, 'main', 2 * second, 3 * second)).toThrow('inside')
    const locked = { ...document(), tracks: document().tracks.map((track) => track.id === 'overlay' ? { ...track, locked: true } : track) } as Composition
    expect(() => rippleDeleteGap(locked, 'main', 2 * second, 3 * second)).toThrow('locked')
  })

  it('magnetizes later clips without moving a clip that crosses the anchor', () => {
    const next = magnetizeTrack(document(), 'main', second)
    const main = next.tracks.find((track) => track.id === 'main')!

    expect(main.clips.map((item) => [item.id, item.timelineStartTicks])).toEqual([
      ['a', 0],
      ['b', 2 * second],
      ['c', 4 * second],
    ])
  })

  it('fails closed for locked tracks and invalid anchors', () => {
    const value = document()
    const locked = {
      ...value,
      tracks: value.tracks.map((track) => track.id === 'main' ? { ...track, locked: true } : track),
    } as Composition

    expect(() => rippleDeleteClip(locked, 'a')).toThrow('locked')
    expect(() => magnetizeTrack(value, 'main', -1)).toThrow('safe tick')
  })
})


describe('fractional static ripple landings', () => {
  it('packs consecutive fractional-frame titles using each actual landed endpoint', () => {
    let document = createComposition({ width: 1280, height: 720, fps: 30000 / 1001, fpsRate: { numerator: 30000, denominator: 1001 }, backgroundColor: '#000000' })
    document = addTrack(document, { id: 'titles', kind: 'text', name: 'Titles', hidden: false, locked: false, clips: [] })
    for (const [id, start] of [['a', 0], ['b', 66_733], ['c', 133_467]] as const) document = addClip(document, 'titles', { id, kind: 'text', timelineStartTicks: start, durationTicks: 33_367, text: id, x: 0, y: 0, opacity: 1, style: { fontSizePx: 48, color: '#FFFFFFFF', align: 'center' } })
    const before = JSON.stringify(document)
    const packed = magnetizeTrack(document, 'titles')
    expect(packed.tracks[0]!.clips.map((clip) => clip.timelineStartTicks)).toEqual([0, 33_367, 66_733])
    expect(compositionGaps(packed.tracks[0]!.clips)).toEqual([])
    expect(JSON.stringify(document)).toBe(before)
    expect(magnetizeTrack(packed, 'titles')).toEqual(packed)
  })

  it('closes a frame gap with remeasured title endpoints and leaves protected unmoved content unchanged', () => {
    let document = createComposition({ width: 1280, height: 720, fps: 30000 / 1001, fpsRate: { numerator: 30000, denominator: 1001 }, backgroundColor: '#000000' })
    document = addTrack(document, { id: 'titles', kind: 'text', name: 'Titles', hidden: false, locked: false, clips: [] })
    for (const [id, start, locked] of [['head', 0, true], ['tail', 66_733, false]] as const) document = addClip(document, 'titles', { id, kind: 'text', timelineStartTicks: start, durationTicks: 33_367, locked, text: id, x: 0, y: 0, opacity: 1, style: { fontSizePx: 48, color: '#FFFFFFFF', align: 'center' } })
    const before = JSON.stringify(document)
    const closed = rippleDeleteGap(document, 'titles', 33_367, 66_733)
    expect(closed.tracks[0]!.clips[0]).toEqual(document.tracks[0]!.clips[0])
    expect(closed.tracks[0]!.clips[1]).toMatchObject({ timelineStartTicks: 33_367, durationTicks: 33_366 })
    expect(JSON.stringify(document)).toBe(before)
  })
})


describe('Filmcraft caption sync-lock extraction', () => {
  const cue = (id: string, start: number, end: number) => ({ id, timelineStartTick: start, timelineEndTick: end, text: id, cueId: 'file-' + id, speaker: 'Voice', settings: 'line:90%' })
  it('retains, removes, trims and joins every range boundary without losing metadata', () => {
    for (const [s, e, expected] of [
      [0, 3, [0, 3]], [0, 4, [0, 3]], [0, 7, [0, 5]],
      [3, 5, null], [4, 5, null], [3, 7, [3, 5]], [5, 7, [3, 5]],
    ] as const) {
      const original = cue('cue', s, e)
      const [track] = extractCaptionRanges([{ id: 'caption', name: 'Caption', clips: [original] }], [{ start: 3, end: 5 }])
      expect(track!.clips).toEqual(expected ? [{ ...original, timelineStartTick: expected[0], timelineEndTick: expected[1] }] : [])
      expect(original.timelineStartTick).toBe(s)
      expect(original.timelineEndTick).toBe(e)
    }
  })
  it('closes multiple original-coordinate ranges once', () => {
    const [track] = extractCaptionRanges([{ id: 'caption', name: 'Caption', clips: [cue('span', 0, 10)] }], [{ start: 2, end: 4 }, { start: 6, end: 8 }])
    expect(track!.clips[0]).toEqual(cue('span', 0, 6))
  })
  it('integrates with ordinary clip and gap deletion; preserves locked and opted-out tracks', () => {
    const active = { id: 'captions', name: 'Captions', language: 'fr', vttBlocks: ['NOTE retained'], clips: [cue('caption', 2 * second, 6 * second)] }
    const locked = { ...active, id: 'locked-captions', locked: true, clips: [cue('locked-cue', 2 * second, 6 * second)] }
    const optedOut = { ...active, id: 'free-captions', syncLock: false, clips: [cue('free-cue', 2 * second, 6 * second)] }
    const value = { ...document(), captionTracks: [active, locked, optedOut] }
    const snapshot = JSON.stringify(value)
    const result = rippleDeleteClip(value, 'b')
    expect(result.captionTracks![0]).toEqual({ ...active, clips: [cue('caption', 2 * second, 4 * second)] })
    expect(result.captionTracks![1]).toBe(locked)
    expect(result.captionTracks![2]).toBe(optedOut)
    expect(JSON.stringify(value)).toBe(snapshot)
    const gap = rippleDeleteGap(value, 'main', 2 * second, 3 * second)
    expect(gap.captionTracks![0]!.clips).toEqual([cue('caption', 2 * second, 5 * second)])
  })
})


it('matches independently executed pinned Filmcraft global extraction', () => {
 const adapt = (tracks: typeof rippleOracle.cases[number]['before']) => tracks.map(track=>({
  id:String(track.id),name:track.name,locked:track.locked,syncLock:track.sync_lock,hidden:!track.enabled,language:track.language,vttBlocks:track.vtt_blocks,
  clips:track.captions.map(cue=>({id:String(cue.id),cueId:cue.cue_id,text:cue.text,speaker:cue.speaker,settings:cue.settings,timelineStartTick:cue.start,timelineEndTick:cue.start+cue.duration})),
 }))
 expect(rippleOracle.commit).toBe('5231852443363f001c3f6b396dd9b1e6461ae2be')
 for(const scenario of rippleOracle.cases){
  const actual=extractCaptionRanges(adapt(scenario.before),scenario.holes.map(([start,end])=>({start:start!,end:end!})))
  expect(actual,scenario.name).toEqual(adapt(scenario.after))
 }
})

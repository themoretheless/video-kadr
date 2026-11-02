import { describe, expect, it } from 'vitest'
import { addClip, addTrack, createComposition, moveClipsByTrackOffset } from './commands'

function fixture() {
  let document = createComposition({ width: 1280, height: 720, fps: 30, backgroundColor: '#000000' })
  for (const id of ['a', 'b', 'c', 'hidden']) document = addTrack(document, { id, kind: 'text', name: id, hidden: false, locked: false, clips: [] })
  for (const [trackId, id, start] of [['a', 'first', 0], ['b', 'second', 1_000_000], ['hidden', 'partner', 2_000_000]] as const) document = addClip(document, trackId, { id, kind: 'text', timelineStartTicks: start, durationTicks: 100_000, text: id, x: 0, y: 0, opacity: 1, style: { fontSizePx: 48, color: '#FFFFFFFF', align: 'center' } })
  return document
}

describe('displayed lane offsets', () => {
  it('moves the visible block by one lane and retains hidden partner lanes and timing offsets', () => {
    const document = fixture()
    const moved = moveClipsByTrackOffset(document, ['second', 'partner', 'first'], 'first', 'b', 1_000_000, ['a', 'b', 'c'])
    expect(moved.tracks.map((track) => [track.id, track.clips.map((clip) => [clip.id, clip.timelineStartTicks])])).toEqual([
      ['a', []], ['b', [['first', 1_000_000]]], ['c', [['second', 2_000_000]]], ['hidden', [['partner', 3_000_000]]],
    ])
    expect(document.tracks[0]!.clips[0]!.timelineStartTicks).toBe(0)
  })

  it('refuses the complete block for out-of-range, locked or occupied destinations', () => {
    const document = fixture()
    const before = JSON.stringify(document)
    expect(() => moveClipsByTrackOffset(document, ['first', 'second'], 'first', 'c', 0, ['a', 'b', 'c'])).toThrow('exceeds')
    const locked = { ...document, tracks: document.tracks.map((track) => track.id === 'c' ? { ...track, locked: true } : track) }
    expect(() => moveClipsByTrackOffset(locked, ['first', 'second'], 'first', 'b', 0, ['a', 'b', 'c'])).toThrow('locked')
    const occupied = addClip(document, 'c', { ...document.tracks[0]!.clips[0]!, id: 'obstacle', timelineStartTicks: 1_000_000 })
    expect(() => moveClipsByTrackOffset(occupied, ['first', 'second'], 'first', 'b', 0, ['a', 'b', 'c'])).toThrow()
    expect(JSON.stringify(document)).toBe(before)
    expect(() => moveClipsByTrackOffset(document, ['first'], 'first', 'b', 0, ['a', 'b', 'b'])).toThrow('distinct')
  })
})

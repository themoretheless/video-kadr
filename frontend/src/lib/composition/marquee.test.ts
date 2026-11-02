import { describe, expect, it } from 'vitest'
import { addClip, addTrack, createComposition, setClipLocked } from './commands'
import { createCompositionLink } from './links'
import { marqueeRect, marqueeScrollStep, selectMarqueeClips, selectMarqueeKeyframes } from './marquee'

function fixture() {
  let document = createComposition({ width: 1280, height: 720, fps: 30, backgroundColor: '#000000' })
  for (const id of ['a', 'b']) {
    document = addTrack(document, { id: id + '-lane', kind: 'text', name: id, hidden: false, locked: false, clips: [] })
    document = addClip(document, id + '-lane', { id, kind: 'text', timelineStartTicks: 0, durationTicks: 1_000_000, text: id, x: 0, y: 0, opacity: 1, style: { fontSizePx: 48, color: '#FFFFFFFF', align: 'center' } })
  }
  return document
}
const bounds = [{ clipId: 'a', trackId: 'a-lane', left: -1000, right: 1000, top: 0, bottom: 50 }, { clipId: 'b', trackId: 'b-lane', left: 0, right: 100, top: 64, bottom: 114 }]

describe('clip marquee semantics', () => {
  it('intersects long clips, respects rendered bounds and releases subjects when the box shrinks', () => {
    const document = fixture()
    expect(selectMarqueeClips(document, marqueeRect(10, 10, 90, 100), bounds, 'b')).toMatchObject({ ids: ['a', 'b'], primaryId: 'b' })
    expect(selectMarqueeClips(document, marqueeRect(10, 10, 90, 40), bounds, 'b')).toMatchObject({ ids: ['a'], primaryId: 'a' })
    expect(selectMarqueeClips(document, marqueeRect(10, 10, 90, 100), bounds.slice(0, 1), null).ids).toEqual(['a'])
    expect(selectMarqueeClips(document, marqueeRect(10, 10, 10, 100), bounds, null).ids).toEqual([])
  })
  it('excludes locked direct hits but expands an unlocked hit to the whole link including locked partners', () => {
    let document = setClipLocked(fixture(), 'b', true)
    expect(selectMarqueeClips(document, marqueeRect(10, 65, 90, 100), bounds, null).ids).toEqual([])
    document = createCompositionLink(setClipLocked(document, 'b', false), 'link', ['a', 'b'])
    document = setClipLocked(document, 'b', true)
    const before = JSON.stringify(document)
    expect(selectMarqueeClips(document, marqueeRect(10, 10, 90, 40), bounds, 'b')).toMatchObject({ ids: ['a', 'b'], primaryId: 'b', trackId: 'b-lane' })
    expect(selectMarqueeClips(document, marqueeRect(10, 10, 90, 40), bounds, null, false).ids).toEqual(['a'])
    expect(JSON.stringify(document)).toBe(before)
  })
  it('normalizes reverse sweeps and computes bounded horizontal edge scrolling', () => {
    expect(marqueeRect(90, 100, 10, 20)).toEqual({ left: 10, top: 20, right: 90, bottom: 100 })
    expect(() => marqueeRect(NaN, 0, 1, 1)).toThrow()
    expect(marqueeScrollStep(50, 0, 100)).toBe(0)
    expect(marqueeScrollStep(5, 0, 100)).toBe(-18)
    expect(marqueeScrollStep(90, 0, 100)).toBe(14)
  })
})

it('tests key centres on expanded rows, x only on collapsed rows, and rebuilds the exact unlocked set', () => {
  const keys = [
    { channelId: 'x', tick: 1, x: 100, y: 112, laneTop: 100, laneBottom: 124, expanded: false, locked: false },
    { channelId: 'y', tick: 2, x: 200, y: 232, laneTop: 220, laneBottom: 244, expanded: true, locked: false },
    { channelId: 'locked', tick: 3, x: 150, y: 112, laneTop: 100, laneBottom: 124, expanded: false, locked: true },
  ]
  expect(selectMarqueeKeyframes(marqueeRect(50, 118, 300, 235), keys)).toEqual([{ channelId: 'x', ticks: [1] }, { channelId: 'y', ticks: [2] }])
  expect(selectMarqueeKeyframes(marqueeRect(50, 118, 300, 230), keys)).toEqual([{ channelId: 'x', ticks: [1] }])
  expect(selectMarqueeKeyframes(marqueeRect(150, 118, 300, 235), keys)).toEqual([{ channelId: 'y', ticks: [2] }])
  expect(selectMarqueeKeyframes(marqueeRect(50, 118, 300, 118), keys)).toEqual([{ channelId: 'x', ticks: [1] }])
})

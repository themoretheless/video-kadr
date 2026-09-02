import { describe, expect, it } from 'vitest'
import { captureScrollAnchor, restoreScrollAnchor, timelineViewport, visibleRulerMarks, visibleTimelineItems } from './timelineUi.js'

describe('timeline virtualization', () => {
  it('renders a bounded 10k clip window and stable ruler range', () => {
    const clips = Array.from({ length: 10_000 }, (_, index) => ({ id: `clip-${index}`, left: index * 50, right: index * 50 + 48 }))
    const viewport = timelineViewport(250_000, 1_000, 500_000)
    const visible = visibleTimelineItems(clips, viewport, (clip) => clip.left, (clip) => clip.right)
    expect(visible.length).toBeLessThan(40)
    expect(visible[0]!.id).toBe('clip-4991')
    expect(visibleRulerMarks(viewport, 50).length).toBeLessThan(40)
  })

  it('preserves the same timeline anchor across zoom changes', () => {
    const anchor = captureScrollAnchor(1_180, 100, 1_000_000)
    expect(anchor.tick).toBe(10_000_000)
    expect(restoreScrollAnchor(anchor, 200, 1_000_000)).toBe(2_180)
  })
})

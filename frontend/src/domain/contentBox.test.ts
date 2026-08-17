import { describe, expect, it } from 'vitest'
import {
  boxDeltaToNormalized,
  boxToNormalized,
  contentBox,
  normalizedToBox,
  overlayPlacement,
  rotationOffset,
  sourcePixelScale,
  titlePlacement,
} from './contentBox'

const ELEMENT = { width: 800, height: 450 }

describe('contentBox', () => {
  it('is the element box when the aspect ratios already match', () => {
    expect(contentBox(ELEMENT, { width: 1920, height: 1080 })).toEqual({
      left: 0,
      top: 0,
      width: 800,
      height: 450,
    })
  })

  it('adds horizontal bars for a source narrower than its element', () => {
    // A 9:16 source in a 16:9 element: height fills, width is 450 * 9/16.
    const box = contentBox(ELEMENT, { width: 1080, height: 1920 })
    expect(box.height).toBeCloseTo(450)
    expect(box.width).toBeCloseTo(253.125)
    expect(box.left).toBeCloseTo((800 - 253.125) / 2)
    expect(box.top).toBeCloseTo(0)
  })

  it('adds vertical bars for a source wider than its element', () => {
    const box = contentBox({ width: 800, height: 800 }, { width: 1920, height: 1080 })
    expect(box.width).toBeCloseTo(800)
    expect(box.height).toBeCloseTo(450)
    expect(box.top).toBeCloseTo(175)
    expect(box.left).toBeCloseTo(0)
  })

  it('degrades to the element box for an unknown or degenerate source', () => {
    const full = { left: 0, top: 0, width: 800, height: 450 }
    expect(contentBox(ELEMENT, null)).toEqual(full)
    expect(contentBox(ELEMENT, { width: 0, height: 1080 })).toEqual(full)
    expect(contentBox(ELEMENT, { width: Number.NaN, height: 1080 })).toEqual(full)
  })

  it('never produces a non-finite box from a non-finite element', () => {
    const box = contentBox({ width: Number.POSITIVE_INFINITY, height: -5 }, ELEMENT)
    expect(Number.isFinite(box.width)).toBe(true)
    expect(Number.isFinite(box.height)).toBe(true)
  })
})

describe('normalized mapping', () => {
  // The letterboxed case is the one RectOverlay.vue gets wrong: the centre of
  // the frame must land on the centre of the CONTENT, not of the element.
  const box = contentBox(ELEMENT, { width: 1080, height: 1920 })

  it('maps the frame centre onto the content centre, not the element centre', () => {
    const { left, top } = normalizedToBox(box, 0.5, 0.5)
    expect(left).toBeCloseTo(400)
    expect(top).toBeCloseTo(225)
    // The frame's left edge sits on the letterbox bar, not at x = 0.
    expect(normalizedToBox(box, 0, 0).left).toBeCloseTo(box.left)
  })

  it('round-trips a point through both directions', () => {
    const { left, top } = normalizedToBox(box, 0.25, 0.8)
    const back = boxToNormalized(box, left, top)
    expect(back.x).toBeCloseTo(0.25)
    expect(back.y).toBeCloseTo(0.8)
  })

  it('scales a pointer delta by the content size', () => {
    const delta = boxDeltaToNormalized(box, box.width / 4, box.height / 2)
    expect(delta.x).toBeCloseTo(0.25)
    expect(delta.y).toBeCloseTo(0.5)
  })

  it('returns zero instead of dividing by an unmeasured box', () => {
    const empty = { left: 0, top: 0, width: 0, height: 0 }
    expect(boxToNormalized(empty, 10, 10)).toEqual({ x: 0, y: 0 })
    expect(boxDeltaToNormalized(empty, 10, 10)).toEqual({ x: 0, y: 0 })
  })
})

describe('sourcePixelScale', () => {
  it('converts source pixels into preview pixels', () => {
    const box = contentBox(ELEMENT, { width: 1920, height: 1080 })
    expect(sourcePixelScale(box, { width: 1920, height: 1080 })).toBeCloseTo(450 / 1080)
  })

  it('falls back to 1:1 when nothing has been measured', () => {
    expect(sourcePixelScale({ left: 0, top: 0, width: 0, height: 0 }, null)).toBe(1)
  })
})

describe('rotationOffset', () => {
  it('is zero without rotation', () => {
    expect(rotationOffset(100, 50, 0)).toEqual({ x: 0, y: 0 })
  })

  it('matches the growth FFmpeg rotw/roth produce at 90 degrees', () => {
    // A 100x50 layer rotated a quarter turn becomes 50x100.
    const offset = rotationOffset(100, 50, 90)
    expect(offset.x).toBeCloseTo((50 - 100) / 2)
    expect(offset.y).toBeCloseTo((100 - 50) / 2)
  })

  it('is symmetric in the sign of the angle', () => {
    expect(rotationOffset(80, 40, -30)).toEqual(rotationOffset(80, 40, 30))
  })
})

describe('overlayPlacement', () => {
  const box = contentBox(ELEMENT, { width: 1920, height: 1080 })

  it('places a sized overlay at its normalized top-left corner', () => {
    const placement = overlayPlacement(box, { x: 0.05, y: 0.1, width: 0.25, height: 0.25 }, null)
    expect(placement.left).toBeCloseTo(40)
    expect(placement.top).toBeCloseTo(45)
    expect(placement.width).toBeCloseTo(200)
    expect(placement.height).toBeCloseTo(112.5)
  })

  it('derives the height from the asset aspect when the wire height is null', () => {
    const placement = overlayPlacement(box, { x: 0, y: 0, width: 0.5, height: null }, 2)
    expect(placement.width).toBeCloseTo(400)
    expect(placement.height).toBeCloseTo(200)
  })

  it('falls back to the frame aspect when the asset size is unknown', () => {
    const placement = overlayPlacement(box, { x: 0, y: 0, width: 0.5, height: null }, null)
    expect(placement.height).toBeCloseTo(placement.width * (box.height / box.width))
  })

  it('shifts a rotated layer so its bounding box keeps the anchor corner', () => {
    const upright = overlayPlacement(box, { x: 0.1, y: 0.1, width: 0.25, height: 0.25 }, null)
    const turned = overlayPlacement(
      box,
      { x: 0.1, y: 0.1, width: 0.25, height: 0.25, rotation: 90 },
      null,
    )
    expect(turned.left - upright.left).toBeCloseTo((upright.height - upright.width) / 2)
    expect(turned.top - upright.top).toBeCloseTo((upright.width - upright.height) / 2)
  })

  it('positions a letterboxed overlay against the content box', () => {
    const narrow = contentBox(ELEMENT, { width: 1080, height: 1920 })
    const placement = overlayPlacement(narrow, { x: 0, y: 0, width: 1, height: 1 }, null)
    expect(placement.left).toBeCloseTo(narrow.left)
    expect(placement.width).toBeCloseTo(narrow.width)
  })
})

describe('titlePlacement', () => {
  const source = { width: 1920, height: 1080 }
  const box = contentBox(ELEMENT, source)

  it('scales the 1080p font size down to the content height', () => {
    const placement = titlePlacement(box, source, { x: 0.5, y: 0.85, align: 'center', fontSize: 48 })
    // 48px at 1080 tall becomes 48 * 450 / 1080 in a 450px tall preview.
    expect(placement.fontSizePx).toBeCloseTo(20)
  })

  it('gives the same preview size regardless of the source resolution', () => {
    const small = { width: 640, height: 360 }
    const smallBox = contentBox(ELEMENT, small)
    const a = titlePlacement(box, source, { x: 0.5, y: 0.5, align: 'center', fontSize: 48 })
    const b = titlePlacement(smallBox, small, { x: 0.5, y: 0.5, align: 'center', fontSize: 48 })
    expect(a.fontSizePx).toBeCloseTo(b.fontSizePx)
  })

  it('turns the alignment into the anchor fraction drawtext uses', () => {
    const at = (align: 'left' | 'center' | 'right') =>
      titlePlacement(box, source, { x: 0.5, y: 0.5, align, fontSize: 48 }).anchorX
    expect(at('left')).toBe(0)
    expect(at('center')).toBe(0.5)
    expect(at('right')).toBe(1)
  })

  it('converts the source-pixel box, border and shadow options', () => {
    const placement = titlePlacement(box, source, {
      x: 0.5,
      y: 0.5,
      align: 'center',
      fontSize: 48,
      box: { padding: 12 },
      borderWidth: 4,
      shadowX: 2,
      shadowY: -6,
    })
    const scale = 450 / 1080
    expect(placement.paddingPx).toBeCloseTo(12 * scale)
    expect(placement.borderPx).toBeCloseTo(4 * scale)
    expect(placement.shadowXPx).toBeCloseTo(2 * scale)
    expect(placement.shadowYPx).toBeCloseTo(-6 * scale)
  })

  it('anchors against the content box on a letterboxed source', () => {
    const narrow = contentBox(ELEMENT, { width: 1080, height: 1920 })
    const placement = titlePlacement(narrow, { width: 1080, height: 1920 }, {
      x: 0,
      y: 0.5,
      align: 'left',
      fontSize: 48,
    })
    expect(placement.left).toBeCloseTo(narrow.left)
    expect(placement.left).toBeGreaterThan(0)
  })
})

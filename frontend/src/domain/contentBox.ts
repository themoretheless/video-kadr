// Geometry of the real video content box inside a player element.
//
// A `<video>` element letterboxes its source: the element box is whatever CSS
// gave it, while the decoded frame is centred inside it with `object-fit:
// contain`. Overlay coordinates on the wire are normalized against the OUTPUT
// FRAME, so every preview placement has to be computed against the content box
// and never against the element box. RectOverlay.vue does the latter and is
// therefore wrong on any source whose aspect ratio differs from its element;
// nothing here repeats that.
//
// Everything in this file is pure and framework-free, so the mapping that
// decides "what the user drags is what renders" is unit-testable on its own.

import type { TitleAlign } from '../types'

export interface Size {
  width: number
  height: number
}

/**
 * The real video pixels inside an element, in CSS pixels relative to that
 * element's own box. `left`/`top` are the letterbox bars.
 */
export interface ContentBox {
  left: number
  top: number
  width: number
  height: number
}

/** Design frame the backend authors title sizes against (`titles.rs`). */
export const TITLE_REFERENCE_HEIGHT = 1080

/** Below this a dimension is treated as "not measured yet". */
const MIN_DIMENSION = 1e-6

function finite(value: number): number {
  return Number.isFinite(value) ? value : 0
}

/**
 * Content box of a source rendered with `object-fit: contain` inside an
 * element. An unmeasured element or an unknown source degrades to the element
 * box, which is exactly the (correct) identity case for a full-bleed frame.
 */
export function contentBox(element: Size, source: Size | null): ContentBox {
  const elementWidth = Math.max(0, finite(element.width))
  const elementHeight = Math.max(0, finite(element.height))
  const full = { left: 0, top: 0, width: elementWidth, height: elementHeight }
  if (!source) return full
  const sourceWidth = Math.max(0, finite(source.width))
  const sourceHeight = Math.max(0, finite(source.height))
  if (
    sourceWidth < MIN_DIMENSION ||
    sourceHeight < MIN_DIMENSION ||
    elementWidth < MIN_DIMENSION ||
    elementHeight < MIN_DIMENSION
  ) {
    return full
  }
  const scale = Math.min(elementWidth / sourceWidth, elementHeight / sourceHeight)
  const width = sourceWidth * scale
  const height = sourceHeight * scale
  return {
    left: (elementWidth - width) / 2,
    top: (elementHeight - height) / 2,
    width,
    height,
  }
}

/**
 * Preview pixels per source pixel. Filter options the backend leaves in source
 * pixels (drawtext `boxborderw`, `borderw`, `shadowx`) go through this.
 */
export function sourcePixelScale(box: ContentBox, source: Size | null): number {
  const sourceHeight = source ? Math.max(0, finite(source.height)) : 0
  if (sourceHeight < MIN_DIMENSION || box.height < MIN_DIMENSION) return 1
  return box.height / sourceHeight
}

/** Normalized frame coordinate to CSS pixels inside the element box. */
export function normalizedToBox(box: ContentBox, x: number, y: number): { left: number; top: number } {
  return {
    left: box.left + finite(x) * box.width,
    top: box.top + finite(y) * box.height,
  }
}

/** CSS pixels inside the element box back to a normalized frame coordinate. */
export function boxToNormalized(box: ContentBox, left: number, top: number): { x: number; y: number } {
  return {
    x: box.width < MIN_DIMENSION ? 0 : (finite(left) - box.left) / box.width,
    y: box.height < MIN_DIMENSION ? 0 : (finite(top) - box.top) / box.height,
  }
}

/** A normalized delta for a pointer delta measured in CSS pixels. */
export function boxDeltaToNormalized(
  box: ContentBox,
  deltaX: number,
  deltaY: number,
): { x: number; y: number } {
  return {
    x: box.width < MIN_DIMENSION ? 0 : finite(deltaX) / box.width,
    y: box.height < MIN_DIMENSION ? 0 : finite(deltaY) / box.height,
  }
}

/**
 * How far a CSS-rotated element has to move so that its rotated bounding box
 * keeps the same top-left corner.
 *
 * `overlays.rs` rotates a layer with `ow=rotw(a):oh=roth(a)` and then composites
 * the GROWN layer at the overlay's `x`/`y`. CSS instead rotates about the
 * element centre, which keeps the centre and grows the bounding box in every
 * direction. Adding this offset makes the two agree.
 */
export function rotationOffset(
  width: number,
  height: number,
  degrees: number,
): { x: number; y: number } {
  const radians = (finite(degrees) * Math.PI) / 180
  const cos = Math.abs(Math.cos(radians))
  const sin = Math.abs(Math.sin(radians))
  const rotatedWidth = finite(width) * cos + finite(height) * sin
  const rotatedHeight = finite(width) * sin + finite(height) * cos
  return {
    x: (rotatedWidth - finite(width)) / 2,
    y: (rotatedHeight - finite(height)) / 2,
  }
}

/** Structural shape of the overlay fields placement depends on. */
export interface OverlayGeometry {
  x: number
  y: number
  width: number
  height?: number | null
  rotation?: number
}

export interface OverlayPlacement {
  /** CSS pixels inside the element box, rotation offset already applied. */
  left: number
  top: number
  width: number
  height: number
  rotation: number
}

/**
 * Where an overlay layer sits in the preview.
 *
 * A null wire height means "keep the asset's aspect ratio", which is what the
 * backend's `scale=W:-1` does. `naturalAspect` is the asset's width/height; when
 * it is unknown the layer falls back to the frame's own aspect so the box is
 * still grabbable instead of collapsing to nothing.
 */
export function overlayPlacement(
  box: ContentBox,
  overlay: OverlayGeometry,
  naturalAspect: number | null,
): OverlayPlacement {
  const width = Math.max(0, finite(overlay.width)) * box.width
  const explicitHeight = typeof overlay.height === 'number' ? overlay.height : null
  let height: number
  if (explicitHeight !== null && Number.isFinite(explicitHeight)) {
    height = Math.max(0, explicitHeight) * box.height
  } else {
    const aspect =
      naturalAspect !== null && Number.isFinite(naturalAspect) && naturalAspect > MIN_DIMENSION
        ? naturalAspect
        : box.height < MIN_DIMENSION
          ? 1
          : box.width / box.height
    height = width / aspect
  }
  const rotation = finite(overlay.rotation ?? 0)
  const offset = rotationOffset(width, height, rotation)
  const anchor = normalizedToBox(box, overlay.x, overlay.y)
  return {
    left: anchor.left + offset.x,
    top: anchor.top + offset.y,
    width,
    height,
    rotation,
  }
}

/** Structural shape of the title fields placement depends on. */
export interface TitleGeometry {
  x: number
  y: number
  align: TitleAlign
  fontSize: number
  box?: { padding: number } | null
  borderWidth?: number
  shadowX?: number
  shadowY?: number
}

export interface TitlePlacement {
  /** Anchor point in CSS pixels inside the element box. */
  left: number
  top: number
  /**
   * Fraction of the text width that sits left of the anchor: 0 for a left
   * aligned title, 0.5 for a centred one, 1 for a right aligned one. The
   * vertical anchor is always the middle of the text, as `drawtext` gets
   * `y=h*Y-text_h/2`.
   */
  anchorX: number
  fontSizePx: number
  paddingPx: number
  borderPx: number
  shadowXPx: number
  shadowYPx: number
}

const TITLE_ANCHORS: Record<TitleAlign, number> = { left: 0, center: 0.5, right: 1 }

/**
 * Where a drawtext title sits in the preview.
 *
 * `fontSize` is authored at 1080p and scaled by the backend to the source
 * height, then the export resize carries it along, so in preview pixels the
 * source height cancels out and only the content box height is left. Options the
 * backend passes through in source pixels do need the source scale.
 */
export function titlePlacement(
  box: ContentBox,
  source: Size | null,
  title: TitleGeometry,
): TitlePlacement {
  const scale = sourcePixelScale(box, source)
  const anchor = normalizedToBox(box, title.x, title.y)
  return {
    left: anchor.left,
    top: anchor.top,
    anchorX: TITLE_ANCHORS[title.align] ?? 0.5,
    fontSizePx: (Math.max(0, finite(title.fontSize)) * box.height) / TITLE_REFERENCE_HEIGHT,
    paddingPx: Math.max(0, finite(title.box?.padding ?? 0)) * scale,
    borderPx: Math.max(0, finite(title.borderWidth ?? 0)) * scale,
    shadowXPx: finite(title.shadowX ?? 0) * scale,
    shadowYPx: finite(title.shadowY ?? 0) * scale,
  }
}

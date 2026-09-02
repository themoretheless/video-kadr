export const TIMELINE_TRACK_HEIGHT_PX = 64
export const TIMELINE_HEADER_WIDTH_PX = 180
export const TIMELINE_OVERSCAN_PX = 240

export interface TimelineViewport {
  startPx: number
  endPx: number
}

export function timelineViewport(scrollLeft: number, clientWidth: number, contentWidth: number): TimelineViewport {
  const startPx = Math.max(0, finite(scrollLeft) - TIMELINE_HEADER_WIDTH_PX - TIMELINE_OVERSCAN_PX)
  const endPx = Math.min(
    Math.max(0, finite(contentWidth)),
    Math.max(startPx, finite(scrollLeft) + Math.max(0, finite(clientWidth)) + TIMELINE_OVERSCAN_PX),
  )
  return { startPx, endPx }
}

export function visibleTimelineItems<T>(
  items: readonly T[],
  viewport: TimelineViewport,
  leftPx: (item: T) => number,
  rightPx: (item: T) => number,
): readonly T[] {
  return items.filter((item) => rightPx(item) >= viewport.startPx && leftPx(item) <= viewport.endPx)
}

export function visibleRulerMarks(viewport: TimelineViewport, zoomPxPerSecond: number): readonly number[] {
  const zoom = Math.max(1, finite(zoomPxPerSecond))
  const first = Math.max(0, Math.floor(viewport.startPx / zoom))
  const last = Math.max(first, Math.ceil(viewport.endPx / zoom))
  return Array.from({ length: last - first + 1 }, (_, index) => first + index)
}

export interface ScrollAnchor {
  tick: number
  viewportOffsetPx: number
}

export function captureScrollAnchor(scrollLeft: number, zoomPxPerSecond: number, timeBase: number): ScrollAnchor {
  const laneScroll = Math.max(0, finite(scrollLeft) - TIMELINE_HEADER_WIDTH_PX)
  return {
    tick: Math.round((laneScroll / Math.max(1, finite(zoomPxPerSecond))) * timeBase),
    viewportOffsetPx: Math.min(TIMELINE_HEADER_WIDTH_PX, Math.max(0, finite(scrollLeft))),
  }
}

export function restoreScrollAnchor(anchor: ScrollAnchor, zoomPxPerSecond: number, timeBase: number): number {
  return Math.max(0, (anchor.tick / timeBase) * Math.max(1, finite(zoomPxPerSecond)) + anchor.viewportOffsetPx)
}

function finite(value: number): number {
  return Number.isFinite(value) ? value : 0
}

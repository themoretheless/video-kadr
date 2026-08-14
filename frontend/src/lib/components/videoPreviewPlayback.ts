export interface PreviewTimelineSegment { start: number; end: number }
export interface PreviewPlaybackCursor { index: number; seekTo: number | null }

export function remapOrderedTimelineCursor(
  segments: readonly { id: string }[],
  currentId: string,
  currentIndex: number,
): number {
  if (!segments.length) return 0
  const idIndex = segments.findIndex((segment) => segment.id === currentId)
  if (idIndex >= 0) return idIndex
  const fallback = Number.isInteger(currentIndex) ? currentIndex : 0
  return Math.max(0, Math.min(fallback, segments.length - 1))
}

function containsTime(segment: PreviewTimelineSegment, time: number): boolean {
  return time >= segment.start && time < segment.end
}

export function enterOrderedTimeline(
  segments: readonly PreviewTimelineSegment[],
  currentTime: number,
  preferredIndex: number | null = null,
): PreviewPlaybackCursor {
  if (!segments.length) return { index: 0, seekTo: null }
  if (Number.isFinite(currentTime)) {
    if (preferredIndex != null && Number.isInteger(preferredIndex)) {
      const preferred = segments[preferredIndex]
      if (preferred) return containsTime(preferred, currentTime)
        ? { index: preferredIndex, seekTo: null }
        : { index: preferredIndex, seekTo: preferred.start }
    }
    const containingIndex = segments.findIndex((segment) => containsTime(segment, currentTime))
    if (containingIndex >= 0) return { index: containingIndex, seekTo: null }
  }
  return { index: 0, seekTo: segments[0]!.start }
}

export function stepOrderedTimeline(
  segments: readonly PreviewTimelineSegment[],
  currentTime: number,
  currentIndex: number,
): PreviewPlaybackCursor {
  if (!segments.length) return { index: 0, seekTo: null }
  if (!Number.isInteger(currentIndex) || currentIndex < 0 || currentIndex >= segments.length) {
    return enterOrderedTimeline(segments, currentTime)
  }
  const current = segments[currentIndex]!
  if (!Number.isFinite(currentTime)) return { index: currentIndex, seekTo: current.start }
  if (currentTime >= current.end) {
    const nextIndex = (currentIndex + 1) % segments.length
    return { index: nextIndex, seekTo: segments[nextIndex]!.start }
  }
  if (currentTime < current.start) {
    const containingIndex = segments.findIndex((segment) => containsTime(segment, currentTime))
    if (containingIndex >= 0) return { index: containingIndex, seekTo: null }
    return { index: currentIndex, seekTo: current.start }
  }
  return { index: currentIndex, seekTo: null }
}

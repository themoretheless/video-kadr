// Snapping for timeline drags. Pure functions: the panel collects the candidate
// times (playhead, clip edges, markers) and asks for the nearest one inside a
// pixel tolerance converted to seconds.

export type SnapKind = 'playhead' | 'clip' | 'marker' | 'edge'

export interface SnapTarget {
  t: number
  kind: SnapKind
}

export interface SnapResult {
  /** The snapped time, or the input when nothing was close enough. */
  t: number
  /** The target that captured the drag, for the visible snap indicator. */
  hit: SnapTarget | null
}

/**
 * Nearest target within `tolerance` seconds. Ties keep the earlier candidate so
 * the result never depends on the order the panel happened to collect them in.
 */
export function snapTime(t: number, targets: readonly SnapTarget[], tolerance: number): SnapResult {
  if (!Number.isFinite(t)) return { t: 0, hit: null }
  if (!Number.isFinite(tolerance) || tolerance <= 0) return { t, hit: null }
  let best: SnapTarget | null = null
  let bestDistance = tolerance
  for (const target of targets) {
    if (!Number.isFinite(target.t)) continue
    const distance = Math.abs(target.t - t)
    if (distance < bestDistance) {
      best = target
      bestDistance = distance
    }
  }
  return best ? { t: best.t, hit: best } : { t, hit: null }
}

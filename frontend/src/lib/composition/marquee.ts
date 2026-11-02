import { linkedClipIds } from './links'
import type { Composition } from './types'

export interface MarqueeRect { left: number; top: number; right: number; bottom: number }
export interface MarqueeClipBounds extends MarqueeRect { clipId: string; trackId: string }

export function marqueeRect(x1: number, y1: number, x2: number, y2: number): MarqueeRect {
  if (![x1, y1, x2, y2].every(Number.isFinite)) throw new Error('Invalid marquee coordinates')
  return { left: Math.min(x1, x2), top: Math.min(y1, y2), right: Math.max(x1, x2), bottom: Math.max(y1, y2) }
}

/** Bounds come from displayed lanes; selection is rebuilt rather than accumulated. */
export function selectMarqueeClips(document: Composition, rect: MarqueeRect, bounds: readonly MarqueeClipBounds[], primaryId: string | null, expandLinks = true): { ids: string[]; primaryId: string | null; trackId: string | null } {
  rect = marqueeRect(rect.left, rect.top, rect.right, rect.bottom)
  const selected = new Set<string>()
  for (const bound of bounds) {
    if (rect.right === rect.left || rect.bottom === rect.top || ![bound.left, bound.top, bound.right, bound.bottom].every(Number.isFinite) || bound.right <= bound.left || bound.bottom <= bound.top) continue
    const track = document.tracks.find((track) => track.id === bound.trackId)
    const clip = track?.clips.find((clip) => clip.id === bound.clipId)
    if (!clip || track?.locked || clip.locked) continue
    if (bound.right <= rect.left || bound.left >= rect.right || bound.bottom <= rect.top || bound.top >= rect.bottom) continue
    for (const id of expandLinks ? linkedClipIds(document, clip.id) : [clip.id]) selected.add(id)
  }
  const ids = [...selected]
  const primary = primaryId && selected.has(primaryId) ? primaryId : ids[0] ?? null
  return { ids, primaryId: primary, trackId: primary ? document.tracks.find((track) => track.clips.some((clip) => clip.id === primary))!.id : null }
}

/** Horizontal edge scrolling only; callers preserve the press in content coordinates. */
export function marqueeScrollStep(pointerX: number, left: number, right: number): number {
  const edge = 24
  if (pointerX < left + edge) return -Math.min(18, Math.max(0, left + edge - pointerX))
  if (pointerX > right - edge) return Math.min(18, Math.max(0, pointerX - right + edge))
  return 0
}

export interface MarqueeKeyframeBounds { channelId: string; tick: number; x: number; y: number; laneTop: number; laneBottom: number; expanded: boolean; locked: boolean }
/** Collapsed rows test x only within the swept lane; expanded rows test drawn centres on both axes. */
export function selectMarqueeKeyframes(rect: MarqueeRect, bounds: readonly MarqueeKeyframeBounds[]): { channelId: string; ticks: number[] }[] {
  rect = marqueeRect(rect.left, rect.top, rect.right, rect.bottom)
  if (rect.left === rect.right && rect.top === rect.bottom) return []
  const selected = new Map<string, Set<number>>()
  for (const key of bounds) {
    if (key.locked || ![key.x, key.y, key.laneTop, key.laneBottom, key.tick].every(Number.isFinite)) continue
    if (key.x < rect.left || key.x > rect.right || key.laneBottom < rect.top || key.laneTop > rect.bottom) continue
    if (key.expanded && (key.y < rect.top || key.y > rect.bottom)) continue
    const ticks = selected.get(key.channelId) ?? new Set<number>()
    ticks.add(key.tick)
    selected.set(key.channelId, ticks)
  }
  return [...selected].map(([channelId, ticks]) => ({ channelId, ticks: [...ticks] }))
}

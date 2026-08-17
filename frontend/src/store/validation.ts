// Shared sanitizers for the feature store modules. Snapshot restore reads
// untrusted JSON (localStorage, the projects API, an older client), so every
// value crosses this file before it reaches module state: numbers are
// finite-checked and range-clamped, strings length-capped, collections
// size-capped. Nothing here ever casts an unknown through `as`.

import type { Interpolation, Keyframe, KeyframeTrack } from '../types'

/** Caps from the feature contract, section 0. */
export const MAX_CLIPS = 200
export const MAX_OVERLAYS = 32
export const MAX_TITLES = 32
export const MAX_AUDIO_TRACKS = 8
export const MAX_KEYFRAMES = 64
export const MAX_TEXT_LENGTH = 512

/** Asset ids are opaque; the same allow-list the backend store enforces. */
const ASSET_ID = /^ast_[a-zA-Z0-9]{16,}$/
const HEX_COLOR = /^#[0-9a-fA-F]{6}$/

const INTERPOLATIONS: readonly Interpolation[] = ['hold', 'linear', 'smooth']

export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

/** A finite number or the fallback. Rejects NaN, Infinity and non-numbers. */
export function finiteOr(value: unknown, fallback: number): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : fallback
}

export function clampNumber(value: unknown, min: number, max: number, fallback: number): number {
  return Math.max(min, Math.min(max, finiteOr(value, fallback)))
}

export function clampInt(value: unknown, min: number, max: number, fallback: number): number {
  return Math.max(min, Math.min(max, Math.round(finiteOr(value, fallback))))
}

/** A clamped number, or null when the field is absent/unusable (nullable wire). */
export function clampNullable(value: unknown, min: number, max: number): number | null {
  if (typeof value !== 'number' || !Number.isFinite(value)) return null
  return Math.max(min, Math.min(max, value))
}

export function boolOr(value: unknown, fallback: boolean): boolean {
  return typeof value === 'boolean' ? value : fallback
}

export function enumOr<T extends string>(value: unknown, allowed: readonly T[], fallback: T): T {
  return typeof value === 'string' && (allowed as readonly string[]).includes(value)
    ? (value as T)
    : fallback
}

/**
 * Length-capped text. C0/C1 control characters are stripped (they have no place
 * in a title and only complicate the drawtext escaping downstream); the newline
 * is kept because multi-line titles are legitimate.
 */
export function textOr(value: unknown, fallback: string, max = MAX_TEXT_LENGTH): string {
  if (typeof value !== 'string') return fallback
  // eslint-disable-next-line no-control-regex -- stripping control characters is the point
  const cleaned = value.replace(/[\u0000-\u0009\u000B-\u001F\u007F-\u009F]/g, '')
  return cleaned.slice(0, max)
}

export function assetIdOr(value: unknown, fallback: string | null): string | null {
  if (typeof value !== 'string') return fallback
  const id = value.trim()
  return ASSET_ID.test(id) ? id : fallback
}

export function hexColorOr(value: unknown, fallback: string): string {
  if (typeof value !== 'string') return fallback
  const color = value.trim()
  return HEX_COLOR.test(color) ? color.toUpperCase() : fallback
}

/**
 * Map an untrusted array through `parse`, dropping entries the parser rejects
 * and stopping at `max` items so a hostile snapshot cannot grow state without
 * bound.
 */
export function sanitizeList<T>(
  value: unknown,
  max: number,
  parse: (item: unknown, index: number) => T | null,
): T[] {
  if (!Array.isArray(value)) return []
  const out: T[] = []
  for (const item of value) {
    if (out.length >= max) break
    const parsed = parse(item, out.length)
    if (parsed !== null) out.push(parsed)
  }
  return out
}

/**
 * Normalize a keyframe track: finite times/values only, clamped into range,
 * sorted by time, one point per time (last wins) and capped at 64 points.
 */
export function sanitizeKeyframeTrack(
  value: unknown,
  min: number,
  max: number,
  maxTime = Number.MAX_SAFE_INTEGER,
): KeyframeTrack {
  if (!Array.isArray(value)) return []
  const byTime = new Map<number, Keyframe>()
  for (const candidate of value) {
    if (!isRecord(candidate)) continue
    if (typeof candidate.t !== 'number' || !Number.isFinite(candidate.t)) continue
    if (typeof candidate.v !== 'number' || !Number.isFinite(candidate.v)) continue
    const t = Math.max(0, Math.min(maxTime, candidate.t))
    byTime.set(t, {
      t,
      v: Math.max(min, Math.min(max, candidate.v)),
      interp: enumOr(candidate.interp, INTERPOLATIONS, 'linear'),
    })
  }
  return [...byTime.values()].sort((a, b) => a.t - b.t).slice(0, MAX_KEYFRAMES)
}

export function cloneTrack(track: KeyframeTrack): KeyframeTrack {
  return track.map((point) => ({ ...point }))
}

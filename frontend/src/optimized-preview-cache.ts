import { sha256 } from '@noble/hashes/sha2.js'
import { optimizedPreviewFrame } from './api'
import type { ProjectDocument } from './project-schema'

export const PREVIEW_GRAPH_VERSION = 'preview-graph-v1'
export const PREVIEW_MAPPING_TIME_BASE = 1_000_000

export interface PreviewRenderSettings {
  width: number
  height: number
  pixelRatioMilli: number
  sourceMode: 'original' | 'proxy'
  rendererCompatibility: string
}

export interface PreviewFrameIdentity {
  sourceFingerprint: string
  graphVersion: string
  timelineTick: number
  settings: PreviewRenderSettings
}

export interface CachedPreviewFrame {
  bitmap: ImageBitmap
  width: number
  height: number
  bytes: number
}

function canonical(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(canonical)
  if (value && typeof value === 'object') {
    return Object.fromEntries(Object.entries(value as Record<string, unknown>)
      .filter(([, item]) => item !== undefined)
      .sort(([a], [b]) => a.localeCompare(b))
      .map(([key, item]) => [key, canonical(item)]))
  }
  if (typeof value === 'number' && !Number.isFinite(value)) throw new Error('preview graph contains a non-finite number')
  return value
}

function digest(value: unknown): string {
  const bytes = new TextEncoder().encode(JSON.stringify(canonical(value)))
  return Array.from(sha256(bytes), byte => byte.toString(16).padStart(2, '0')).join('')
}

export function previewGraphFingerprint(sourceFingerprint: string, graph: unknown): string {
  if (!/^[a-f0-9]{64}$/.test(sourceFingerprint)) throw new Error('preview source fingerprint is invalid')
  return digest({ schema: PREVIEW_GRAPH_VERSION, sourceFingerprint, graph })
}

export function previewTimelineTick(seconds: number): number {
  if (!Number.isFinite(seconds) || seconds < 0) throw new Error('preview timeline time is invalid')
  const tick = Math.round(seconds * PREVIEW_MAPPING_TIME_BASE)
  if (!Number.isSafeInteger(tick)) throw new Error('preview timeline tick exceeds safe integer range')
  return tick
}

interface WireRange { start: number; end: number }

function wireRanges(value: unknown): WireRange[] {
  if (!Array.isArray(value)) return []
  return value
    .filter((item): item is WireRange => Boolean(item && typeof item === 'object'
      && Number.isFinite((item as WireRange).start) && Number.isFinite((item as WireRange).end)
      && (item as WireRange).end > (item as WireRange).start))
    .map(item => ({ start: item.start, end: item.end }))
    .sort((left, right) => left.start - right.start)
}

/** Map the media element's raw source clock into the output clock produced by
 * the legacy single-clip EditPlan: keep ranges are concatenated, optionally
 * reversed, then the resulting timestamps are divided by playback speed. */
export function sourceMediaTimeToEditedSeconds(
  sourceSeconds: number,
  sourceDuration: number,
  editPayload: Record<string, unknown>,
): number | null {
  if (!Number.isFinite(sourceSeconds) || !Number.isFinite(sourceDuration) || sourceDuration <= 0) return null
  const segments = wireRanges(editPayload.segments)
  const trim = wireRanges(editPayload.trim ? [editPayload.trim] : [])[0]
  const ranges = segments.length ? segments : [trim ?? { start: 0, end: sourceDuration }]
  let concatenatedOffset = 0
  let position: number | null = null
  let totalDuration = 0
  for (const range of ranges) {
    const start = Math.max(0, Math.min(sourceDuration, range.start))
    const end = Math.max(0, Math.min(sourceDuration, range.end))
    if (end <= start) continue
    const duration = end - start
    if (position === null && sourceSeconds >= start && sourceSeconds < end) {
      position = concatenatedOffset + Math.min(duration, Math.max(0, sourceSeconds - start))
    }
    concatenatedOffset += duration
    totalDuration += duration
  }
  if (position === null || totalDuration <= 0) return null
  const speedValue = typeof editPayload.speed === 'number' && Number.isFinite(editPayload.speed) ? editPayload.speed : 1
  const speed = Math.min(2, Math.max(0.5, speedValue))
  if (editPayload.reverse === true) position = Math.max(0, totalDuration - position)
  const output = position / speed
  return output >= totalDuration / speed ? null : output
}

/** Until timeline-to-EditPlan (#20) lands, the backend endpoint can only claim
 * exactness for the untouched single primary clip represented by EditRequest. */
export function legacySingleClipPreviewEligible(document: ProjectDocument | null): boolean {
  if (!document) return true
  const sequence = document.sequences.find(item => item.id === document.activeSequenceId)
  if (!sequence) return false
  const primary = document.media.find(item => item.id === document.primaryMediaId)
  const duration = primary?.metadata.duration
  if (!primary || primary.kind !== 'video' || typeof duration !== 'number' || !Number.isFinite(duration) || duration <= 0) return false
  const fullDurationTicks = Math.round(duration * sequence.settings.timeBase)
  if (!Number.isSafeInteger(fullDurationTicks)) return false
  const videoTracks = sequence.tracks.filter(track => track.kind === 'video' && track.hidden !== true)
  if (videoTracks.length !== 1 || videoTracks[0]!.clips.length !== 1) return false
  const clip = videoTracks[0]!.clips[0]!
  const enabledEffects = clip.effects.filter(effect => effect.enabled)
  return clip.mediaId === document.primaryMediaId
    && clip.timelineStartTick === 0
    && clip.sourceInTick === 0
    && clip.sourceOutTick === fullDurationTicks
    && clip.durationTicks === fullDurationTicks
    && enabledEffects.length === 1
    && enabledEffects[0]!.kind === 'legacy_edit'
}

export function previewFrameKey(identity: PreviewFrameIdentity): string {
  const { settings } = identity
  if (!Number.isSafeInteger(identity.timelineTick) || identity.timelineTick < 0 || !/^[a-f0-9]{64}$/.test(identity.sourceFingerprint) || !/^[a-f0-9]{64}$/.test(identity.graphVersion)) throw new Error('invalid preview frame identity')
  if (!Number.isInteger(settings.width) || settings.width <= 0 || !Number.isInteger(settings.height) || settings.height <= 0 || settings.width > 7680 || settings.height > 4320 || !Number.isInteger(settings.pixelRatioMilli) || settings.pixelRatioMilli < 250 || settings.pixelRatioMilli > 4000 || !settings.rendererCompatibility) throw new Error('invalid preview render settings')
  return digest({ schema: 'preview-frame-v1', ...identity })
}

export class PreviewFrameCache {
  private readonly entries = new Map<string, CachedPreviewFrame>()
  private usedBytes = 0

  constructor(readonly maxBytes = 64 * 1024 * 1024, readonly maxEntries = 240) {
    if (!Number.isSafeInteger(maxBytes) || maxBytes <= 0 || !Number.isSafeInteger(maxEntries) || maxEntries <= 0) throw new Error('invalid preview cache budget')
  }

  get bytes(): number { return this.usedBytes }
  get size(): number { return this.entries.size }

  get(key: string): CachedPreviewFrame | null {
    const value = this.entries.get(key)
    if (!value) return null
    this.entries.delete(key); this.entries.set(key, value)
    return value
  }

  put(key: string, bitmap: ImageBitmap): boolean {
    const bytes = bitmap.width * bitmap.height * 4
    if (!Number.isSafeInteger(bytes) || bytes <= 0 || bytes > this.maxBytes) { bitmap.close(); return false }
    this.delete(key)
    this.entries.set(key, { bitmap, width: bitmap.width, height: bitmap.height, bytes })
    this.usedBytes += bytes
    while (this.usedBytes > this.maxBytes || this.entries.size > this.maxEntries) {
      const oldest = this.entries.keys().next().value as string | undefined
      if (!oldest) break
      this.delete(oldest)
    }
    return this.entries.has(key)
  }

  delete(key: string): void {
    const current = this.entries.get(key)
    if (!current) return
    this.entries.delete(key); this.usedBytes -= current.bytes; current.bitmap.close()
  }

  clear(): void {
    for (const value of this.entries.values()) value.bitmap.close()
    this.entries.clear(); this.usedBytes = 0
  }
}

export function requestBackendPreviewFrame(
  edit: Record<string, unknown>,
  identity: PreviewFrameIdentity,
  signal?: AbortSignal,
): Promise<{ blob: Blob; cacheKey: string | null }> {
  return optimizedPreviewFrame(edit, {
    timelineTick: identity.timelineTick,
    timelineTimeBase: PREVIEW_MAPPING_TIME_BASE,
    width: identity.settings.width,
    height: identity.settings.height,
    quality: 82,
  }, signal)
}

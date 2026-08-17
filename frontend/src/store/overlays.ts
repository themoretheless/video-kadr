// Overlays (watermark, logo, PiP, green screen), drawtext titles and the
// subtitle track. Foundation ships state, serialization and validated restore;
// the text agent adds the editing actions, the subtitle cue model and the
// selection the live preview drags.

import { reactive } from 'vue'
import {
  boxDeltaToNormalized,
  boxToNormalized,
  normalizedToBox,
  overlayPlacement,
  type ContentBox,
} from '../domain/contentBox'
import { toast } from '../toasts'
import type {
  AssetEntry,
  ChromaKeySpec,
  OverlayAudioSpec,
  OverlayKind,
  OverlaySpec,
  SubtitlePosition,
  SubtitleSpec,
  TitleAlign,
  TitleAnimation,
  TitleBoxSpec,
  TitleSpec,
} from '../types'
import { assetUrl, findAsset, uploadAsset } from './assets'
import {
  assetIdOr,
  boolOr,
  clampInt,
  clampNullable,
  clampNumber,
  enumOr,
  hexColorOr,
  isRecord,
  MAX_OVERLAYS,
  MAX_TEXT_LENGTH,
  MAX_TITLES,
  sanitizeList,
  textOr,
} from './validation'

const OVERLAY_KINDS: readonly OverlayKind[] = ['image', 'video']
const TITLE_ALIGNS: readonly TitleAlign[] = ['center', 'left', 'right']
const TITLE_ANIMATIONS: readonly TitleAnimation[] = [
  'none',
  'fade',
  'slide-up',
  'typewriter',
  'pop',
]
const SUBTITLE_POSITIONS: readonly SubtitlePosition[] = ['bottom', 'top']

/** Overlays may hang off the frame, so placement is not clamped to 0..1. */
const MIN_COORD = -1
const MAX_COORD = 2
const MAX_TIMELINE_SECONDS = 24 * 60 * 60
const MAX_FADE_SECONDS = 60

export interface OverlaysState {
  overlays: OverlaySpec[]
  titles: TitleSpec[]
  subtitles: SubtitleSpec | null
}

function defaults(): OverlaysState {
  return { overlays: [], titles: [], subtitles: null }
}

export const overlaysState = reactive<OverlaysState>(defaults())

export function resetOverlays(): void {
  overlaysState.overlays = []
  overlaysState.titles = []
  overlaysState.subtitles = null
  resetOverlaysUi()
}

function sanitizeChromaKey(value: unknown): ChromaKeySpec | null {
  if (!isRecord(value)) return null
  return {
    color: hexColorOr(value.color, '#00FF00'),
    similarity: clampNumber(value.similarity, 0, 1, 0.12),
    blend: clampNumber(value.blend, 0, 1, 0.05),
  }
}

function sanitizeOverlayAudio(value: unknown): OverlayAudioSpec | null {
  if (!isRecord(value)) return null
  return {
    enabled: boolOr(value.enabled, false),
    volume: clampNumber(value.volume, 0, 4, 1),
  }
}

/** Validate an untrusted overlay; null when it has no resolvable asset. */
export function sanitizeOverlay(value: unknown): OverlaySpec | null {
  if (!isRecord(value)) return null
  const assetId = assetIdOr(value.assetId, null)
  if (!assetId) return null
  const kind = enumOr(value.kind, OVERLAY_KINDS, 'image')
  return {
    assetId,
    kind,
    x: clampNumber(value.x, MIN_COORD, MAX_COORD, 0),
    y: clampNumber(value.y, MIN_COORD, MAX_COORD, 0),
    width: clampNumber(value.width, 0.001, 4, 0.25),
    height: clampNullable(value.height, 0.001, 4),
    opacity: clampNumber(value.opacity, 0, 1, 1),
    rotation: clampNumber(value.rotation, -360, 360, 0),
    start: clampNumber(value.start, 0, MAX_TIMELINE_SECONDS, 0),
    end: clampNullable(value.end, 0, MAX_TIMELINE_SECONDS),
    fadeIn: clampNumber(value.fadeIn, 0, MAX_FADE_SECONDS, 0),
    fadeOut: clampNumber(value.fadeOut, 0, MAX_FADE_SECONDS, 0),
    chromaKey: sanitizeChromaKey(value.chromaKey),
    // Audio only exists for video overlays; an image overlay silently drops it.
    audio: kind === 'video' ? sanitizeOverlayAudio(value.audio) : null,
  }
}

function serializeOverlay(overlay: OverlaySpec): Record<string, unknown> {
  const out: Record<string, unknown> = {
    assetId: overlay.assetId,
    kind: overlay.kind,
    x: clampNumber(overlay.x, MIN_COORD, MAX_COORD, 0),
    y: clampNumber(overlay.y, MIN_COORD, MAX_COORD, 0),
    width: clampNumber(overlay.width, 0.001, 4, 0.25),
    height: clampNullable(overlay.height, 0.001, 4),
  }
  const opacity = clampNumber(overlay.opacity, 0, 1, 1)
  if (opacity !== 1) out.opacity = opacity
  const rotation = clampNumber(overlay.rotation, -360, 360, 0)
  if (rotation !== 0) out.rotation = rotation
  const start = clampNumber(overlay.start, 0, MAX_TIMELINE_SECONDS, 0)
  if (start !== 0) out.start = start
  const end = clampNullable(overlay.end, 0, MAX_TIMELINE_SECONDS)
  if (end !== null) out.end = end
  const fadeIn = clampNumber(overlay.fadeIn, 0, MAX_FADE_SECONDS, 0)
  if (fadeIn > 0) out.fadeIn = fadeIn
  const fadeOut = clampNumber(overlay.fadeOut, 0, MAX_FADE_SECONDS, 0)
  if (fadeOut > 0) out.fadeOut = fadeOut
  const chromaKey = sanitizeChromaKey(overlay.chromaKey)
  if (chromaKey) out.chromaKey = { ...chromaKey }
  const audio = overlay.kind === 'video' ? sanitizeOverlayAudio(overlay.audio) : null
  if (audio?.enabled) out.audio = { ...audio }
  return out
}

function sanitizeTitleBox(value: unknown): TitleBoxSpec | null {
  if (!isRecord(value)) return null
  return {
    color: hexColorOr(value.color, '#000000'),
    opacity: clampNumber(value.opacity, 0, 1, 0.5),
    padding: clampInt(value.padding, 0, 200, 12),
  }
}

/** Validate an untrusted title; null when it carries no text to draw. */
export function sanitizeTitle(value: unknown): TitleSpec | null {
  if (!isRecord(value)) return null
  const text = textOr(value.text, '')
  if (!text.trim()) return null
  return {
    text,
    fontAssetId: assetIdOr(value.fontAssetId, null),
    fontSize: clampInt(value.fontSize, 4, 512, 48),
    color: hexColorOr(value.color, '#FFFFFF'),
    x: clampNumber(value.x, MIN_COORD, MAX_COORD, 0.5),
    y: clampNumber(value.y, MIN_COORD, MAX_COORD, 0.85),
    align: enumOr(value.align, TITLE_ALIGNS, 'center'),
    box: sanitizeTitleBox(value.box),
    borderWidth: clampNumber(value.borderWidth, 0, 20, 0),
    borderColor: hexColorOr(value.borderColor, '#000000'),
    shadowX: clampNumber(value.shadowX, -100, 100, 0),
    shadowY: clampNumber(value.shadowY, -100, 100, 0),
    shadowColor: hexColorOr(value.shadowColor, '#000000'),
    start: clampNumber(value.start, 0, MAX_TIMELINE_SECONDS, 0),
    end: clampNullable(value.end, 0, MAX_TIMELINE_SECONDS),
    fadeIn: clampNumber(value.fadeIn, 0, MAX_FADE_SECONDS, 0),
    fadeOut: clampNumber(value.fadeOut, 0, MAX_FADE_SECONDS, 0),
    animation: enumOr(value.animation, TITLE_ANIMATIONS, 'none'),
  }
}

function serializeTitle(title: TitleSpec): Record<string, unknown> {
  const out: Record<string, unknown> = {
    text: title.text,
    fontSize: clampInt(title.fontSize, 4, 512, 48),
    color: hexColorOr(title.color, '#FFFFFF'),
    x: clampNumber(title.x, MIN_COORD, MAX_COORD, 0.5),
    y: clampNumber(title.y, MIN_COORD, MAX_COORD, 0.85),
    align: enumOr(title.align, TITLE_ALIGNS, 'center'),
  }
  const fontAssetId = assetIdOr(title.fontAssetId, null)
  if (fontAssetId) out.fontAssetId = fontAssetId
  const box = sanitizeTitleBox(title.box)
  if (box) out.box = { ...box }
  const borderWidth = clampNumber(title.borderWidth, 0, 20, 0)
  if (borderWidth > 0) {
    out.borderWidth = borderWidth
    out.borderColor = hexColorOr(title.borderColor, '#000000')
  }
  const shadowX = clampNumber(title.shadowX, -100, 100, 0)
  const shadowY = clampNumber(title.shadowY, -100, 100, 0)
  if (shadowX !== 0 || shadowY !== 0) {
    out.shadowX = shadowX
    out.shadowY = shadowY
    out.shadowColor = hexColorOr(title.shadowColor, '#000000')
  }
  const start = clampNumber(title.start, 0, MAX_TIMELINE_SECONDS, 0)
  if (start !== 0) out.start = start
  const end = clampNullable(title.end, 0, MAX_TIMELINE_SECONDS)
  if (end !== null) out.end = end
  const fadeIn = clampNumber(title.fadeIn, 0, MAX_FADE_SECONDS, 0)
  if (fadeIn > 0) out.fadeIn = fadeIn
  const fadeOut = clampNumber(title.fadeOut, 0, MAX_FADE_SECONDS, 0)
  if (fadeOut > 0) out.fadeOut = fadeOut
  const animation = enumOr(title.animation, TITLE_ANIMATIONS, 'none')
  if (animation !== 'none') out.animation = animation
  return out
}

/** Validate the untrusted subtitle block; null when no asset is attached. */
export function sanitizeSubtitles(value: unknown): SubtitleSpec | null {
  if (!isRecord(value)) return null
  const assetId = assetIdOr(value.assetId, null)
  if (!assetId) return null
  return {
    assetId,
    burnIn: boolOr(value.burnIn, true),
    fontSize: clampInt(value.fontSize, 4, 256, 24),
    color: hexColorOr(value.color, '#FFFFFF'),
    outlineWidth: clampNumber(value.outlineWidth, 0, 20, 2),
    position: enumOr(value.position, SUBTITLE_POSITIONS, 'bottom'),
    marginV: clampInt(value.marginV, 0, 1000, 40),
  }
}

export function overlaysPayload(): Record<string, unknown> {
  const payload: Record<string, unknown> = {}
  const overlays = sanitizeList(overlaysState.overlays, MAX_OVERLAYS, sanitizeOverlay)
  if (overlays.length) payload.overlays = overlays.map(serializeOverlay)
  const titles = sanitizeList(overlaysState.titles, MAX_TITLES, sanitizeTitle)
  if (titles.length) payload.titles = titles.map(serializeTitle)
  const subtitles = sanitizeSubtitles(overlaysState.subtitles)
  if (subtitles) payload.subtitles = { ...subtitles }
  return payload
}

export function applyOverlaysSnapshot(raw: unknown): void {
  const source = isRecord(raw) ? raw : {}
  overlaysState.overlays = sanitizeList(source.overlays, MAX_OVERLAYS, sanitizeOverlay)
  overlaysState.titles = sanitizeList(source.titles, MAX_TITLES, sanitizeTitle)
  overlaysState.subtitles = sanitizeSubtitles(source.subtitles)
  // Undo, a project restore or a new clip can invalidate both of them.
  clampSelection()
  if (overlaysUi.cuesAssetId !== (overlaysState.subtitles?.assetId ?? null)) {
    forgetCues()
  }
}

// --- editing surface -------------------------------------------------------
//
// Everything below is UI-facing. `overlaysState` is deep-watched for undo and
// autosave, so transient editor state (what is selected, the parsed cue list)
// lives in `overlaysUi` instead: selecting a title must not create an undo step
// and must not travel into a saved project.

export type OverlaySelectionKind = 'title' | 'overlay'

export interface OverlaySelection {
  kind: OverlaySelectionKind
  index: number
}

/** One parsed `.srt`/`.vtt` cue, times in seconds on the OUTPUT timeline. */
export interface SubtitleCue {
  start: number
  end: number
  text: string
}

/** Enough for a feature-length film at a normal speaking rate. */
export const MAX_SUBTITLE_CUES = 2000
/** Contract cap for a subtitle asset body, section 4. */
const MAX_SUBTITLE_BYTES = 4 * 1024 * 1024
const MAX_CUE_SECONDS = 24 * 60 * 60

export interface OverlaysUiState {
  selection: OverlaySelection | null
  /**
   * Index of the overlay whose chroma key colour is being picked off the
   * current frame, or null when the eyedropper is idle.
   */
  eyedropper: number | null
  /** Cues parsed from the attached asset; empty until one is loaded. */
  cues: SubtitleCue[]
  /** Asset the cues were parsed from, so a snapshot swap can drop them. */
  cuesAssetId: string | null
  /** Set once the user edits a cue: the asset no longer matches the list. */
  cuesDirty: boolean
  cuesLoading: boolean
  cuesError: string
}

export const overlaysUi = reactive<OverlaysUiState>({
  selection: null,
  eyedropper: null,
  cues: [],
  cuesAssetId: null,
  cuesDirty: false,
  cuesLoading: false,
  cuesError: '',
})

function resetOverlaysUi(): void {
  overlaysUi.selection = null
  overlaysUi.eyedropper = null
  forgetCues()
}

/** Arm the frame eyedropper for one overlay's chroma key. */
export function startEyedropper(index: number): void {
  if (!Number.isInteger(index) || index < 0 || index >= overlaysState.overlays.length) return
  overlaysUi.eyedropper = index
  selectItem('overlay', index)
}

export function cancelEyedropper(): void {
  overlaysUi.eyedropper = null
}

/**
 * Store a colour sampled from the current frame and disarm the eyedropper.
 * A picked colour also turns the key on, which is what the user just asked for.
 */
export function applyEyedropper(color: string): void {
  const index = overlaysUi.eyedropper
  overlaysUi.eyedropper = null
  if (index === null) return
  const overlay = overlaysState.overlays[index]
  if (!overlay) return
  const key = overlay.chromaKey
  overlay.chromaKey = key
    ? { ...key, color: hexColorOr(color, key.color) }
    : { color: hexColorOr(color, '#00FF00'), similarity: 0.12, blend: 0.05 }
}

function forgetCues(): void {
  overlaysUi.cues = []
  overlaysUi.cuesAssetId = null
  overlaysUi.cuesDirty = false
  overlaysUi.cuesLoading = false
  overlaysUi.cuesError = ''
}

function clampSelection(): void {
  const selection = overlaysUi.selection
  if (!selection) return
  const list = selection.kind === 'title' ? overlaysState.titles : overlaysState.overlays
  if (selection.index < 0 || selection.index >= list.length) overlaysUi.selection = null
}

export function selectItem(kind: OverlaySelectionKind, index: number): void {
  overlaysUi.selection = { kind, index }
  clampSelection()
}

export function isSelected(kind: OverlaySelectionKind, index: number): boolean {
  const selection = overlaysUi.selection
  return selection !== null && selection.kind === kind && selection.index === index
}

export function clearSelection(): void {
  overlaysUi.selection = null
}

/** A new title, staged low and centred like a lower third. */
export function createTitle(): TitleSpec {
  return {
    text: 'Новый заголовок',
    fontAssetId: null,
    fontSize: 48,
    color: '#FFFFFF',
    x: 0.5,
    y: 0.85,
    align: 'center',
    box: null,
    borderWidth: 0,
    borderColor: '#000000',
    shadowX: 0,
    shadowY: 0,
    shadowColor: '#000000',
    start: 0,
    end: null,
    fadeIn: 0,
    fadeOut: 0,
    animation: 'none',
  }
}

/** Append a title and select it. Returns its index, or -1 when the cap is hit. */
export function addTitle(): number {
  if (overlaysState.titles.length >= MAX_TITLES) return -1
  overlaysState.titles.push(createTitle())
  const index = overlaysState.titles.length - 1
  selectItem('title', index)
  return index
}

export function removeTitle(index: number): void {
  if (!Number.isInteger(index) || index < 0 || index >= overlaysState.titles.length) return
  overlaysState.titles.splice(index, 1)
  clampSelection()
}

/** Move a list entry by `delta` places. Stacking order is drawing order. */
function moveWithin<T>(list: T[], index: number, delta: number): number {
  if (!Number.isInteger(index) || index < 0 || index >= list.length) return -1
  const target = index + Math.trunc(delta)
  if (target < 0 || target >= list.length || target === index) return -1
  const [entry] = list.splice(index, 1)
  list.splice(target, 0, entry)
  return target
}

export function moveTitle(index: number, delta: number): void {
  const target = moveWithin(overlaysState.titles, index, delta)
  if (target >= 0) selectItem('title', target)
}

/** A new overlay covering a quarter of the frame in the top-left corner. */
export function createOverlay(asset: AssetEntry): OverlaySpec | null {
  if (asset.kind !== 'image' && asset.kind !== 'video') return null
  const kind: OverlayKind = asset.kind
  return {
    assetId: asset.id,
    kind,
    x: 0.05,
    y: 0.05,
    width: 0.25,
    height: null,
    opacity: 1,
    rotation: 0,
    start: 0,
    end: null,
    fadeIn: 0,
    fadeOut: 0,
    chromaKey: null,
    audio: kind === 'video' ? { enabled: false, volume: 1 } : null,
  }
}

export function addOverlay(asset: AssetEntry): number {
  if (overlaysState.overlays.length >= MAX_OVERLAYS) return -1
  const overlay = createOverlay(asset)
  if (!overlay) return -1
  overlaysState.overlays.push(overlay)
  const index = overlaysState.overlays.length - 1
  selectItem('overlay', index)
  return index
}

export function removeOverlay(index: number): void {
  if (!Number.isInteger(index) || index < 0 || index >= overlaysState.overlays.length) return
  overlaysState.overlays.splice(index, 1)
  clampSelection()
}

export function moveOverlay(index: number, delta: number): void {
  const target = moveWithin(overlaysState.overlays, index, delta)
  if (target >= 0) selectItem('overlay', target)
}

/** Aspect ratio of an overlay's asset, or null when the size is unknown. */
export function overlayAspect(overlay: OverlaySpec): number | null {
  const asset = findAsset(overlay.assetId)
  if (!asset?.width || !asset.height) return null
  return asset.width / asset.height
}

/** True when a timed item is on screen at `time` output seconds. */
function visibleAt(item: { start?: number; end?: number | null }, time: number): boolean {
  if (!Number.isFinite(time)) return true
  const start = item.start ?? 0
  const end = item.end
  if (time < start) return false
  return typeof end !== 'number' ? true : time <= end
}

export function titleVisibleAt(title: TitleSpec, time: number): boolean {
  return visibleAt(title, time)
}

export function overlayVisibleAt(overlay: OverlaySpec, time: number): boolean {
  return visibleAt(overlay, time)
}

// --- live preview dragging -------------------------------------------------

/** One drag step from the preview, in CSS pixels of the player element. */
export interface PreviewDrag {
  dx: number
  dy: number
  dw: number
  dh: number
}

/**
 * Apply a preview drag to one overlay.
 *
 * Deltas arrive in CSS pixels and are converted against the video CONTENT box,
 * so a letterboxed source drags at the same rate as a full-bleed one. The same
 * clamps the sanitizers use are applied here, so a drag can never push state
 * somewhere `overlaysPayload` would then rewrite behind the user's back.
 */
export function dragOverlay(index: number, box: ContentBox, change: PreviewDrag): void {
  const overlay = overlaysState.overlays[index]
  if (!overlay) return
  const move = boxDeltaToNormalized(box, change.dx, change.dy)
  const size = boxDeltaToNormalized(box, change.dw, change.dh)
  overlay.x = clampNumber(overlay.x + move.x, MIN_COORD, MAX_COORD, overlay.x)
  overlay.y = clampNumber(overlay.y + move.y, MIN_COORD, MAX_COORD, overlay.y)
  if (change.dw !== 0) {
    overlay.width = clampNumber(overlay.width + size.x, MIN_PREVIEW_SIZE, 4, overlay.width)
  }
  if (change.dh !== 0) {
    // A vertical resize necessarily pins the height, so a layer that was
    // following its source aspect ratio stops doing so from here on.
    const current =
      typeof overlay.height === 'number'
        ? overlay.height
        : box.height > MIN_PREVIEW_SIZE
          ? overlayPlacement(box, overlay, overlayAspect(overlay)).height / box.height
          : MIN_PREVIEW_SIZE
    overlay.height = clampNumber(current + size.y, MIN_PREVIEW_SIZE, 4, current)
  }
}

/** Apply a preview drag to one title. Titles carry an anchor, not a size. */
export function dragTitle(index: number, box: ContentBox, change: PreviewDrag): void {
  const title = overlaysState.titles[index]
  if (!title) return
  const anchor = normalizedToBox(box, title.x, title.y)
  const moved = boxToNormalized(box, anchor.left + change.dx, anchor.top + change.dy)
  title.x = clampNumber(moved.x, MIN_COORD, MAX_COORD, title.x)
  title.y = clampNumber(moved.y, MIN_COORD, MAX_COORD, title.y)
}

/** Smallest normalized layer size a drag may leave behind. */
const MIN_PREVIEW_SIZE = 0.01

// --- subtitles -------------------------------------------------------------

export function defaultSubtitles(assetId: string): SubtitleSpec {
  return {
    assetId,
    burnIn: true,
    fontSize: 24,
    color: '#FFFFFF',
    outlineWidth: 2,
    position: 'bottom',
    marginV: 40,
  }
}

export function attachSubtitles(assetId: string): void {
  const id = assetIdOr(assetId, null)
  if (!id) return
  overlaysState.subtitles = defaultSubtitles(id)
  forgetCues()
}

export function clearSubtitles(): void {
  overlaysState.subtitles = null
  forgetCues()
}

/**
 * Parse a `.srt` or `.vtt` document into cues.
 *
 * Deliberately forgiving in the same places the backend parser is: it accepts
 * both timestamp spellings, ignores WebVTT cue settings after the end stamp,
 * and skips anything it cannot read instead of throwing. Nothing here trusts a
 * length: text is capped per cue and the list is capped overall.
 */
export function parseSubtitleCues(text: string): SubtitleCue[] {
  if (typeof text !== 'string') return []
  const body = text.replace(/^\uFEFF/, '').replace(/\r\n?/g, '\n')
  const lines = body.split('\n')
  const cues: SubtitleCue[] = []
  for (let index = 0; index < lines.length && cues.length < MAX_SUBTITLE_CUES; index += 1) {
    const separator = lines[index].indexOf('-->')
    if (separator < 0) continue
    const start = parseTimestamp(lines[index].slice(0, separator))
    const rest = lines[index].slice(separator + 3).trim().split(/\s+/)[0] ?? ''
    const end = parseTimestamp(rest)
    if (start === null || end === null || end < start) continue
    // Everything up to the next blank line belongs to this cue.
    const payload: string[] = []
    let cursor = index + 1
    while (cursor < lines.length && lines[cursor].trim() !== '') {
      payload.push(lines[cursor])
      cursor += 1
    }
    index = cursor
    cues.push({
      start,
      end,
      text: textOr(payload.join('\n').trim(), '', MAX_TEXT_LENGTH),
    })
  }
  return cues
}

/** `HH:MM:SS,mmm`, `HH:MM:SS.mmm` or `MM:SS.mmm` in seconds; null when broken. */
function parseTimestamp(value: string): number | null {
  const token = value.trim()
  const match = /^(?:(\d{1,3}):)?(\d{1,2}):(\d{1,2})[,.](\d{1,3})$/.exec(token)
  if (!match) return null
  const hours = match[1] ? Number(match[1]) : 0
  const minutes = Number(match[2])
  const seconds = Number(match[3])
  const fraction = Number(match[4]) / 10 ** match[4].length
  const total = hours * 3600 + minutes * 60 + seconds + fraction
  return Number.isFinite(total) ? Math.min(total, MAX_CUE_SECONDS) : null
}

function formatTimestamp(value: number): string {
  const clamped = Math.max(0, Math.min(MAX_CUE_SECONDS, Number.isFinite(value) ? value : 0))
  const hours = Math.floor(clamped / 3600)
  const minutes = Math.floor((clamped % 3600) / 60)
  const seconds = Math.floor(clamped % 60)
  const millis = Math.round((clamped - Math.floor(clamped)) * 1000)
  const pad = (part: number, width: number) => String(part).padStart(width, '0')
  return `${pad(hours, 2)}:${pad(minutes, 2)}:${pad(seconds, 2)},${pad(Math.min(millis, 999), 3)}`
}

/** Render the cue list back into SubRip, the format the burn-in filter reads. */
export function serializeSubtitleCues(cues: readonly SubtitleCue[]): string {
  const lines: string[] = []
  let number = 0
  for (const cue of cues) {
    const text = textOr(cue.text, '', MAX_TEXT_LENGTH).trim()
    if (!text) continue
    const start = clampNumber(cue.start, 0, MAX_CUE_SECONDS, 0)
    const end = Math.max(start, clampNumber(cue.end, 0, MAX_CUE_SECONDS, start))
    number += 1
    lines.push(String(number), `${formatTimestamp(start)} --> ${formatTimestamp(end)}`, text, '')
  }
  return lines.join('\n')
}

/** Adopt a parsed cue list for `assetId` without marking it dirty. */
export function setSubtitleCues(assetId: string, cues: SubtitleCue[]): void {
  overlaysUi.cues = cues.slice(0, MAX_SUBTITLE_CUES)
  overlaysUi.cuesAssetId = assetId
  overlaysUi.cuesDirty = false
  overlaysUi.cuesError = cues.length ? '' : 'В файле не найдено ни одной реплики'
}

/** Mark the list as no longer matching the stored asset. */
export function markCuesDirty(): void {
  overlaysUi.cuesDirty = true
}

export function removeCue(index: number): void {
  if (!Number.isInteger(index) || index < 0 || index >= overlaysUi.cues.length) return
  overlaysUi.cues.splice(index, 1)
  markCuesDirty()
}

/** Index of the cue covering `time` output seconds, or -1. */
export function cueIndexAt(time: number): number {
  if (!Number.isFinite(time)) return -1
  return overlaysUi.cues.findIndex((cue) => time >= cue.start && time <= cue.end)
}

/**
 * Read the cue list of the attached asset. The upload path parses the local
 * File directly; this is the restore path, where only an id survived.
 */
export async function loadSubtitleCues(): Promise<void> {
  const spec = overlaysState.subtitles
  if (!spec || overlaysUi.cuesLoading) return
  if (overlaysUi.cuesAssetId === spec.assetId && overlaysUi.cues.length) return
  const asset = findAsset(spec.assetId)
  if (!asset) {
    overlaysUi.cuesError = 'Файл субтитров не найден в библиотеке ассетов'
    return
  }
  overlaysUi.cuesLoading = true
  overlaysUi.cuesError = ''
  try {
    const response = await fetch(assetUrl(asset))
    if (!response.ok) throw new Error(`HTTP ${response.status}`)
    const text = await response.text()
    if (text.length > MAX_SUBTITLE_BYTES) throw new Error('файл слишком большой')
    setSubtitleCues(spec.assetId, parseSubtitleCues(text))
  } catch (error) {
    overlaysUi.cues = []
    overlaysUi.cuesAssetId = null
    overlaysUi.cuesError = `Не удалось прочитать субтитры: ${describe(error)}`
  } finally {
    overlaysUi.cuesLoading = false
  }
}

function describe(error: unknown): string {
  return error instanceof Error ? error.message : String(error)
}

/**
 * Upload the edited cue list as a new subtitle asset and point the spec at it.
 *
 * The render burns in the stored FILE, so an edited list only reaches the
 * output once it has been stored. Returns false when nothing was uploaded.
 */
export async function commitSubtitleCues(): Promise<boolean> {
  const spec = overlaysState.subtitles
  if (!spec || !overlaysUi.cuesDirty) return false
  const body = serializeSubtitleCues(overlaysUi.cues)
  if (!body.trim()) {
    overlaysUi.cuesError = 'Список реплик пуст, нечего сохранять'
    return false
  }
  const source = findAsset(spec.assetId)
  const name = (source?.filename ?? 'subtitles').replace(/\.[^.]+$/, '')
  const file = new File([body], `${name}-edited.srt`, { type: 'application/x-subrip' })
  const asset = await uploadAsset(file, 'subtitle')
  if (!asset) return false
  spec.assetId = asset.id
  overlaysUi.cuesAssetId = asset.id
  overlaysUi.cuesDirty = false
  overlaysUi.cuesError = ''
  toast('success', 'Субтитры сохранены и будут вшиты при экспорте')
  return true
}

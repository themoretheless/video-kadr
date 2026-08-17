// Aspect-ratio and social-platform export presets.
//
// These are pure functions over the classic `EditState` framing fields, so the
// export panel stays declarative and the maths is unit-testable. The file lives
// under `components/spatial/` because that directory is the spatial agent's
// exclusive space in this build-out; nothing here is 360-specific and it would
// sit better in `components/edit/` once ownership is merged.

import type { EditState, VideoInfo } from '../../types'

export type FitMode = 'crop' | 'pad'

export interface Rect {
  x: number
  y: number
  w: number
  h: number
}

export interface AspectPreset {
  id: string
  label: string
  /**
   * Integer ratio. The backend parses `pad` as `<u32>:<u32>` with both sides
   * capped at 100, so 2.39:1 travels as its closest representable pair
   * (98:41 = 2.3902, 0.01% off) rather than as a decimal it would reject.
   */
  ratio: { w: number; h: number }
}

export const ASPECT_PRESETS: readonly AspectPreset[] = [
  { id: '9:16', label: '9:16', ratio: { w: 9, h: 16 } },
  { id: '1:1', label: '1:1', ratio: { w: 1, h: 1 } },
  { id: '4:5', label: '4:5', ratio: { w: 4, h: 5 } },
  { id: '16:9', label: '16:9', ratio: { w: 16, h: 9 } },
  { id: '2.39:1', label: '2.39:1', ratio: { w: 98, h: 41 } },
]

/** The framing fields a preset rewrites. */
export interface FramingPatch {
  cropEnabled: boolean
  crop: Rect
  pad: string
}

function evenDown(value: number): number {
  const rounded = Math.max(2, Math.floor(value))
  return rounded - (rounded % 2)
}

/** Largest centred rectangle of the requested ratio inside the source frame. */
export function cropForAspect(video: VideoInfo, ratio: { w: number; h: number }): Rect {
  const sourceWidth = Math.max(2, Math.floor(video.width))
  const sourceHeight = Math.max(2, Math.floor(video.height))
  const target = ratio.w / ratio.h
  let width = sourceWidth
  let height = width / target
  if (height > sourceHeight) {
    height = sourceHeight
    width = height * target
  }
  const cropWidth = Math.min(sourceWidth, evenDown(width))
  const cropHeight = Math.min(sourceHeight, evenDown(height))
  return {
    x: Math.floor((sourceWidth - cropWidth) / 2),
    y: Math.floor((sourceHeight - cropHeight) / 2),
    w: cropWidth,
    h: cropHeight,
  }
}

/**
 * `crop` fills the target ratio and throws away what does not fit; `pad` keeps
 * every pixel and adds bars. Only one of the two is ever active, so a preset
 * applied over another preset cannot leave a stale crop behind.
 */
export function framingForAspect(
  video: VideoInfo,
  preset: AspectPreset,
  mode: FitMode,
): FramingPatch {
  const full: Rect = {
    x: 0,
    y: 0,
    w: Math.max(2, Math.floor(video.width)),
    h: Math.max(2, Math.floor(video.height)),
  }
  if (mode === 'pad') {
    return { cropEnabled: false, crop: full, pad: `${preset.ratio.w}:${preset.ratio.h}` }
  }
  return { cropEnabled: true, crop: cropForAspect(video, preset.ratio), pad: '' }
}

export type PlatformId = 'reels' | 'shorts' | 'tiktok' | 'youtube'

export interface PlatformPreset {
  id: PlatformId
  label: string
  aspect: string
  /** Output width in pixels; the height follows the aspect. */
  width: number
  fps: number | null
  mode: FitMode
  hint: string
}

export const PLATFORM_PRESETS: readonly PlatformPreset[] = [
  {
    id: 'reels',
    label: 'Reels',
    aspect: '9:16',
    width: 1080,
    fps: 30,
    mode: 'crop',
    hint: 'Instagram Reels: 1080×1920, 30 кадров/с, MP4 H.264.',
  },
  {
    id: 'shorts',
    label: 'Shorts',
    aspect: '9:16',
    width: 1080,
    fps: 30,
    mode: 'crop',
    hint: 'YouTube Shorts: 1080×1920, 30 кадров/с, MP4 H.264.',
  },
  {
    id: 'tiktok',
    label: 'TikTok',
    aspect: '9:16',
    width: 1080,
    fps: 30,
    mode: 'crop',
    hint: 'TikTok: 1080×1920, 30 кадров/с, MP4 H.264.',
  },
  {
    id: 'youtube',
    label: 'YouTube',
    aspect: '16:9',
    width: 1920,
    fps: null,
    mode: 'pad',
    hint: 'YouTube: 1920×1080 с полями, исходная частота кадров, MP4 H.264.',
  },
]

/** Every `EditState` field a platform preset touches. */
export type ExportPresetPatch = Pick<
  EditState,
  'cropEnabled' | 'crop' | 'pad' | 'scaleEnabled' | 'scale' | 'fps' | 'format' | 'codec'
>

export function findAspect(id: string): AspectPreset | undefined {
  return ASPECT_PRESETS.find((preset) => preset.id === id)
}

export function findPlatform(id: string): PlatformPreset | undefined {
  return PLATFORM_PRESETS.find((preset) => preset.id === id)
}

export function platformPatch(video: VideoInfo, preset: PlatformPreset): ExportPresetPatch {
  const aspect = findAspect(preset.aspect) ?? ASPECT_PRESETS[3]!
  const framing = framingForAspect(video, aspect, preset.mode)
  return {
    ...framing,
    scaleEnabled: true,
    // A negative height keeps the aspect and rounds to an even number, which is
    // the same convention the width presets in the frame panel use.
    scale: { w: preset.width, h: -2 },
    fps: preset.fps,
    format: 'mp4',
    codec: 'h264',
  }
}

/** True when the current framing already matches the preset. */
export function isAspectActive(edit: EditState, preset: AspectPreset, mode: FitMode): boolean {
  if (mode === 'pad') return edit.pad === `${preset.ratio.w}:${preset.ratio.h}`
  if (!edit.cropEnabled || !edit.crop.h) return false
  return Math.abs(edit.crop.w / edit.crop.h - preset.ratio.w / preset.ratio.h) < 0.02
}

import { COMPOSITION_TIME_BASE, MAX_TEXT_LENGTH, type CompositionKeyframe, type CompositionVisualAnimation, type TextClip, type TextStyle } from '../composition/types'

export const MAX_SRT_BYTES = 2 * 1024 * 1024
export const MAX_SRT_CUES = 10_000
const CAPTION_SAFE_MARGIN = 0.08

export function defaultSubtitleFontSize(canvasHeight?: number): number {
  return canvasHeight === undefined ? 48 : Math.max(1, Math.round(canvasHeight * 0.05))
}

export interface SubtitleCue {
  readonly id: string
  readonly startTicks: number
  readonly endTicks: number
  readonly text: string
  readonly style?: Partial<TextStyle>
  readonly position?: { x: number; y: number }
  readonly coordinateResolution?: { width: number; height: number }
  readonly movement?: {
    from: { x: number; y: number }
    to: { x: number; y: number }
    startTicks?: number
    endTicks?: number
  }
  readonly anchor?: readonly [number, number]
  readonly rotationDegrees?: number
  readonly scale?: { x?: number; y?: number }
  readonly fade?: { inTicks: number; outTicks: number } | {
    times: readonly [number, number, number, number]
    opacities: readonly [number, number, number]
  }
}

export class SubtitleFormatError extends Error {
  constructor(message: string) {
    super(message)
    this.name = 'SubtitleFormatError'
  }
}

/** Parse UTF-8 SRT locally. No speech recognition or remote service is used. */
export function parseSrt(input: string): SubtitleCue[] {
  if (new TextEncoder().encode(input).byteLength > MAX_SRT_BYTES) {
    throw new SubtitleFormatError(`SRT exceeds ${MAX_SRT_BYTES} bytes`)
  }
  const normalized = input.replace(/^\uFEFF/, '').replace(/\r\n?/g, '\n').trim()
  if (!normalized) return []

  const cues = normalized.split(/\n{2,}/).map((block, index) => parseBlock(block, index))
  if (cues.length > MAX_SRT_CUES) {
    throw new SubtitleFormatError(`SRT contains more than ${MAX_SRT_CUES} cues`)
  }
  return cues
}

/** Parse a timecoded SRT or plain-text caption file exported by desktop editors. */
export function parseTimecodedText(input: string): SubtitleCue[] {
  try {
    return parseSrt(input)
  } catch (srtError) {
    const normalized = input.replace(/^\uFEFF/, '').replace(/\r\n?/g, '\n').trim()
    const lines = normalized.split('\n').filter((line) => line.trim())
    const cues = lines.map((line, index) => {
      const match = line.trim().match(
        /^\[?(\d{1,3}):(\d{2}):(\d{2})[,.](\d{3})\s*-->\s*(\d{1,3}):(\d{2}):(\d{2})[,.](\d{3})\]?\s+(.+)$/,
      )
      if (!match) throw srtError
      const cue: SubtitleCue = {
        id: `subtitle-${index + 1}`,
        startTicks: timestampToTicks(match.slice(1, 5).map(Number)),
        endTicks: timestampToTicks(match.slice(5, 9).map(Number)),
        text: match[9]!.trim(),
      }
      validateCue(cue, index)
      return cue
    })
    if (cues.length > MAX_SRT_CUES) {
      throw new SubtitleFormatError(`Subtitle file contains more than ${MAX_SRT_CUES} cues`)
    }
    return cues
  }
}

export function formatSrt(cues: readonly SubtitleCue[]): string {
  if (cues.length > MAX_SRT_CUES) {
    throw new SubtitleFormatError(`SRT contains more than ${MAX_SRT_CUES} cues`)
  }
  return cues
    .map((cue, index) => {
      validateCue(cue, index)
      return `${index + 1}\n${formatTimestamp(cue.startTicks)} --> ${formatTimestamp(cue.endTicks)}\n${cue.text}`
    })
    .join('\n\n')
    .concat(cues.length ? '\n' : '')
}

export function cuesToTextClips(
  cues: readonly SubtitleCue[],
  idForCue: (cue: SubtitleCue, index: number) => string,
  canvas?: { width: number; height: number },
): TextClip[] {
  return cues.map((cue, index) => {
    validateCue(cue, index)
    const resolution = cue.coordinateResolution
    const toCanvas = (point: {x: number; y: number}) => resolution && canvas ? {
      x: point.x * canvas.width / resolution.width,
      y: point.y * canvas.height / resolution.height,
    } : point
    const motion = cue.movement && { ...cue.movement, from: toCanvas(cue.movement.from), to: toCanvas(cue.movement.to) }
    const anchor = cue.anchor ?? (canvas || cue.position ? [0.5, 1] as const : undefined)
    const position = motion?.from ?? (cue.position && toCanvas(cue.position)) ?? (anchor && canvas ? {
      x: canvas.width * (CAPTION_SAFE_MARGIN + (1 - 2 * CAPTION_SAFE_MARGIN) * anchor[0]),
      y: canvas.height * (CAPTION_SAFE_MARGIN + (1 - 2 * CAPTION_SAFE_MARGIN) * anchor[1]),
    } : undefined)
    const durationTicks = cue.endTicks - cue.startTicks
    const fade = captionFade(cue.fade, durationTicks)
    const movement = motion && canvas ? captionMovement(motion, durationTicks, canvas) : undefined
    const animation: CompositionVisualAnimation | undefined = fade || movement || cue.scale ? {
      ...fade, ...movement,
      ...(cue.scale?.x !== undefined ? { scaleX: { mode: 'constant', value: cue.scale.x } } : {}),
      ...(cue.scale?.y !== undefined ? { scaleY: { mode: 'constant', value: cue.scale.y } } : {}),
    } : undefined
    const fontSizePx = cue.style?.fontSizePx ?? defaultSubtitleFontSize(canvas?.height)
    return {
      id: idForCue(cue, index),
      kind: 'text',
      timelineStartTicks: cue.startTicks,
      durationTicks,
      text: cue.text,
      x: position && canvas ? position.x - canvas.width / 2 : 0,
      y: position && canvas ? position.y - canvas.height / 2 : 0,
      ...(cue.rotationDegrees !== undefined ? { rotationDegrees: cue.rotationDegrees } : {}),
      ...(anchor ? { textAnchor: anchor } : {}),
      ...(canvas ? { textBoxWidth: Math.max(1, Math.round(canvas.width * (1 - 2 * CAPTION_SAFE_MARGIN))) } : {}),
      ...(animation ? { animation } : {}),
      opacity: 1,
      style: {
        fontSizePx,
        color: '#FFFFFFFF',
        strokeColor: '#000000FF',
        strokeWidthPx: Math.max(1, fontSizePx * 0.06),
        align: 'center',
        ...cue.style,
      },
    }
  })
}

function captionMovement(move: NonNullable<SubtitleCue['movement']>, duration: number, canvas: {width: number; height: number}): CompositionVisualAnimation {
  const start = move.startTicks ?? 0
  const end = move.endTicks ?? duration
  const jump = start === end
  const ticks = new Set([0, Math.max(0, Math.min(start, duration)), Math.min(end, duration), duration])
  if (jump && end + 1000 < duration) ticks.add(end + 1000)
  const channel = (axis: 'x' | 'y', origin: number) => ({
    mode: 'keyframes' as const,
    track: {
      timeBase: COMPOSITION_TIME_BASE,
      interpolation: jump ? 'hold' as const : 'linear' as const,
      keyframes: [...ticks].sort((a, b) => a - b).map(tick => {
        const progress = jump ? Number(tick >= end + 1000) : tick <= start ? 0 : tick >= end ? 1 : (tick - start) / (end - start)
        return { tick, value: move.from[axis] + (move.to[axis] - move.from[axis]) * progress - origin }
      }),
    },
  })
  return { x: channel('x', canvas.width / 2), y: channel('y', canvas.height / 2) }
}

function captionFade(fade: SubtitleCue['fade'], duration: number): CompositionVisualAnimation | undefined {
  if (fade && 'times' in fade) {
    const [startIn, endIn, startOut, endOut] = fade.times
    const [before, middle, after] = fade.opacities
    const sample = (tick: number) => {
      if (tick < startIn) return before
      if (tick < endIn) return before + (middle - before) * (tick - startIn) / (endIn - startIn)
      if (tick < startOut) return middle
      if (tick < endOut) return middle + (after - middle) * (tick - startOut) / (endOut - startOut)
      return after
    }
    const boundaries = new Set([0, duration])
    for (const tick of fade.times) {
      if (tick > 0 && tick < duration) boundaries.add(tick)
    }
    // Preserve a zero-length transition at its exact boundary on the cue clock.
    if (startIn === endIn && endIn > 0 && endIn <= duration) boundaries.add(endIn - 1)
    if (startOut === endOut && endOut > 0 && endOut <= duration) boundaries.add(endOut - 1)
    const keyframes = [...boundaries].sort((a, b) => a - b).map(tick => ({ tick, value: sample(tick) }))
    return { opacity: { mode: 'keyframes', track: { timeBase: COMPOSITION_TIME_BASE, interpolation: 'linear', keyframes } } }
  }
  const { inTicks = 0, outTicks = 0 } = fade ?? {}
  if (!inTicks && !outTicks) return undefined
  const keyframes: CompositionKeyframe[] = []
  const fadeOutStart = duration - outTicks
  if (inTicks >= duration) {
    keyframes.push({ tick: 0, value: 0 }, { tick: duration, value: duration / inTicks })
  } else {
    keyframes.push({ tick: 0, value: inTicks ? 0 : outTicks ? Math.min(1, duration / outTicks) : 1 })
    if (inTicks && inTicks > fadeOutStart) {
      // Fade-in wins until its last tick. At the boundary the fade-out is already in progress.
      if (inTicks > 1) keyframes.push({ tick: inTicks - 1, value: (inTicks - 1) / inTicks })
      keyframes.push({ tick: inTicks, value: (duration - inTicks) / outTicks })
    } else if (inTicks) {
      keyframes.push({ tick: inTicks, value: 1 })
    }
    if (outTicks) {
      if (fadeOutStart > inTicks) keyframes.push({ tick: fadeOutStart, value: 1 })
      keyframes.push({ tick: duration, value: 0 })
    }
  }
  return { opacity: { mode: 'keyframes', track: { timeBase: COMPOSITION_TIME_BASE, interpolation: 'linear', keyframes } } }
}

export function textClipsToCues(clips: readonly TextClip[]): SubtitleCue[] {
  return clips
    .map((clip) => ({
      id: clip.id,
      startTicks: clip.timelineStartTicks,
      endTicks: clip.timelineStartTicks + clip.durationTicks,
      text: clip.text,
    }))
    .sort((left, right) => left.startTicks - right.startTicks || left.id.localeCompare(right.id))
}

function parseBlock(block: string, index: number): SubtitleCue {
  const lines = block.split('\n')
  if (lines[0]?.trim().match(/^\d+$/)) lines.shift()
  const timing = lines.shift()?.trim() ?? ''
  const match = timing.match(
    /^(\d{1,3}):(\d{2}):(\d{2})[,.](\d{3})\s*-->\s*(\d{1,3}):(\d{2}):(\d{2})[,.](\d{3})(?:\s+.*)?$/,
  )
  if (!match) throw new SubtitleFormatError(`Invalid SRT timing at cue ${index + 1}`)
  const startTicks = timestampToTicks(match.slice(1, 5).map(Number))
  const endTicks = timestampToTicks(match.slice(5, 9).map(Number))
  const cue: SubtitleCue = {
    id: `subtitle-${index + 1}`,
    startTicks,
    endTicks,
    text: lines.join('\n').trim(),
  }
  validateCue(cue, index)
  return cue
}

function timestampToTicks(parts: number[]): number {
  const [hours = 0, minutes = 0, seconds = 0, milliseconds = 0] = parts
  if (minutes > 59 || seconds > 59) throw new SubtitleFormatError('Invalid SRT timestamp')
  const totalMilliseconds = ((hours * 60 + minutes) * 60 + seconds) * 1_000 + milliseconds
  return totalMilliseconds * (COMPOSITION_TIME_BASE / 1_000)
}

function formatTimestamp(ticks: number): string {
  if (!Number.isSafeInteger(ticks) || ticks < 0) {
    throw new SubtitleFormatError('Subtitle timestamp must be a non-negative safe integer')
  }
  const totalMilliseconds = Math.round(ticks / (COMPOSITION_TIME_BASE / 1_000))
  const milliseconds = totalMilliseconds % 1_000
  const totalSeconds = Math.floor(totalMilliseconds / 1_000)
  const seconds = totalSeconds % 60
  const totalMinutes = Math.floor(totalSeconds / 60)
  const minutes = totalMinutes % 60
  const hours = Math.floor(totalMinutes / 60)
  return `${String(hours).padStart(2, '0')}:${String(minutes).padStart(2, '0')}:${String(seconds).padStart(2, '0')},${String(milliseconds).padStart(3, '0')}`
}

function validateCue(cue: SubtitleCue, index: number): void {
  if (
    !Number.isSafeInteger(cue.startTicks) ||
    !Number.isSafeInteger(cue.endTicks) ||
    cue.startTicks < 0 ||
    cue.endTicks <= cue.startTicks
  ) {
    throw new SubtitleFormatError(`Invalid range at cue ${index + 1}`)
  }
  if (!cue.text.trim()) throw new SubtitleFormatError(`Empty text at cue ${index + 1}`)
  if (Array.from(cue.text).length > MAX_TEXT_LENGTH) {
    throw new SubtitleFormatError(`Cue ${index + 1} exceeds ${MAX_TEXT_LENGTH} characters`)
  }
}

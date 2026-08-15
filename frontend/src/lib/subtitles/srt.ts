import { COMPOSITION_TIME_BASE, MAX_TEXT_LENGTH, type TextClip } from '../composition/types'

export const MAX_SRT_BYTES = 2 * 1024 * 1024
export const MAX_SRT_CUES = 10_000

export interface SubtitleCue {
  readonly id: string
  readonly startTicks: number
  readonly endTicks: number
  readonly text: string
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
): TextClip[] {
  return cues.map((cue, index) => {
    validateCue(cue, index)
    return {
      id: idForCue(cue, index),
      kind: 'text',
      timelineStartTicks: cue.startTicks,
      durationTicks: cue.endTicks - cue.startTicks,
      text: cue.text,
      x: 0,
      y: 0,
      opacity: 1,
      style: {
        fontSizePx: 48,
        color: '#FFFFFFFF',
        backgroundColor: '#00000099',
        align: 'center',
      },
    }
  })
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

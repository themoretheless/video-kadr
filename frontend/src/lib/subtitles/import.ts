import { COMPOSITION_TIME_BASE } from '../composition/types.js'
import { MAX_SRT_BYTES, MAX_SRT_CUES, SubtitleFormatError, formatSrt, parseTimecodedText, type SubtitleCue } from './srt.js'
import { parseWebVtt } from './webvtt.js'

/** Normalize subtitle documents to the composition clock before any mutation. */
export function parseSubtitleDocument(input: string): SubtitleCue[] {
  if (new TextEncoder().encode(input).byteLength > MAX_SRT_BYTES) {
    throw new SubtitleFormatError('Subtitle file exceeds 2 MiB')
  }
  const body = input.replace(/^\uFEFF/, '').replace(/\r\n?/g, '\n').trim()
  let cues: SubtitleCue[]
  if (/^WEBVTT(?:\s|$)/.test(body)) {
    cues = parseWebVtt(`${body}\n`).cues.map((cue, index) => ({
      id: `subtitle-${index + 1}`,
      startTicks: Math.round(cue.startTicks * COMPOSITION_TIME_BASE / 90_000),
      endTicks: Math.round(cue.endTicks * COMPOSITION_TIME_BASE / 90_000),
      text: plainVttText(cue.text),
    }))
  } else if (/^\[Events\]$/im.test(body)) {
    cues = parseAss(body)
  } else {
    cues = parseTimecodedText(body)
  }
  // Reuse the same bounds and text validation as SRT and text-clip creation.
  formatSrt(cues)
  return cues
}

function plainVttText(text: string): string {
  return text.replace(/<[^>]*>/g, '').replace(/&(amp|lt|gt|nbsp|lrm|rlm);/g, (_match, entity: string) =>
    ({ amp: '&', lt: '<', gt: '>', nbsp: '\u00a0', lrm: '\u200e', rlm: '\u200f' })[entity]!)
}

function parseAss(body: string): SubtitleCue[] {
  let inEvents = false
  let fields: string[] = []
  const cues: SubtitleCue[] = []
  for (const line of body.split('\n')) {
    const trimmed = line.trim()
    if (/^\[.*\]$/.test(trimmed)) {
      inEvents = trimmed.toLowerCase() === '[events]'
      continue
    }
    if (!inEvents) continue
    if (/^Format:/i.test(trimmed)) {
      fields = trimmed.slice(trimmed.indexOf(':') + 1).split(',').map((field) => field.trim().toLowerCase())
      if (new Set(fields).size !== fields.length || fields.at(-1) !== 'text' || !fields.includes('start') || !fields.includes('end')) {
        throw new SubtitleFormatError('Invalid ASS event format')
      }
    } else if (/^Dialogue:/i.test(trimmed)) {
      if (!fields.length) throw new SubtitleFormatError('ASS event Format is missing')
      const values = trimmed.slice(trimmed.indexOf(':') + 1).split(',')
      if (values.length < fields.length) throw new SubtitleFormatError('Incomplete ASS dialogue')
      const text = values.slice(fields.length - 1).join(',')
        .replace(/\{[^}]*\}/g, '').replace(/\\[Nn]/g, '\n').replace(/\\h/g, '\u00a0').trim()
      cues.push({
        id: `subtitle-${cues.length + 1}`,
        startTicks: assTimestamp(values[fields.indexOf('start')]!.trim()),
        endTicks: assTimestamp(values[fields.indexOf('end')]!.trim()),
        text,
      })
      if (cues.length > MAX_SRT_CUES) throw new SubtitleFormatError('Subtitle cue count exceeds limit')
    }
  }
  return cues
}

function assTimestamp(value: string): number {
  const match = /^(\d+):([0-5]\d):([0-5]\d)\.(\d{2})$/.exec(value)
  if (!match) throw new SubtitleFormatError(`Invalid ASS timestamp: ${value}`)
  return (Number(match[1]) * 3600 + Number(match[2]) * 60 + Number(match[3])) * COMPOSITION_TIME_BASE
    + Number(match[4]) * (COMPOSITION_TIME_BASE / 100)
}

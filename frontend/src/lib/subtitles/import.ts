import { COMPOSITION_TIME_BASE, type TextStyle } from '../composition/types.js'
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
  let section = ''
  let fields: string[] = []
  let styleFields: string[] = []
  const styles = new Map<string, Partial<TextStyle>>()
  const cues: SubtitleCue[] = []
  for (const line of body.split('\n')) {
    const trimmed = line.trim()
    if (/^\[.*\]$/.test(trimmed)) {
      section = trimmed.toLowerCase()
      continue
    }
    if (section === '[v4+ styles]') {
      if (/^Format:/i.test(trimmed)) styleFields = assFields(trimmed)
      else if (/^Style:/i.test(trimmed)) {
        const values = assFields(trimmed, false)
        const read = (field: string) => values[styleFields.indexOf(field)]
        const name = read('name')
        if (name) styles.set(name, assStyle(read))
      }
      continue
    }
    if (section !== '[events]') continue
    if (/^Format:/i.test(trimmed)) {
      fields = assFields(trimmed)
      if (new Set(fields).size !== fields.length || fields.at(-1) !== 'text' || !fields.includes('start') || !fields.includes('end')) {
        throw new SubtitleFormatError('Invalid ASS event format')
      }
    } else if (/^Dialogue:/i.test(trimmed)) {
      if (!fields.length) throw new SubtitleFormatError('ASS event Format is missing')
      const values = trimmed.slice(trimmed.indexOf(':') + 1).split(',')
      if (values.length < fields.length) throw new SubtitleFormatError('Incomplete ASS dialogue')
      const style = { ...styles.get(values[fields.indexOf('style')]?.trim() ?? '') }
      const text = values.slice(fields.length - 1).join(',').replace(/\{([^}]*)\}/g, (_whole, block: string) => {
        for (const tag of block.split('\\')) applyAssOverride(style, tag.trim())
        return ''
      }).replace(/\\[Nn]/g, '\n').replace(/\\h/g, '\u00a0').trim()
      cues.push({
        id: `subtitle-${cues.length + 1}`,
        startTicks: assTimestamp(values[fields.indexOf('start')]!.trim()),
        endTicks: assTimestamp(values[fields.indexOf('end')]!.trim()),
        text,
        ...(Object.keys(style).length ? { style } : {}),
      })
      if (cues.length > MAX_SRT_CUES) throw new SubtitleFormatError('Subtitle cue count exceeds limit')
    }
  }
  return cues
}

type AssStyle = { -readonly [Key in keyof TextStyle]?: TextStyle[Key] }

function assFields(line: string, lowercase = true) {
  return line.slice(line.indexOf(':') + 1).split(',').map(field => lowercase ? field.trim().toLowerCase() : field.trim())
}

function assNumber(value: string | undefined, min: number, max: number) {
  const number = value ? Number(value) : NaN
  return Number.isFinite(number) && number >= min && number <= max ? number : undefined
}

function assColor(value: string | undefined) {
  const hex = /^&H([\da-f]{1,8})&?$/i.exec(value ?? '')?.[1]?.padStart(8, '0')
  if (!hex) return undefined
  return `#${hex.slice(6, 8)}${hex.slice(4, 6)}${hex.slice(2, 4)}FF`.toUpperCase()
}

function assAlign(value: string | undefined) {
  if (!/^[1-9]$/.test(value ?? '')) return undefined
  return (['left', 'center', 'right'] as const)[(Number(value) - 1) % 3]
}

function assFont(value: string | undefined): TextStyle['fontFamily'] {
  return (['Noto Sans', 'Arial Unicode MS', 'DejaVu Sans', 'Arial'] as const)
    .find(family => family.toLowerCase() === value?.toLowerCase())
}

function assStyle(read: (field: string) => string | undefined): AssStyle {
  const style: AssStyle = {}
  const size = assNumber(read('fontsize'), 1, 512)
  const color = assColor(read('primarycolour'))
  const family = assFont(read('fontname'))
  const align = assAlign(read('alignment'))
  const outline = assNumber(read('outline'), 0, 100)
  const shadow = assNumber(read('shadow'), 0, 100)
  if (size !== undefined) style.fontSizePx = size
  if (color) style.color = color
  if (family) style.fontFamily = family
  if (align) style.align = align
  if (outline !== undefined) {
    style.strokeWidthPx = outline
    style.strokeColor = assColor(read('outlinecolour')) ?? '#000000FF'
  }
  if (shadow) {
    style.shadowX = shadow
    style.shadowY = shadow
    style.shadowColor = assColor(read('backcolour')) ?? '#000000FF'
  }
  return style
}

function applyAssOverride(style: AssStyle, tag: string) {
  const size = /^fs(\d+(?:\.\d+)?)$/i.exec(tag)
  const color = /^(?:1?c)(&H[0-9a-f]{1,8}&?)$/i.exec(tag)
  const align = /^an([1-9])$/i.exec(tag)
  const font = /^fn(.+)$/i.exec(tag)
  if (size) style.fontSizePx = assNumber(size[1], 1, 512) ?? style.fontSizePx
  if (color) style.color = assColor(color[1]) ?? style.color
  if (align) style.align = assAlign(align[1]) ?? style.align
  if (font) {
    const family = assFont(font[1])
    if (family) style.fontFamily = family
  }
}

function assTimestamp(value: string): number {
  const match = /^(\d+):([0-5]\d):([0-5]\d)\.(\d{2})$/.exec(value)
  if (!match) throw new SubtitleFormatError(`Invalid ASS timestamp: ${value}`)
  return (Number(match[1]) * 3600 + Number(match[2]) * 60 + Number(match[3])) * COMPOSITION_TIME_BASE
    + Number(match[4]) * (COMPOSITION_TIME_BASE / 100)
}

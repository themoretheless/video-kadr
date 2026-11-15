import { COMPOSITION_TIME_BASE, type TextStyle } from '../composition/types.js'
import { MAX_VISUAL_POSITION } from '../composition/validation.js'
import { MAX_SRT_BYTES, MAX_SRT_CUES, SubtitleFormatError, formatSrt, parseTimecodedText, type SubtitleCue } from './srt.js'
import { parseWebVtt } from './webvtt.js'

/** Normalize subtitle documents to the composition clock before any mutation. */
export function parseSubtitleDocument(input: string): SubtitleCue[] {
  return parseSubtitleDocumentWithReport(input).cues
}

export function parseSubtitleDocumentWithReport(input: string) {
  if (new TextEncoder().encode(input).byteLength > MAX_SRT_BYTES) {
    throw new SubtitleFormatError('Subtitle file exceeds 2 MiB')
  }
  const body = input.replace(/^\uFEFF/, '').replace(/\r\n?/g, '\n').trim()
  let cues: SubtitleCue[]
  let simplifiedCueCount = 0
  if (/^WEBVTT(?:\s|$)/.test(body)) {
    cues = parseWebVtt(`${body}\n`).cues.map((cue, index) => ({
      id: `subtitle-${index + 1}`,
      startTicks: Math.round(cue.startTicks * COMPOSITION_TIME_BASE / 90_000),
      endTicks: Math.round(cue.endTicks * COMPOSITION_TIME_BASE / 90_000),
      text: plainVttText(cue.text),
    }))
  } else if (/^\[Events\]$/im.test(body)) {
    const parsed = parseAss(body)
    cues = parsed.cues
    simplifiedCueCount = parsed.simplifiedCueCount
  } else {
    cues = parseTimecodedText(body)
  }
  // Reuse the same bounds and text validation as SRT and text-clip creation.
  formatSrt(cues)
  return { cues, simplifiedCueCount }
}

function plainVttText(text: string): string {
  return text.replace(/<[^>]*>/g, '').replace(/&(amp|lt|gt|nbsp|lrm|rlm);/g, (_match, entity: string) =>
    ({ amp: '&', lt: '<', gt: '>', nbsp: '\u00a0', lrm: '\u200e', rlm: '\u200f' })[entity]!)
}

function parseAss(body: string) {
  let section = ''
  let fields: string[] = []
  let styleFields: string[] = []
  const styles = new Map<string, { style: AssStyle; anchor?: SubtitleCue['anchor'] }>()
  const cues: SubtitleCue[] = []
  let simplifiedCueCount = 0
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
        if (name) styles.set(name, { style: assStyle(read), anchor: assAnchor(read('alignment')) })
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
      const namedStyle = styles.get(values[fields.indexOf('style')]?.trim() ?? '')
      const style = { ...namedStyle?.style }
      let anchor = namedStyle?.anchor
      let position: SubtitleCue['position']
      let simplified = false
      const text = values.slice(fields.length - 1).join(',').replace(/\{([^}]*)\}/g, (_whole, block: string) => {
        for (const rawTag of assTags(block)) {
          const tag = rawTag.trim()
          if (!tag) continue
          const coordinates = assPosition(tag)
          if (coordinates) position = coordinates
          else if (!applyAssOverride(style, tag)) simplified = true
          anchor = assAnchor(/^an([1-9])$/i.exec(tag)?.[1]) ?? anchor
        }
        return ''
      }).replace(/\\[Nn]/g, '\n').replace(/\\h/g, '\u00a0').trim()
      cues.push({
        id: `subtitle-${cues.length + 1}`,
        startTicks: assTimestamp(values[fields.indexOf('start')]!.trim()),
        endTicks: assTimestamp(values[fields.indexOf('end')]!.trim()),
        text,
        ...(Object.keys(style).length ? { style } : {}),
        ...(position ? { position } : {}),
        ...(anchor ? { anchor } : {}),
      })
      if (simplified) simplifiedCueCount++
      if (cues.length > MAX_SRT_CUES) throw new SubtitleFormatError('Subtitle cue count exceeds limit')
    }
  }
  return { cues, simplifiedCueCount }
}

function assTags(block: string): string[] {
  const tags: string[] = []
  let depth = 0
  let start = -1
  for (let index = 0; index < block.length; index++) {
    const char = block[index]
    if (char === '(') depth++
    if (char === ')') depth = Math.max(0, depth - 1)
    if (char === '\\' && depth === 0) {
      if (start >= 0) tags.push(block.slice(start, index))
      start = index + 1
    }
  }
  if (start >= 0) tags.push(block.slice(start))
  return tags
}

function assPosition(tag: string): SubtitleCue['position'] {
  const match = /^pos\(\s*([+-]?(?:\d+(?:\.\d*)?|\.\d+))\s*,\s*([+-]?(?:\d+(?:\.\d*)?|\.\d+))\s*\)$/i.exec(tag)
  if (!match) return undefined
  const x = assNumber(match[1], -MAX_VISUAL_POSITION, MAX_VISUAL_POSITION)
  const y = assNumber(match[2], -MAX_VISUAL_POSITION, MAX_VISUAL_POSITION)
  return x === undefined || y === undefined ? undefined : { x, y }
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

function assAnchor(value: string | undefined): SubtitleCue['anchor'] {
  if (!/^[1-9]$/.test(value ?? '')) return undefined
  const index = Number(value) - 1
  return [(index % 3) / 2, 1 - Math.floor(index / 3) / 2]
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
  for (const flag of ['bold', 'italic'] as const) {
    const value = read(flag)
    if (value === '-1' || value === '1' || value === '0') style[flag] = value !== '0'
  }
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

function applyAssOverride(style: AssStyle, tag: string): boolean {
  const size = /^fs(\d+(?:\.\d+)?)$/i.exec(tag)
  const color = /^(?:1?c)(&H[0-9a-f]{1,8}&?)$/i.exec(tag)
  const align = /^an([1-9])$/i.exec(tag)
  const font = /^fn(.+)$/i.exec(tag)
  const face = /^([bi])(-?1|0)$/i.exec(tag)
  const fontSize = assNumber(size?.[1], 1, 512)
  if (fontSize !== undefined) style.fontSizePx = fontSize
  if (color) style.color = assColor(color[1]) ?? style.color
  if (align) style.align = assAlign(align[1]) ?? style.align
  if (font) {
    const family = assFont(font[1])
    if (family) style.fontFamily = family
    return !!family
  }
  if (face) style[face[1]!.toLowerCase() === 'b' ? 'bold' : 'italic'] = face[2] !== '0'
  return fontSize !== undefined || !!(color || align || face)
}

function assTimestamp(value: string): number {
  const match = /^(\d+):([0-5]\d):([0-5]\d)\.(\d{2})$/.exec(value)
  if (!match) throw new SubtitleFormatError(`Invalid ASS timestamp: ${value}`)
  return (Number(match[1]) * 3600 + Number(match[2]) * 60 + Number(match[3])) * COMPOSITION_TIME_BASE
    + Number(match[4]) * (COMPOSITION_TIME_BASE / 100)
}

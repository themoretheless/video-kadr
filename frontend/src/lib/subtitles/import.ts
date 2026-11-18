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
  let legacySsa = false
  const styles = new Map<string, { style: AssStyle; anchor?: SubtitleCue['anchor'] }>()
  const cues: SubtitleCue[] = []
  let simplifiedCueCount = 0
  for (const line of body.split('\n')) {
    const trimmed = line.trim()
    if (/^\[.*\]$/.test(trimmed)) {
      section = trimmed.toLowerCase()
      if (section === '[v4 styles]') {
        legacySsa = true
        styleFields = 'name fontname fontsize primarycolour secondarycolour tertiarycolour backcolour bold italic borderstyle outline shadow alignment marginl marginr marginv alphalevel encoding'.split(' ')
      } else if (section === '[v4+ styles]') {
        legacySsa = false
        styleFields = []
      }
      continue
    }
    if (section === '[script info]' && /^ScriptType:\s*v4\.00\+?$/i.test(trimmed)) {
      legacySsa = !trimmed.endsWith('+')
    }
    if (section === '[v4+ styles]' || section === '[v4 styles]') {
      if (/^Format:/i.test(trimmed)) styleFields = assFields(trimmed)
      else if (/^Style:/i.test(trimmed)) {
        const values = assFields(trimmed, false)
        const readField = (field: string) => values[styleFields.indexOf(field)]
        const read = section === '[v4 styles]' ? (field: string) => ssaStyleField(readField, field) : readField
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
      if (!fields.length && legacySsa) {
        fields = 'marked start end style name marginl marginr marginv effect text'.split(' ')
      }
      if (!fields.length) throw new SubtitleFormatError('ASS event Format is missing')
      const values = trimmed.slice(trimmed.indexOf(':') + 1).split(',')
      if (values.length < fields.length) throw new SubtitleFormatError('Incomplete ASS dialogue')
      const styleName = values[fields.indexOf('style')]?.trim() ?? ''
      const namedStyle = styles.get(styleName) ?? styles.get('Default')
      let style = { ...namedStyle?.style }
      let anchor = namedStyle?.anchor
      let position: SubtitleCue['position']
      let fade: SubtitleCue['fade']
      let simplified = false
      let previousBlockEnd = 0
      let hasText = false
      const dialogue = values.slice(fields.length - 1).join(',')
      const text = dialogue.replace(/\{([^}]*)\}/g, (whole, block: string, offset: number) => {
        hasText ||= dialogue.slice(previousBlockEnd, offset).trim().length > 0
        previousBlockEnd = offset + whole.length
        for (const rawTag of assTags(block)) {
          const tag = rawTag.trim()
          if (!tag) continue
          const reset = /^r(.*)$/.exec(tag)
          if (reset) {
            if (hasText) simplified = true
            else {
              const target = styles.get(reset[1]!.trim()) ?? namedStyle
              const alignment = style.align
              style = { ...target?.style }
              delete style.align
              if (alignment !== undefined) style.align = alignment
            }
            continue
          }
          const coordinates = assPosition(tag)
          const envelope = assFade(tag)
          const alpha = /^(alpha|[134]a)&H([\da-f]{1,2})&?$/i.exec(tag)
          if (coordinates) position = coordinates
          else if (envelope) fade ??= envelope
          else if (alpha) {
            if (hasText) simplified = true
            else applyAssAlpha(style, alpha[1]!.toLowerCase(), alpha[2]!)
          } else if (!applyAssOverride(style, tag, hasText)) simplified = true
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
        ...(fade ? { fade } : {}),
      })
      if (simplified) simplifiedCueCount++
      if (cues.length > MAX_SRT_CUES) throw new SubtitleFormatError('Subtitle cue count exceeds limit')
    }
  }
  return { cues, simplifiedCueCount }
}

function assFade(tag: string): SubtitleCue['fade'] {
  const timed = /^fade\(([^()]*)\)$/i.exec(tag)
  if (timed) {
    const values = timed[1]!.split(',').map(value => value.trim())
    if (values.length !== 7 || values.some(value => !/^[+-]?\d+$/.test(value))) return undefined
    const [a1, a2, a3, t1, t2, t3, t4] = values.map(Number) as [number, number, number, number, number, number, number]
    if ([a1, a2, a3].some(value => value < 0 || value > 255) ||
        [t1, t2, t3, t4].some(value => value < -2_147_483_648 || value > 2_147_483_647) ||
        t1 > t2 || t2 > t3 || t3 > t4) return undefined
    return {
      times: [t1 * 1000, t2 * 1000, t3 * 1000, t4 * 1000],
      opacities: [(255 - a1) / 255, (255 - a2) / 255, (255 - a3) / 255],
    }
  }
  const match = /^fad\(\s*(\d+)\s*,\s*(\d+)\s*\)$/i.exec(tag)
  if (!match) return undefined
  const fadeIn = assNumber(match[1], 0, 2_147_483_647)
  const fadeOut = assNumber(match[2], 0, 2_147_483_647)
  return fadeIn === undefined || fadeOut === undefined ? undefined : {
    inTicks: fadeIn * 1000,
    outTicks: fadeOut * 1000,
  }
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

function ssaStyleField(read: (field: string) => string | undefined, field: string): string | undefined {
  if (field === 'alignment') {
    const alignment: Record<string, string> = {1:'1', 2:'2', 3:'3', 4:'9', 5:'7', 6:'8', 7:'9', 8:'3', 9:'4', 10:'5', 11:'6'}
    return alignment[read(field) ?? '']
  }
  if (field !== 'primarycolour' && field !== 'outlinecolour' && field !== 'backcolour') return read(field)
  const color = read(field === 'outlinecolour' ? 'backcolour' : field)
  if (!color) return undefined
  const hex = /^&H([\da-f]{1,8})&?$/i.exec(color)
  const bgr = hex ? Number.parseInt(hex[1]!, 16) : /^-?\d+$/.test(color) ? Number(color) : NaN
  if (!Number.isSafeInteger(bgr) || bgr < -0x80000000 || bgr > 0xffffffff) return undefined
  const alpha = field === 'backcolour' ? 128 : Math.max(0, Math.min(255, Math.trunc(Number(read('alphalevel')) || 0)))
  return `&H${alpha.toString(16).padStart(2, '0')}${(bgr & 0xffffff).toString(16).padStart(6, '0')}`
}

function assColor(value: string | undefined) {
  const hex = /^&H([\da-f]{1,8})&?$/i.exec(value ?? '')?.[1]?.padStart(8, '0')
  if (!hex) return undefined
  return `#${hex.slice(6, 8)}${hex.slice(4, 6)}${hex.slice(2, 4)}${assOpacity(hex.slice(0, 2))}`.toUpperCase()
}

function assOpacity(transparency: string): string {
  return (255 - Number.parseInt(transparency, 16)).toString(16).padStart(2, '0').toUpperCase()
}

function applyAssAlpha(style: AssStyle, channel: string, transparency: string): void {
  const opacity = assOpacity(transparency)
  if (channel === 'alpha' || channel === '1a') style.color = (style.color ?? '#FFFFFFFF').slice(0, 7) + opacity
  if (channel === 'alpha' || channel === '3a') style.strokeColor = (style.strokeColor ?? '#000000FF').slice(0, 7) + opacity
  if (channel === 'alpha' || channel === '4a') style.shadowColor = (style.shadowColor ?? '#000000FF').slice(0, 7) + opacity
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

function applyAssOverride(style: AssStyle, tag: string, hasText: boolean): boolean {
  const geometry = /^(bord|shad|[xy]shad)([+-]?(?:\d+(?:\.\d*)?|\.\d+))$/i.exec(tag)
  if (geometry) {
    const property = geometry[1]!.toLowerCase()
    const distance = assNumber(geometry[2], property === 'xshad' || property === 'yshad' ? -100 : 0, 100)
    if (hasText || distance === undefined) return false
    if (property === 'bord') style.strokeWidthPx = distance
    if (property === 'shad' || property === 'xshad') style.shadowX = distance
    if (property === 'shad' || property === 'yshad') style.shadowY = distance
    if (property !== 'bord') style.shadowColor ??= '#000000FF'
    return true
  }
  const size = /^fs(\d+(?:\.\d+)?)$/i.exec(tag)
  const color = /^([134]?)c(&H[0-9a-f]{1,8}&?)$/i.exec(tag)
  const align = /^an([1-9])$/i.exec(tag)
  const font = /^fn(.+)$/i.exec(tag)
  const face = /^([bi])(-?1|0)$/i.exec(tag)
  const fontSize = assNumber(size?.[1], 1, 512)
  if (fontSize !== undefined) style.fontSizePx = fontSize
  if (color) {
    if (hasText) return false
    const rgb = assColor(color[2])?.slice(0, 7)
    if (!rgb) return false
    const channel = color[1] === '3' ? 'strokeColor' : color[1] === '4' ? 'shadowColor' : 'color'
    style[channel] = rgb + (style[channel]?.slice(7) ?? 'FF')
    return true
  }
  if (align) style.align = assAlign(align[1]) ?? style.align
  if (font) {
    const family = assFont(font[1])
    if (family) style.fontFamily = family
    return !!family
  }
  if (face) style[face[1]!.toLowerCase() === 'b' ? 'bold' : 'italic'] = face[2] !== '0'
  return fontSize !== undefined || !!(align || face)
}

function assTimestamp(value: string): number {
  const match = /^(\d+):([0-5]\d):([0-5]\d)\.(\d{2})$/.exec(value)
  if (!match) throw new SubtitleFormatError(`Invalid ASS timestamp: ${value}`)
  return (Number(match[1]) * 3600 + Number(match[2]) * 60 + Number(match[3])) * COMPOSITION_TIME_BASE
    + Number(match[4]) * (COMPOSITION_TIME_BASE / 100)
}

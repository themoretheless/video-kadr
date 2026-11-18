import { COMPOSITION_TIME_BASE, type TextStyle, type SubtitleKaraokeSpan } from '../composition/types.js'
import { MAX_VISUAL_POSITION } from '../composition/validation.js'
import { MAX_SRT_BYTES, MAX_SRT_CUES, SubtitleFormatError, defaultSubtitleFontSize, formatSrt, parseTimecodedText, type SubtitleCue } from './srt.js'
import { parseWebVtt } from './webvtt.js'

/** Normalize subtitle documents to the composition clock before any mutation. */
export function parseSubtitleDocument(input: string): SubtitleCue[] {
  return parseSubtitleDocumentWithReport(input).cues
}

export function parseSubtitleDocumentWithReport(input: string, canvas?: {width: number; height: number}) {
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
    const parsed = parseAss(body, canvas)
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

function parseAss(body: string, canvas: {width: number; height: number} | undefined) {
  const lines = body.split('\n')
  const { resolution, scaleBorderAndShadow, wrapStyle } = assScriptInfo(lines)
  const typographyScale = canvas && resolution ? canvas.height / resolution.height : 1
  const defaultFontSize = defaultSubtitleFontSize(canvas?.height)
  const positionFits = ({x, y}: {x: number; y: number}) => !canvas || (
    Math.abs(x * (resolution ? canvas.width / resolution.width : 1) - canvas.width / 2) <= MAX_VISUAL_POSITION &&
    Math.abs(y * typographyScale - canvas.height / 2) <= MAX_VISUAL_POSITION
  )
  let section = ''
  let fields: string[] = []
  let styleFields: string[] = []
  let legacySsa = false
  const styles = new Map<string, { style: AssStyle; anchor?: SubtitleCue['anchor']; rotationDegrees?: number; scale?: SubtitleCue['scale']; margins: AssMargins; secondaryColor?: string }>()
  const cues: SubtitleCue[] = []
  let simplifiedCueCount = 0
  for (const line of lines) {
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
        if (name) styles.set(name, {
          style: assStyle(read), anchor: assAnchor(read('alignment')), rotationDegrees: assRotation(read('angle')),
          scale: { x: assScale(read('scalex')), y: assScale(read('scaley')) },
          margins: assMargins(read),
          secondaryColor: assColor(read('secondarycolour')),
        })
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
      const eventMargins = assMargins(field => values[fields.indexOf(field)]?.trim())
      let styleMargins = namedStyle?.margins
      let baseStyle = namedStyle?.style
      let baseSecondaryColor = namedStyle?.secondaryColor ?? ASS_DEFAULT_SECONDARY_COLOR
      let secondaryColor = baseSecondaryColor
      let style = { ...baseStyle }
      let anchor = namedStyle?.anchor
      let hasInlineAlignment = false
      let cueWrapStyle = wrapStyle
      let rotationDegrees = namedStyle?.rotationDegrees
      let baseScale = namedStyle?.scale
      let scale = { ...baseScale }
      let position: SubtitleCue['position']
      let movement: SubtitleCue['movement']
      let fade: SubtitleCue['fade']
      let simplified = false
      let previousBlockEnd = 0
      let textOffset = 0
      let karaokeTime = 0
      const karaokeMarkers: Omit<SubtitleKaraokeSpan, 'end'>[] = []
      let hasText = false
      const dialogue = values.slice(fields.length - 1).join(',')
      const unstyled = dialogue.replace(/(?<!\\)\{([^}]*)\}/g, (whole, block: string, offset: number) => {
        const preceding = dialogue.slice(previousBlockEnd, offset)
        hasText ||= preceding.trim().length > 0
        textOffset += preceding.length
        previousBlockEnd = offset + whole.length
        const previousSecondaryColor = secondaryColor
        for (const rawTag of assTags(block)) {
          const tag = rawTag.trim()
          if (!tag) continue
          const karaoke = /^(kt|kf|ko|k|K)\s*(?:\(([^()]*)\)|(.*))$/.exec(tag)
          const karaokeArgument = (karaoke?.[2] ?? karaoke?.[3])?.trim()
          if (karaoke && (karaokeArgument === '' || /^\+?(?:\d+(?:\.\d*)?|\.\d+)$/.test(karaokeArgument ?? ''))) {
            const command = karaoke[1]
            const centiseconds = karaokeArgument ? Number(karaokeArgument) : command === 'kt' ? 0 : 100
            const ticks = Math.trunc(centiseconds * 10) * 1_000
            if (Number.isSafeInteger(command === 'kt' ? ticks : karaokeTime + ticks)) {
              if (command === 'kt') karaokeTime = ticks
              else {
                karaokeMarkers.push({
                  start: textOffset, startTicks: karaokeTime, endTicks: karaokeTime + ticks,
                  mode: command === 'K' || command === 'kf' ? 'sweep' : command === 'ko' ? 'outline' : 'step',
                })
                karaokeTime += ticks
              }
              if (command === 'K' || command === 'kf') simplified = true
            } else simplified = true
            continue
          }
          const reset = /^r(.*)$/.exec(tag)
          if (reset) {
            const target = styles.get(reset[1]!.trim()) ?? namedStyle
            baseSecondaryColor = target?.secondaryColor ?? ASS_DEFAULT_SECONDARY_COLOR
            secondaryColor = baseSecondaryColor
            if (hasText) simplified = true
            else {
              const alignment = style.align
              baseStyle = target?.style
              style = { ...baseStyle }
              rotationDegrees = target?.rotationDegrees
              baseScale = target?.scale
              scale = { ...baseScale }
              styleMargins = target?.margins
              delete style.align
              if (alignment !== undefined) style.align = alignment
            }
            continue
          }
          const coordinates = assPosition(tag)
          const motion = assMovement(tag)
          const envelope = assFade(tag)
          const alpha = /^(alpha|[1-4]a)(?:&H([\da-f]{1,2})&?)?$/i.exec(tag)
          const secondary = /^2c(?:(&H[\da-f]{1,8}&?))?$/i.exec(tag)
          const rotation = /^frz?([+-]?(?:\d+(?:\.\d*)?|\.\d+))$/i.exec(tag)
          const scaling = /^fsc(?:([xy])(\d+(?:\.\d+)?|\.\d+)?)?$/i.exec(tag)
          const alignment = /^(an|a)(\d+)$/i.exec(tag)
          const wrap = /^q([0-3])$/i.exec(tag)
          const fontSize = /^fs([+-]?(?:\d+(?:\.\d*)?|\.\d+))?$/i.exec(tag)
          if (secondary) {
            const rgb = (assColor(secondary[1]) ?? baseSecondaryColor).slice(0, 7)
            secondaryColor = rgb + secondaryColor.slice(7)
          } else if (wrap) cueWrapStyle = Number(wrap[1])
          else if (fontSize) {
            const rawSize = fontSize[1]
            const size = rawSize && /^[+-]/.test(rawSize)
              ? (style.fontSizePx ?? defaultFontSize / typographyScale) * (1 + Number(rawSize) / 10)
              : Number(rawSize ?? 0)
            if (hasText || !Number.isFinite(size)) simplified = true
            else if (size > 0) style.fontSizePx = size
            else if (baseStyle?.fontSizePx !== undefined) style.fontSizePx = baseStyle.fontSizePx
            else delete style.fontSizePx
          }
          else if (alignment) {
            if (!hasInlineAlignment) {
              const code = alignment[1]?.toLowerCase() === 'an' ? alignment[2] : legacyAssAlignment(alignment[2], true)
              anchor = assAnchor(code) ?? anchor
              style.align = assAlign(code) ?? style.align
              hasInlineAlignment = true
            }
          } else if (coordinates || motion) {
            if (position || movement) simplified = true
            else if (motion) movement = motion
            else position = coordinates
          }
          else if (envelope) fade ??= envelope
          else if (alpha) {
            const channel = alpha[1]!.toLowerCase()
            if (channel === '2a' || channel === 'alpha') {
              const opacity = alpha[2] === undefined ? baseSecondaryColor.slice(7) : assOpacity(alpha[2])
              secondaryColor = secondaryColor.slice(0, 7) + opacity
            }
            if (channel !== '2a') {
              if (hasText) simplified = true
              else applyAssAlpha(style, baseStyle, channel, alpha[2])
            }
          } else if (rotation) {
            const angle = assRotation(rotation[1])
            if (hasText || angle === undefined) simplified = true
            else rotationDegrees = angle
          } else if (scaling) {
            const axis = scaling[1]?.toLowerCase()
            const factor = assScale(scaling[2])
            if (hasText || (scaling[2] !== undefined && factor === undefined)) simplified = true
            else if (axis === 'x') scale.x = factor ?? baseScale?.x
            else if (axis === 'y') scale.y = factor ?? baseScale?.y
            else scale = { ...baseScale }
          } else if (!applyAssOverride(style, baseStyle, tag, hasText)) simplified = true
        }
        const marker = karaokeMarkers.at(-1)
        if (marker?.start === textOffset) {
          karaokeMarkers[karaokeMarkers.length - 1] = { ...marker, secondaryColor }
        } else if (marker && secondaryColor !== previousSecondaryColor) simplified = true
        return ''
      })
      const normalizeText = (value: string) => value.replace(/\\N/g, '\n')
        .replace(/\\n/g, cueWrapStyle === 2 ? '\n' : ' ').replace(/\\h/g, '\u00a0').replace(/\\([{}])/g, '$1')
      const normalizedText = normalizeText(unstyled)
      const withoutLeadingSpace = normalizedText.replace(/^[^\S\u00a0]+/, '')
      const textEnd = Math.max(withoutLeadingSpace.trimEnd().length, withoutLeadingSpace.lastIndexOf('\u00a0') + 1)
      const text = withoutLeadingSpace.slice(0, textEnd)
      const trimOffset = normalizedText.length - withoutLeadingSpace.length
      let rawOffset = 0
      let normalizedOffset = 0
      const karaoke = karaokeMarkers.flatMap((marker, index) => {
        const end = karaokeMarkers[index + 1]?.start ?? unstyled.length
        normalizedOffset += normalizeText(unstyled.slice(rawOffset, marker.start)).length
        const start = Math.max(0, normalizedOffset - trimOffset)
        normalizedOffset += normalizeText(unstyled.slice(marker.start, end)).length
        rawOffset = end
        const stop = Math.min(text.length, normalizedOffset - trimOffset)
        return stop > start ? [{ ...marker, start, end: stop }] : []
      })
      if (style.fontSizePx !== undefined) {
        const scaledSize = style.fontSizePx * typographyScale
        style.fontSizePx = Math.min(512, Math.max(1, scaledSize))
        if (style.fontSizePx !== scaledSize) simplified = true
      }
      if (style.tracking !== undefined) style.tracking *= typographyScale
      if (style.strokeWidthPx !== undefined) {
        const width = style.strokeWidthPx * (scaleBorderAndShadow ? typographyScale : 1)
        style.strokeWidthPx = Math.min(100, width)
        if (style.strokeWidthPx !== width) simplified = true
      }
      if (scaleBorderAndShadow) {
        if (style.shadowX !== undefined) style.shadowX *= typographyScale
        if (style.shadowY !== undefined) style.shadowY *= typographyScale
      }
      const fontSize = style.fontSizePx ?? defaultFontSize
      if (style.tracking !== undefined && (style.tracking < -fontSize || style.tracking > 10 * fontSize)) {
        delete style.tracking
        simplified = true
      }
      if (position && !positionFits(position)) {
        position = undefined
        simplified = true
      }
      if (movement && (!positionFits(movement.from) || !positionFits(movement.to))) {
        movement = undefined
        simplified = true
      }
      const margins = {
        left: eventMargins.left || styleMargins?.left,
        right: eventMargins.right || styleMargins?.right,
        vertical: eventMargins.vertical || styleMargins?.vertical,
      }
      const hasMargins = margins.left !== undefined || margins.right !== undefined || margins.vertical !== undefined
      let textWidth: number | undefined
      if (hasMargins && resolution) {
        const left = margins.left ?? 0
        const right = margins.right ?? 0
        const vertical = margins.vertical ?? 0
        const availableWidth = resolution.width - left - right
        if (availableWidth > 0) textWidth = availableWidth
        else simplified = true
        if (!position && !movement && textWidth !== undefined) {
          const [x, y] = anchor ?? [0.5, 1]
          const candidate = {
            x: left + textWidth * x,
            y: y === 0.5 ? resolution.height / 2 : y === 0 ? vertical : resolution.height - vertical,
          }
          if ((y === 0.5 || vertical < resolution.height) && positionFits(candidate)) position = candidate
          else simplified = true
        }
      }
      cues.push({
        id: `subtitle-${cues.length + 1}`,
        startTicks: assTimestamp(values[fields.indexOf('start')]!.trim()),
        endTicks: assTimestamp(values[fields.indexOf('end')]!.trim()),
        text,
        ...(karaoke.length ? { karaoke } : {}),
        ...(karaoke.length && namedStyle?.secondaryColor ? { karaokeSecondaryColor: namedStyle.secondaryColor } : {}),
        ...(cueWrapStyle === 2 ? { noAutoWrap: true } : {}),
        ...(Object.keys(style).length ? { style } : {}),
        ...(position ? { position } : {}),
        ...(textWidth !== undefined ? { textWidth } : {}),
        ...(movement ? { movement } : {}),
        ...(resolution && (position || movement || textWidth !== undefined) ? { coordinateResolution: resolution } : {}),
        ...(rotationDegrees !== undefined ? { rotationDegrees } : {}),
        ...(scale.x !== undefined || scale.y !== undefined ? { scale } : {}),
        ...(anchor ? { anchor } : {}),
        ...(fade ? { fade } : {}),
      })
      if (simplified) simplifiedCueCount++
      if (cues.length > MAX_SRT_CUES) throw new SubtitleFormatError('Subtitle cue count exceeds limit')
    }
  }
  return { cues, simplifiedCueCount }
}

type AssMargins = {left?: number; right?: number; vertical?: number}

function assMargins(read: (field: string) => string | undefined): AssMargins {
  return {
    left: assNumber(read('marginl'), 0, 2_147_483_647),
    right: assNumber(read('marginr'), 0, 2_147_483_647),
    vertical: assNumber(read('marginv'), 0, 2_147_483_647),
  }
}

function assScriptInfo(lines: readonly string[]) {
  let scriptInfo = false
  let hasScriptInfo = false
  let width: number | undefined
  let height: number | undefined
  let scaleBorderAndShadow = false
  let wrapStyle = 0
  for (const line of lines) {
    const trimmed = line.trim()
    if (/^\[.*\]$/.test(trimmed)) {
      scriptInfo = /^\[script info\]$/i.test(trimmed)
      hasScriptInfo ||= scriptInfo
    }
    if (!scriptInfo) continue
    const scaling = /^ScaledBorderAndShadow:\s*(yes|no)$/i.exec(trimmed)
    const wrap = /^WrapStyle:\s*([0-3])$/i.exec(trimmed)
    if (wrap) wrapStyle = Number(wrap[1])
    if (scaling) scaleBorderAndShadow = scaling[1]?.toLowerCase() === 'yes'
    const field = /^PlayRes([XY]):\s*(\d+)$/i.exec(trimmed)
    if (field?.[1]?.toLowerCase() === 'x') width = assNumber(field[2], 1, 2_147_483_647)
    if (field?.[1]?.toLowerCase() === 'y') height = assNumber(field[2], 1, 2_147_483_647)
  }
  if (width !== undefined && height === undefined) {
    height = width === 1280 ? 1024 : Math.max(1, Math.ceil((width - 1) * 3 / 4))
  } else if (height !== undefined && width === undefined) {
    width = height === 1024 ? 1280 : Math.min(2_147_483_647, Math.floor(height * 4 / 3))
  }
  const resolution = width !== undefined && height !== undefined ? { width, height }
    : hasScriptInfo ? { width: 384, height: 288 } : undefined
  return { resolution, scaleBorderAndShadow, wrapStyle }
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

function assMovement(tag: string): SubtitleCue['movement'] {
  const match = /^move\(([^()]*)\)$/i.exec(tag)
  if (!match) return undefined
  const fields = match[1]!.split(',').map(field => field.trim())
  if (fields.length !== 4 && fields.length !== 6) return undefined
  const [x1, y1, x2, y2] = fields.slice(0, 4).map(field =>
    /^[+-]?(?:\d+(?:\.\d*)?|\.\d+)$/.test(field) ? assNumber(field, -MAX_VISUAL_POSITION, MAX_VISUAL_POSITION) : undefined)
  if (x1 === undefined || y1 === undefined || x2 === undefined || y2 === undefined) return undefined
  const path = { from: { x: x1, y: y1 }, to: { x: x2, y: y2 } }
  if (fields.length === 4) return path
  const [first, second] = fields.slice(4).map(field =>
    /^[+-]?\d+$/.test(field) ? assNumber(field, -2_147_483_648, 2_147_483_647) : undefined)
  if (first === undefined || second === undefined) return undefined
  const start = Math.min(first, second)
  const end = Math.max(first, second)
  return end <= 0 ? path : { ...path, startTicks: start * 1000, endTicks: end * 1000 }
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
  if (field === 'alignment') return legacyAssAlignment(read(field))
  if (field !== 'primarycolour' && field !== 'secondarycolour' && field !== 'outlinecolour' && field !== 'backcolour') return read(field)
  const color = read(field === 'outlinecolour' ? 'backcolour' : field)
  if (!color) return undefined
  const hex = /^&H([\da-f]{1,8})&?$/i.exec(color)
  const bgr = hex ? Number.parseInt(hex[1]!, 16) : /^-?\d+$/.test(color) ? Number(color) : NaN
  if (!Number.isSafeInteger(bgr) || bgr < -0x80000000 || bgr > 0xffffffff) return undefined
  const alpha = field === 'backcolour' ? 128 : Math.max(0, Math.min(255, Math.trunc(Number(read('alphalevel')) || 0)))
  return `&H${alpha.toString(16).padStart(2, '0')}${(bgr & 0xffffff).toString(16).padStart(6, '0')}`
}

function legacyAssAlignment(value: string | undefined, inline = false): string | undefined {
  if (!/^(?:[1-9]|10|11)$/.test(value ?? '')) return undefined
  const code = Number(value)
  if (inline && (code === 4 || code === 8)) return '7'
  return ['', '1', '2', '3', '9', '7', '8', '9', '3', '4', '5', '6'][code]
}

function assColor(value: string | undefined) {
  const hex = /^&H([\da-f]{1,8})&?$/i.exec(value ?? '')?.[1]?.padStart(8, '0')
  if (!hex) return undefined
  return `#${hex.slice(6, 8)}${hex.slice(4, 6)}${hex.slice(2, 4)}${assOpacity(hex.slice(0, 2))}`.toUpperCase()
}

function assOpacity(transparency: string): string {
  return (255 - Number.parseInt(transparency, 16)).toString(16).padStart(2, '0').toUpperCase()
}

function assRotation(value: string | undefined): number | undefined {
  const degrees = assNumber(value, -Number.MAX_VALUE, Number.MAX_VALUE)
  // ASS uses positive counterclockwise angles in the screen plane.
  return degrees === undefined ? undefined : (-(degrees % 360) || 0)
}

function assScale(value: string | undefined): number | undefined {
  const percent = assNumber(value, 1, 1600)
  return percent === undefined ? undefined : percent / 100
}

const ASS_COLOR_CHANNELS = { '1': 'color', '3': 'strokeColor', '4': 'shadowColor' } as const
const ASS_DEFAULT_COLORS = { color: '#FFFFFFFF', strokeColor: '#000000FF', shadowColor: '#00000000' }
const ASS_DEFAULT_SECONDARY_COLOR = '#00FFFFFF'

function applyAssAlpha(style: AssStyle, baseStyle: AssStyle | undefined, channel: string, transparency?: string): void {
  for (const [number, field] of Object.entries(ASS_COLOR_CHANNELS)) {
    if (channel !== 'alpha' && channel !== number + 'a') continue
    const baseColor = baseStyle?.[field] ?? ASS_DEFAULT_COLORS[field]
    const opacity = transparency === undefined ? baseColor.slice(7) : assOpacity(transparency)
    style[field] = (style[field] ?? baseColor).slice(0, 7) + opacity
  }
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
  const size = assNumber(read('fontsize'), Number.MIN_VALUE, Number.MAX_VALUE)
  const color = assColor(read('primarycolour'))
  const family = assFont(read('fontname'))
  const align = assAlign(read('alignment'))
  const outline = assNumber(read('outline'), 0, Number.MAX_VALUE)
  const shadow = assNumber(read('shadow'), 0, 100)
  const spacing = assNumber(read('spacing'), 0, Number.MAX_VALUE)
  if (size !== undefined) style.fontSizePx = size
  if (color) style.color = color
  if (family) style.fontFamily = family
  if (align) style.align = align
  if (spacing !== undefined) style.tracking = spacing
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

function applyAssOverride(style: AssStyle, baseStyle: AssStyle | undefined, tag: string, hasText: boolean): boolean {
  const spacing = /^fsp([+-]?(?:\d+(?:\.\d*)?|\.\d+))?$/i.exec(tag)
  if (spacing) {
    const tracking = spacing[1] === undefined ? baseStyle?.tracking : assNumber(spacing[1], -Number.MAX_VALUE, Number.MAX_VALUE)
    if (hasText || (spacing[1] !== undefined && tracking === undefined)) return false
    if (tracking === undefined) delete style.tracking
    else style.tracking = tracking
    return true
  }
  const geometry = /^(bord|shad|[xy]shad)([+-]?(?:\d+(?:\.\d*)?|\.\d+))?$/i.exec(tag)
  if (geometry) {
    const property = geometry[1]!.toLowerCase()
    const distance = assNumber(geometry[2], property === 'xshad' || property === 'yshad' ? -100 : 0,
      property === 'bord' ? Number.MAX_VALUE : 100)
    if (hasText || (geometry[2] !== undefined && distance === undefined)) return false
    const fields: readonly ('strokeWidthPx' | 'shadowX' | 'shadowY')[] = property === 'bord' ? ['strokeWidthPx']
      : property === 'shad' ? ['shadowX', 'shadowY'] : property === 'xshad' ? ['shadowX'] : ['shadowY']
    for (const field of fields) {
      const value = distance ?? baseStyle?.[field]
      if (value === undefined) delete style[field]
      else style[field] = value
    }
    if (property !== 'bord' && distance !== undefined) style.shadowColor ??= '#000000FF'
    return true
  }
  const color = /^([134]?)c(&H[0-9a-f]{1,8}&?)?$/i.exec(tag)
  const font = /^fn(.*)$/i.exec(tag)
  const face = /^([bi])(-?1|0)?$/i.exec(tag)
  if (color) {
    if (hasText) return false
    const channel = ASS_COLOR_CHANNELS[(color[1] || '1') as keyof typeof ASS_COLOR_CHANNELS]
    const baseColor = baseStyle?.[channel] ?? ASS_DEFAULT_COLORS[channel]
    const rgb = (assColor(color[2]) ?? baseColor).slice(0, 7)
    style[channel] = rgb + (style[channel]?.slice(7) ?? baseStyle?.[channel]?.slice(7) ?? (color[2] ? 'FF' : baseColor.slice(7)))
    return true
  }
  if (font) {
    if (hasText) return false
    const name = font[1]!.trim()
    if (!name || name === '0') {
      if (baseStyle?.fontFamily) style.fontFamily = baseStyle.fontFamily
      else delete style.fontFamily
      return true
    }
    const family = assFont(name)
    if (!family) return false
    style.fontFamily = family
    return true
  }
  if (!face || hasText) return false
  const property = face[1]!.toLowerCase() === 'b' ? 'bold' : 'italic'
  const value = face[2] === undefined ? baseStyle?.[property] : face[2] !== '0'
  if (value === undefined) delete style[property]
  else style[property] = value
  return true
}

function assTimestamp(value: string): number {
  const match = /^(\d+):([0-5]\d):([0-5]\d)\.(\d{2})$/.exec(value)
  if (!match) throw new SubtitleFormatError(`Invalid ASS timestamp: ${value}`)
  return (Number(match[1]) * 3600 + Number(match[2]) * 60 + Number(match[3])) * COMPOSITION_TIME_BASE
    + Number(match[4]) * (COMPOSITION_TIME_BASE / 100)
}

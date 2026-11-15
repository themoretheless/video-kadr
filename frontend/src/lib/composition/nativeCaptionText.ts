import type {NativeNestedProject} from '../api'
import {colorToRgba} from './payload'
import {COMPOSITION_BLEND_MODES, MAX_TEXT_LENGTH, type WireRgba, type WireTextClip, type WireTextStyle} from './types'

export type CaptionAppearance = Pick<WireTextStyle,
  'fontFamily' | 'fontSize' | 'fontWeight' | 'italic' | 'align' | 'tracking' | 'lineHeight' | 'backgroundRadius' |
  'strokeWidth' | 'concatShadow' | 'shadowX' | 'shadowY'> &
  Pick<WireTextClip, 'textBoxWidth' | 'textBoxHeight' | 'opacityMultiplier' | 'blendMode'> & {
    readonly textColor: string
    readonly textAlpha: number
    readonly backgroundColor: string
    readonly backgroundAlpha: number
    readonly strokeColor: string
    readonly strokeAlpha: number
    readonly shadowColor: string
    readonly shadowAlpha: number
    readonly paddingX?: number
    readonly paddingY?: number
  }

function rgbHex({ red, green, blue }: WireRgba): string {
  return '#' + [red, green, blue].map(value => Math.round(value * 255).toString(16).padStart(2, '0')).join('')
}

function editedColor(original: WireRgba, hex: string, alpha: number): WireRgba {
  if (!/^#[0-9a-f]{6}$/i.test(hex) || !(alpha >= 0 && alpha <= 1)) throw new Error('Некорректный цвет или непрозрачность')
  return { ...(rgbHex(original) === hex.toLowerCase() ? original : colorToRgba(hex)), alpha }
}

export function captionAppearance(clip: WireTextClip): CaptionAppearance {
  const { style, textBoxWidth, textBoxHeight, opacityMultiplier, blendMode } = clip
  return {
    fontFamily: style.fontFamily, fontSize: style.fontSize, fontWeight: style.fontWeight,
    italic: style.italic, align: style.align, tracking: style.tracking, lineHeight: style.lineHeight,
    textBoxWidth, textBoxHeight, backgroundRadius: style.backgroundRadius,
    opacityMultiplier, blendMode,
    textColor: rgbHex(style.color), textAlpha: style.color.alpha,
    backgroundColor: rgbHex(style.background), backgroundAlpha: style.background.alpha,
    strokeColor: rgbHex(style.stroke), strokeAlpha: style.stroke.alpha, strokeWidth: style.strokeWidth,
    shadowColor: rgbHex(style.shadow), shadowAlpha: style.shadow.alpha,
    shadowX: style.shadowX, shadowY: style.shadowY, concatShadow: style.concatShadow,
    paddingX: style.backgroundPadding?.[0], paddingY: style.backgroundPadding?.[1],
  }
}

export function sameCaptionAppearance(left: CaptionAppearance, right: CaptionAppearance): boolean {
  return left.fontFamily === right.fontFamily && left.fontSize === right.fontSize &&
    left.fontWeight === right.fontWeight && left.italic === right.italic && left.align === right.align &&
    left.tracking === right.tracking && left.lineHeight === right.lineHeight &&
    left.textBoxWidth === right.textBoxWidth && left.textBoxHeight === right.textBoxHeight &&
    left.opacityMultiplier === right.opacityMultiplier && left.blendMode === right.blendMode &&
    left.textColor === right.textColor && left.textAlpha === right.textAlpha &&
    left.backgroundColor === right.backgroundColor && left.strokeColor === right.strokeColor &&
    left.strokeAlpha === right.strokeAlpha && left.strokeWidth === right.strokeWidth &&
    left.shadowColor === right.shadowColor && left.shadowAlpha === right.shadowAlpha &&
    left.shadowX === right.shadowX && left.shadowY === right.shadowY && left.concatShadow === right.concatShadow &&
    left.backgroundRadius === right.backgroundRadius && left.backgroundAlpha === right.backgroundAlpha &&
    left.paddingX === right.paddingX && left.paddingY === right.paddingY
}

/** Edit a cue without changing its clock, placement, or retained animation. */
export function setNativeCaptionText(
  project: NativeNestedProject, compositionId: string, clipId: string, text: string, expectedText: string,
  appearance?: { value: CaptionAppearance; expected: CaptionAppearance },
): NativeNestedProject {
  if(typeof text!=='string'||!text.trim()||text.includes('\0')||Array.from(text).length>MAX_TEXT_LENGTH)throw new Error(`Текст должен содержать от 1 до ${MAX_TEXT_LENGTH} символов без NUL`)
  const node=Object.hasOwn(project.compositions,compositionId)?project.compositions[compositionId]:undefined
  if(!node)throw new Error('Композиция не найдена')
  const track=node.content.tracks.find(track=>track.clips.some(clip=>clip.id===clipId))
  if(!track||track.kind!=='text'||!node.content.metadata?.captionTextTracks?.includes(track.id))throw new Error('Слой не является субтитром')
  if(track.locked)throw new Error('Дорожка субтитров заблокирована')
  const clip=track.clips.find(clip=>clip.id===clipId)!
  if(clip.text!==expectedText)throw new Error('Текст реплики изменился; сбросьте черновик и повторите')
  const current = appearance && captionAppearance(clip)
  if (appearance && current && !sameCaptionAppearance(current, appearance.expected)) {
    throw new Error('Оформление реплики изменилось; сбросьте черновик и повторите')
  }
  let edited = { ...clip, text }
  if (appearance && current && !sameCaptionAppearance(current, appearance.value)) {
    if (clip.concatTitle) throw new Error('Для титра Concat используйте редактор титров')
    const { textBoxWidth, textBoxHeight, opacityMultiplier, blendMode, textColor, textAlpha, backgroundColor, backgroundAlpha,
      strokeColor, strokeAlpha, shadowColor, shadowAlpha, paddingX, paddingY, ...style } = appearance.value
    if (!(style.fontSize >= 1 && style.fontSize <= 1000)) throw new Error('Размер шрифта должен быть от 1 до 1000 px')
    if (!style.fontFamily.trim()) throw new Error('Укажите семейство шрифта')
    if (blendMode !== undefined && !COMPOSITION_BLEND_MODES.includes(blendMode)) throw new Error('Неизвестный режим наложения')
    if (opacityMultiplier !== undefined && !(opacityMultiplier >= 0 && opacityMultiplier <= 1)) {
      throw new Error('Непрозрачность реплики должна быть от 0 до 1')
    }
    if (!(style.strokeWidth >= 0 && style.strokeWidth <= 100)) throw new Error('Толщина обводки должна быть от 0 до 100 px')
    if (!(Math.hypot(Math.round(Math.abs(style.shadowX)), Math.round(Math.abs(style.shadowY))) <= 1000)) {
      throw new Error('Смещение тени должно быть не больше 1000 px')
    }
    const backgroundPadding = paddingX === undefined && paddingY === undefined
      ? undefined : [paddingX ?? 0, paddingY ?? 0] as const
    edited = { ...edited, textBoxWidth, textBoxHeight, opacityMultiplier, blendMode, style: {
      ...clip.style, ...style, backgroundPadding,
      color: editedColor(clip.style.color, textColor, textAlpha),
      background: editedColor(clip.style.background, backgroundColor, backgroundAlpha),
      stroke: editedColor(clip.style.stroke, strokeColor, strokeAlpha),
      shadow: editedColor(clip.style.shadow, shadowColor, shadowAlpha),
    } }
  } else if (text === clip.text) {
    return project
  }
  const tracks = node.content.tracks.map(candidate => candidate.id === track.id
    ? { ...track, clips: track.clips.map(candidate => candidate.id === clipId ? edited : candidate) }
    : candidate)
  return { ...project, compositions: { ...project.compositions,
    [compositionId]: { ...node, content: { ...node.content, tracks } },
  } }
}

import type {NativeNestedProject} from '../api'
import {MAX_TEXT_LENGTH, type WireTextClip, type WireTextStyle} from './types'

export type CaptionAppearance = Pick<WireTextStyle,
  'fontFamily' | 'fontSize' | 'fontWeight' | 'italic' | 'align' | 'tracking' | 'lineHeight' | 'backgroundRadius'> &
  Pick<WireTextClip, 'textBoxWidth' | 'textBoxHeight'> & {
    readonly backgroundAlpha: number
    readonly paddingX?: number
    readonly paddingY?: number
  }

export function captionAppearance(clip: WireTextClip): CaptionAppearance {
  const { style, textBoxWidth, textBoxHeight } = clip
  return {
    fontFamily: style.fontFamily, fontSize: style.fontSize, fontWeight: style.fontWeight,
    italic: style.italic, align: style.align, tracking: style.tracking, lineHeight: style.lineHeight,
    textBoxWidth, textBoxHeight, backgroundRadius: style.backgroundRadius, backgroundAlpha: style.background.alpha,
    paddingX: style.backgroundPadding?.[0], paddingY: style.backgroundPadding?.[1],
  }
}

export function sameCaptionAppearance(left: CaptionAppearance, right: CaptionAppearance): boolean {
  return left.fontFamily === right.fontFamily && left.fontSize === right.fontSize &&
    left.fontWeight === right.fontWeight && left.italic === right.italic && left.align === right.align &&
    left.tracking === right.tracking && left.lineHeight === right.lineHeight &&
    left.textBoxWidth === right.textBoxWidth && left.textBoxHeight === right.textBoxHeight &&
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
    const { textBoxWidth, textBoxHeight, backgroundAlpha, paddingX, paddingY, ...style } = appearance.value
    if (!(style.fontSize >= 1 && style.fontSize <= 1000)) throw new Error('Размер шрифта должен быть от 1 до 1000 px')
    if (!style.fontFamily.trim()) throw new Error('Укажите семейство шрифта')
    if (!(backgroundAlpha >= 0 && backgroundAlpha <= 1)) throw new Error('Непрозрачность фона должна быть от 0 до 1')
    const backgroundPadding = paddingX === undefined && paddingY === undefined
      ? undefined : [paddingX ?? 0, paddingY ?? 0] as const
    edited = { ...edited, textBoxWidth, textBoxHeight, style: {
      ...clip.style, ...style, background: { ...clip.style.background, alpha: backgroundAlpha }, backgroundPadding,
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

import type {NativeNestedProject} from '../api'
import {MAX_TEXT_LENGTH, type WireTextStyle} from './types'

export type CaptionTypography = Pick<WireTextStyle,
  'fontFamily' | 'fontSize' | 'fontWeight' | 'italic' | 'align' | 'tracking' | 'lineHeight'>

export function captionTypography(style: WireTextStyle): CaptionTypography {
  return {
    fontFamily: style.fontFamily, fontSize: style.fontSize, fontWeight: style.fontWeight,
    italic: style.italic, align: style.align, tracking: style.tracking, lineHeight: style.lineHeight,
  }
}

export function sameCaptionTypography(left: CaptionTypography, right: CaptionTypography): boolean {
  return left.fontFamily === right.fontFamily && left.fontSize === right.fontSize &&
    left.fontWeight === right.fontWeight && left.italic === right.italic && left.align === right.align &&
    left.tracking === right.tracking && left.lineHeight === right.lineHeight
}

/** Edit a cue without changing its clock, placement, or retained animation. */
export function setNativeCaptionText(
  project: NativeNestedProject, compositionId: string, clipId: string, text: string, expectedText: string,
  typography?: { value: CaptionTypography; expected: CaptionTypography },
): NativeNestedProject {
  if(typeof text!=='string'||!text.trim()||text.includes('\0')||Array.from(text).length>MAX_TEXT_LENGTH)throw new Error(`Текст должен содержать от 1 до ${MAX_TEXT_LENGTH} символов без NUL`)
  const node=Object.hasOwn(project.compositions,compositionId)?project.compositions[compositionId]:undefined
  if(!node)throw new Error('Композиция не найдена')
  const track=node.content.tracks.find(track=>track.clips.some(clip=>clip.id===clipId))
  if(!track||track.kind!=='text'||!node.content.metadata?.captionTextTracks?.includes(track.id))throw new Error('Слой не является субтитром')
  if(track.locked)throw new Error('Дорожка субтитров заблокирована')
  const clip=track.clips.find(clip=>clip.id===clipId)!
  if(clip.text!==expectedText)throw new Error('Текст реплики изменился; сбросьте черновик и повторите')
  if (typography && !sameCaptionTypography(clip.style, typography.expected)) {
    throw new Error('Оформление реплики изменилось; сбросьте черновик и повторите')
  }
  const styleChanged = typography && !sameCaptionTypography(clip.style, typography.value)
  if (styleChanged && clip.concatTitle) throw new Error('Для титра Concat используйте редактор титров')
  if (styleChanged && !(typography.value.fontSize >= 1 && typography.value.fontSize <= 1000)) {
    throw new Error('Размер шрифта должен быть от 1 до 1000 px')
  }
  if (styleChanged && !typography.value.fontFamily.trim()) throw new Error('Укажите семейство шрифта')
  if (text === clip.text && !styleChanged) return project
  const edited = { ...clip, text, ...(styleChanged ? { style: { ...clip.style, ...typography.value } } : {}) }
  const tracks = node.content.tracks.map(candidate => candidate.id === track.id
    ? { ...track, clips: track.clips.map(candidate => candidate.id === clipId ? edited : candidate) }
    : candidate)
  return { ...project, compositions: { ...project.compositions,
    [compositionId]: { ...node, content: { ...node.content, tracks } },
  } }
}

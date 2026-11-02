import type { NativeNestedProject, NativeColor } from '../api'
import type { Composition, TextStyle, WireTextStyle } from './types'
import { nativeColorToHex } from './colorAnimation'
import { assertValidComposition } from './validation'
import { assertCompositionLocksPreserved } from './locks'

export interface CaptionStylePatch {
  font_family?: TextStyle['fontFamily'] | null
  font_size_px?: number | null
  color?: NativeColor | null
  outline_width?: number | null
}
const fonts = ['Noto Sans','Arial Unicode MS','DejaVu Sans','Arial']
function validate(patch: CaptionStylePatch, ids?: readonly string[] | null): Set<string> | null {
  if (!patch || typeof patch !== 'object' || Object.keys(patch).some(key => !['font_family','font_size_px','color','outline_width'].includes(key))) throw new Error('Некорректный стиль субтитров')
  if (Object.values(patch).every(value => value === null || value === undefined)) throw new Error('Укажите font_family, font_size_px, color или outline_width')
  if (patch.font_family != null && !fonts.includes(patch.font_family)) throw new Error('Шрифт не поддерживается экспортом')
  if (patch.font_size_px != null && (!Number.isFinite(patch.font_size_px) || patch.font_size_px < 1 || patch.font_size_px > 512)) throw new Error('Размер шрифта должен быть от 1 до 512')
  if (patch.outline_width != null && (!Number.isFinite(patch.outline_width) || patch.outline_width < 0 || patch.outline_width > 100)) throw new Error('Ширина контура должна быть от 0 до 100')
  if (patch.color != null) nativeColorToHex(patch.color)
  if (ids == null) return null
  if (!Array.isArray(ids) || !ids.length || ids.length > 512 || ids.some(id => typeof id !== 'string' || !id)) throw new Error('Укажите непустой список реплик или опустите layer_ids')
  return new Set(ids)
}
function ordinaryStyle(style: TextStyle, patch: CaptionStylePatch): TextStyle {
  return {...style,
    ...(patch.font_family != null ? {fontFamily:patch.font_family} : {}),
    ...(patch.font_size_px != null ? {fontSizePx:patch.font_size_px} : {}),
    ...(patch.color != null ? {color:nativeColorToHex(patch.color)} : {}),
    ...(patch.outline_width != null ? {strokeWidthPx:patch.outline_width,strokeColor:style.strokeWidthPx ? style.strokeColor ?? '#000000FF' : '#000000FF'} : {}),
  }
}
function wireStyle(style: WireTextStyle, patch: CaptionStylePatch): WireTextStyle {
  return {...style,
    ...(patch.font_family != null ? {fontFamily:patch.font_family} : {}),
    ...(patch.font_size_px != null ? {fontSize:patch.font_size_px} : {}),
    ...(patch.color != null ? {color:{red:patch.color.r/255,green:patch.color.g/255,blue:patch.color.b/255,alpha:patch.color.a/255}} : {}),
    ...(patch.outline_width != null ? {strokeWidth:patch.outline_width,stroke:style.strokeWidth > 0 ? {...style.stroke} : {red:0,green:0,blue:0,alpha:1}} : {}),
  }
}
export function restyleCaptions(document: Composition, patch: CaptionStylePatch, ids?: readonly string[] | null) {
  assertValidComposition(document)
  const selected = validate(patch, ids), found = new Set<string>()
  const roles = new Set(document.metadata?.captionTextTracks ?? [])
  let captions = 0, restyled = 0
  const tracks = document.tracks.map(track => {
    if (track.kind !== 'text' || !roles.has(track.id)) return track
    captions += track.clips.length
    return {...track,clips:track.clips.map(clip => {
      if (selected && !selected.has(clip.id)) return clip
      if (track.locked || clip.locked) throw new Error('Субтитр или дорожка заблокированы')
      found.add(clip.id); restyled++
      const {colorAnimation: animation, ...base} = clip
      return {...base,...(patch.color == null && animation ? {colorAnimation:animation} : {}),style:ordinaryStyle(clip.style,patch)}
    })}
  })
  if (selected && [...selected].some(id=>!found.has(id))) throw new Error('Слой не является субтитром; для титров используйте обычный редактор стиля')
  const next = {...document,tracks}
  assertValidComposition(next); assertCompositionLocksPreserved(document,next)
  return {document:next,captions,restyled}
}

/** Visit each authored composition once, including unused ones, never per instance. */
export function restyleNestedCaptions(project: NativeNestedProject, patch: CaptionStylePatch, ids?: readonly string[] | null) {
  const selected = validate(patch,ids), found = new Set<string>()
  let captions = 0, restyled = 0
  const compositions = Object.fromEntries(Object.entries(project.compositions).map(([id,composition]) => {
    const roles = new Set(composition.content.metadata?.captionTextTracks ?? [])
    const tracks = composition.content.tracks.map(track => {
      if (track.kind !== 'text' || !roles.has(track.id)) return track
      captions += track.clips.length
      return {...track,clips:track.clips.map(clip => {
        if (selected && !selected.has(clip.id)) return clip
        if (track.locked) throw new Error('Дорожка субтитров заблокирована')
        if (selected && found.has(clip.id)) throw new Error('ID реплики неоднозначен между композициями')
        found.add(clip.id); restyled++
        const {colorAnimation: animation, ...base} = clip
        return {...base,...(patch.color == null && animation ? {colorAnimation:animation} : {}),style:wireStyle(clip.style,patch)}
      })}
    })
    return [id,{...composition,content:{...composition.content,tracks}}]
  }))
  if (selected && [...selected].some(id=>!found.has(id))) throw new Error('Слой не является субтитром')
  return {document:{...project,compositions},captions,restyled}
}

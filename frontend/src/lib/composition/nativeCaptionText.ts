import type {NativeNestedProject} from '../api'
import {colorToRgba} from './payload'
import {
  COMPOSITION_BLEND_MODES, MAX_TEXT_LENGTH, type WireAnimatableValue, type WireRgba,
  type WireTextClip, type WireTextStyle, type WireTransform,
} from './types'

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
    readonly cropLeft: number
    readonly cropTop: number
    readonly cropRight: number
    readonly cropBottom: number
    readonly flipH: boolean
    readonly flipV: boolean
    readonly x: number
    readonly y: number
    readonly scaleX: number
    readonly scaleY: number
    readonly rotation: number
  }

function rgbHex({ red, green, blue }: WireRgba): string {
  return '#' + [red, green, blue].map(value => Math.round(value * 255).toString(16).padStart(2, '0')).join('')
}

function editedColor(original: WireRgba, hex: string, alpha: number): WireRgba {
  if (!/^#[0-9a-f]{6}$/i.test(hex) || !(alpha >= 0 && alpha <= 1)) throw new Error('Некорректный цвет или непрозрачность')
  return { ...(rgbHex(original) === hex.toLowerCase() ? original : colorToRgba(hex)), alpha }
}

function firstValue(value: WireAnimatableValue): number {
  return value.mode === 'constant' ? value.value : value.track.keyframes[0]?.value ?? 0
}

function mapValue(value: WireAnimatableValue, map: (value: number) => number): WireAnimatableValue {
  if (value.mode === 'constant') return { ...value, value: map(value.value) }
  return { ...value, track: { ...value.track, keyframes: value.track.keyframes.map(key => ({
    ...key, value: map(key.value),
  })) } }
}

function shiftValue(value: WireAnimatableValue, delta: number): WireAnimatableValue {
  return delta === 0 ? value : mapValue(value, sample => sample + delta)
}

function rescaleValue(value: WireAnimatableValue, before: number, after: number): WireAnimatableValue {
  if (before === after) return value
  return before === 0 ? shiftValue(value, after) : mapValue(value, sample => sample * after / before)
}

function positionReference({ x, y, position, concatAnimation }: WireTransform): Pick<CaptionAppearance, 'x' | 'y'> {
  if (concatAnimation) return { x: concatAnimation.rest.x * concatAnimation.canvas[0], y: concatAnimation.rest.y * concatAnimation.canvas[1] }
  if (position?.mode === 'path' && position.path.nodes[0]) return position.path.nodes[0].point
  return { x: firstValue(position?.mode === 'xy' ? position.x : x), y: firstValue(position?.mode === 'xy' ? position.y : y) }
}

function shiftPosition(transform: WireTransform, dx: number, dy: number): WireTransform {
  const { position, concatAnimation } = transform
  const moved = { ...transform, x: shiftValue(transform.x, dx), y: shiftValue(transform.y, dy) }
  if (concatAnimation) {
    const nx = dx / concatAnimation.canvas[0], ny = dy / concatAnimation.canvas[1]
    const keys = { ...concatAnimation.keys }
    for (const [field, delta] of [['offsetX', nx], ['offsetY', ny]] as const) {
      const run = keys[field]
      if (!run || delta === 0) continue
      keys[field] = run.map(key => {
        const value = key.value + delta
        if (!key.derived && Math.abs(value) > 3) throw new Error('Смещение выводит ключ позиции за пределы Concat')
        return { ...key, value }
      })
    }
    return { ...moved, concatAnimation: { ...concatAnimation, keys, rest: {
      ...concatAnimation.rest, x: concatAnimation.rest.x + nx, y: concatAnimation.rest.y + ny,
    } } }
  }
  if (position?.mode === 'xy') {
    return { ...moved, position: { ...position, x: shiftValue(position.x, dx), y: shiftValue(position.y, dy) } }
  }
  if (position?.mode === 'path') {
    return { ...moved, position: { ...position, path: { nodes: position.path.nodes.map(node => ({
      ...node, point: { x: node.point.x + dx, y: node.point.y + dy },
    })) } } }
  }
  return moved
}

function poseReference(transform: WireTransform): Pick<CaptionAppearance, 'scaleX' | 'scaleY' | 'rotation'> {
  const animation = transform.concatAnimation
  if (animation) {
    const scale = Math.max(0.001, animation.rest.scaleX)
    return { scaleX: scale * animation.stretch[0], scaleY: scale * animation.stretch[1], rotation: animation.rest.rotationDegrees }
  }
  return { scaleX: firstValue(transform.scaleX), scaleY: firstValue(transform.scaleY), rotation: firstValue(transform.rotationDegrees) }
}

function changePose(transform: WireTransform, before: CaptionAppearance, after: CaptionAppearance): WireTransform {
  if ((before.scaleX !== after.scaleX && !(Number.isFinite(after.scaleX) && after.scaleX > 0)) ||
      (before.scaleY !== after.scaleY && !(Number.isFinite(after.scaleY) && after.scaleY > 0)) ||
      !Number.isFinite(after.rotation)) throw new Error('Укажите положительный масштаб и конечный угол поворота')
  const turn = after.rotation - before.rotation
  const pose = {
    ...transform,
    scaleX: rescaleValue(transform.scaleX, before.scaleX, after.scaleX),
    scaleY: rescaleValue(transform.scaleY, before.scaleY, after.scaleY),
    rotationDegrees: shiftValue(transform.rotationDegrees, turn),
  }
  const animation = transform.concatAnimation
  if (!animation) return pose
  const scale = Math.max(0.001, animation.rest.scaleX)
  const stretch = [
    before.scaleX === after.scaleX ? animation.stretch[0] : after.scaleX / scale,
    before.scaleY === after.scaleY ? animation.stretch[1] : after.scaleY / scale,
  ] as const
  if ((before.scaleX !== after.scaleX && !(stretch[0] >= 0.1 && stretch[0] <= 10)) ||
      (before.scaleY !== after.scaleY && !(stretch[1] >= 0.1 && stretch[1] <= 10))) {
    throw new Error('Растяжение анимированной реплики Concat должно быть от 0,1 до 10')
  }
  const keys = { ...animation.keys }
  if (keys.rotation && turn !== 0) keys.rotation = keys.rotation.map(key => ({ ...key, value: key.value + turn }))
  return { ...pose, concatAnimation: { ...animation, stretch, keys,
    rest: { ...animation.rest, rotationDegrees: after.rotation },
  } }
}

export function captionAppearance(clip: WireTextClip): CaptionAppearance {
  const { style, textBoxWidth, textBoxHeight, opacityMultiplier, blendMode } = clip
  const preparation = clip.transform.concatPreparation
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
    cropLeft: preparation?.crop[0] ?? 0, cropTop: preparation?.crop[1] ?? 0,
    cropRight: preparation?.crop[2] ?? 0, cropBottom: preparation?.crop[3] ?? 0,
    flipH: preparation?.flipH ?? false, flipV: preparation?.flipV ?? false,
    ...positionReference(clip.transform),
    ...poseReference(clip.transform),
  }
}

function sameCaptionFraming(left: CaptionAppearance, right: CaptionAppearance): boolean {
  return left.cropLeft === right.cropLeft && left.cropTop === right.cropTop &&
    left.cropRight === right.cropRight && left.cropBottom === right.cropBottom &&
    left.flipH === right.flipH && left.flipV === right.flipV
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
    left.paddingX === right.paddingX && left.paddingY === right.paddingY && sameCaptionFraming(left, right) &&
    left.x === right.x && left.y === right.y &&
    left.scaleX === right.scaleX && left.scaleY === right.scaleY && left.rotation === right.rotation
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
      strokeColor, strokeAlpha, shadowColor, shadowAlpha, paddingX, paddingY,
      cropLeft, cropTop, cropRight, cropBottom, flipH, flipV, x, y, scaleX, scaleY, rotation, ...style } = appearance.value
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
    let transform = clip.transform
    if (x !== current.x || y !== current.y) {
      if (!Number.isFinite(x) || !Number.isFinite(y)) throw new Error('Укажите позицию реплики по обеим осям')
      transform = shiftPosition(transform, x - current.x, y - current.y)
    }
    if (scaleX !== current.scaleX || scaleY !== current.scaleY || rotation !== current.rotation) {
      transform = changePose(transform, current, appearance.value)
    }
    if (!sameCaptionFraming(current, appearance.value)) {
      const crop = [cropLeft, cropTop, cropRight, cropBottom] as const
      if (!crop.every(edge => edge >= 0 && edge <= 0.9) ||
          cropLeft + cropRight > 0.9 + Number.EPSILON || cropTop + cropBottom > 0.9 + Number.EPSILON) {
        throw new Error('Кадрирование по каждой оси должно оставлять не меньше 10% кадра')
      }
      const canvas = transform.concatPreparation?.canvas ?? [node.content.canvas.width, node.content.canvas.height] as const
      transform = { ...transform, concatPreparation: crop.some(edge => edge > 0) || flipH || flipV
        ? { crop, canvas, flipH, flipV } : undefined }
    }
    edited = { ...edited, textBoxWidth, textBoxHeight, opacityMultiplier, blendMode, transform, style: {
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

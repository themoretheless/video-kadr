import type { ColorClip,CompositionAnimatableColor,CompositionCanvas } from './types'
import { colorToNative,nativeColorToHex,normalizeColorAnimation,packColorAnimation } from './colorAnimation'
import { validateColorAnimation } from './validation'

/** Intrinsic rectangle parameters; placement/transform belong to the owning clip. */
export interface SolidColorParams {
  readonly width: number
  readonly height: number
  readonly color: string
  readonly colorAnimation?: CompositionAnimatableColor
}

export function createSolidColorParams(canvas: Pick<CompositionCanvas,'width'|'height'>,options: Partial<SolidColorParams> = {}): SolidColorParams {
  return editSolidColorParams({ width: canvas.width,height: canvas.height,color: '#000000ff' },options)
}

export function createSolidColorClip(canvas: Pick<CompositionCanvas,'width'|'height'>,id: string,timelineStartTicks: number,durationTicks: number,options: Partial<SolidColorParams> = {}): ColorClip {
  if (!id || !Number.isSafeInteger(timelineStartTicks) || timelineStartTicks < 0 ||
      !Number.isSafeInteger(durationTicks) || durationTicks <= 0 || !Number.isSafeInteger(timelineStartTicks+durationTicks)) throw new Error('Solid clip requires an ID and a positive safe timeline span')
  const params = createSolidColorParams(canvas,options)
  return { ...params,id,kind: 'color',timelineStartTicks,durationTicks,
    transform: { x: 0,y: 0,width: params.width,height: params.height,fit: 'contain' },opacity: 1 }
}

/** Detached immutable edits preserve all temporal metadata through the shared color admission. */
export function editSolidColorParams(value: SolidColorParams,patch: Partial<SolidColorParams>): SolidColorParams {
  const next = { ...value,...patch }
  if (!Number.isInteger(next.width) || next.width < 1 || next.width > 3840 ||
      !Number.isInteger(next.height) || next.height < 1 || next.height > 2160) throw new Error('Solid dimensions must be integers in 1..3840 × 1..2160')
  const color = nativeColorToHex(colorToNative(next.color))
  if (next.colorAnimation === undefined) return { width: next.width,height: next.height,color }
  const errors: string[] = []
  validateColorAnimation(next.colorAnimation,'colorAnimation',(_code,_path,message) => errors.push(message))
  if (errors.length) throw new Error(errors.join('; '))
  return { width: next.width,height: next.height,color,colorAnimation: normalizeColorAnimation(next.colorAnimation) }
}

/** Transport only; interpolation and preview/export sampling remain native. */
export function solidColorParamsToWire(value: SolidColorParams) {
  const valid = editSolidColorParams(value,{})
  return { width: valid.width,height: valid.height,color: colorToNative(valid.color),
    ...(valid.colorAnimation ? { colorAnimation: packColorAnimation(valid.colorAnimation) } : {}) }
}

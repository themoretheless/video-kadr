import type { CompositionCanvas, GraphicClip, GraphicContent, GraphicLayer } from './types'

export function cloneGraphicValue<T>(value: T): T { return JSON.parse(JSON.stringify(value)) as T }

export function appendGraphicLayer(layers: readonly GraphicLayer[], kind: GraphicContent['kind']): readonly GraphicLayer[] {
  const uid = Math.max(0, ...layers.map(layer => layer.uid)) + 1
  if (!Number.isSafeInteger(uid)) throw new Error('Graphic layer IDs exhausted')
  const content: GraphicContent = kind === 'text'
    ? { kind, parameters: { text: 'Текст', size: 56 } }
    : { kind, parameters: { size: [240, 120] } }
  return [...cloneGraphicValue(layers), { uid, enabled: true, content }]
}

export function createGraphicClip(canvas: Pick<CompositionCanvas, 'width' | 'height'>, id: string, start: number, duration: number, kind: GraphicContent['kind']): GraphicClip {
  if (!id || !Number.isSafeInteger(start) || start < 0 || !Number.isSafeInteger(duration) || duration <= 0 || !Number.isSafeInteger(start + duration)) throw new Error('Invalid graphic placement')
  return { id, kind: 'graphic', timelineStartTicks: start, durationTicks: duration, width: canvas.width, height: canvas.height,
    animationOffsetUs: 0, graphic: { layers: appendGraphicLayer([], kind) },
    transform: { x: 0, y: 0, width: canvas.width, height: canvas.height, fit: 'contain' }, opacity: 1 }
}

/** Indices follow native paint order: the first layer is at the back. */
export function moveGraphicLayer(layers: readonly GraphicLayer[], uid: number, delta: -1 | 1): readonly GraphicLayer[] {
  const index = layers.findIndex(layer => layer.uid === uid)
  const target = index + delta
  if (index < 0 || target < 0 || target >= layers.length) throw new Error('Graphic layer cannot move outside its stack')
  const next = cloneGraphicValue([...layers])
  ;[next[index], next[target]] = [next[target]!, next[index]!]
  return next
}

export function createGraphicTemplateClip(canvas: Pick<CompositionCanvas,'width'|'height'>, template: import('../api').NativeGraphicTemplate, id:string, start:number): GraphicClip {
  if (!Number.isSafeInteger(template.durationUs) || template.durationUs<=0 || !template.canvas.every(value=>Number.isSafeInteger(value)&&value>0)) throw new Error('Invalid graphic template placement')
  const {graphic,roll,responsiveTime,...metadata}=cloneGraphicValue(template)
  return {...createGraphicClip(canvas,id,start,template.durationUs,'text'),width:template.canvas[0],height:template.canvas[1],graphic,
    ...(roll ? {roll} : {}),...(responsiveTime ? {responsiveTime} : {}),template:metadata}
}

import type { NativeNestedProject } from '$lib/api'

export function orderedTimelineIds(document: NativeNestedProject): string[] {
  const ids = Object.keys(document.compositions).sort()
  const order = document.timelineOrder
  if (!order?.length) return ids
  if (order.length !== ids.length || new Set(order).size !== ids.length || order.some(id => !Object.hasOwn(document.compositions, id))) throw new Error('Порядок таймлайнов должен содержать каждую композицию ровно один раз')
  return [...order]
}

export function moveTimeline(document: NativeNestedProject, id: string, index: number): NativeNestedProject {
  const order = orderedTimelineIds(document)
  const from = order.indexOf(id)
  if (from < 0) throw new Error('Таймлайн не найден')
  if (!Number.isSafeInteger(index) || index < 0) throw new Error('Некорректная позиция таймлайна')
  const to = Math.min(index, order.length - 1)
  if (from === to) return document
  order.splice(from, 1); order.splice(to, 0, id)
  return { ...document, timelineOrder: order }
}

import { isStableId } from './validation'
/** Create an independent empty cut using the current timeline's exact canvas. */
export function addTimeline(document: NativeNestedProject, sourceId: string, id: string): NativeNestedProject {
  const order = orderedTimelineIds(document)
  if (!isStableId(id) || Object.hasOwn(document.compositions, id)) throw new Error('Нужен новый уникальный идентификатор таймлайна')
  const source = document.compositions[sourceId]
  if (!source) throw new Error('Исходный таймлайн не найден')
  if (order.length >= 1024) throw new Error('Проект уже содержит 1024 таймлайна')
  const names = new Set(Object.entries(document.compositions).map(([key, value]) => value.content.metadata?.sequenceName || key))
  let number = 1
  while (names.has(`Таймлайн ${number}`)) number++
  const content: import('./types').WireComposition = {
    schemaVersion: 1, timeBase: 1_000_000, sources: {},
    canvas: JSON.parse(JSON.stringify(source.content.canvas)),
    metadata: { sequenceName: `Таймлайн ${number}` },
    tracks: [
      ...[1, 2].map(n => ({ id: `video-${n}`, kind: 'video' as const, name: `Видео ${n}`, hidden: false, muted: false, locked: false, clips: [], transitions: [] })),
      ...[1, 2].map(n => ({ id: `audio-${n}`, kind: 'audio' as const, name: `Звук ${n}`, muted: false, solo: false, locked: false, clips: [] })),
    ],
  }
  return { ...document, timelineOrder: [...order, id], compositions: { ...document.compositions, [id]: { content, instances: [] } } }
}

export function timelineReferences(document: NativeNestedProject, id: string): { compositionId: string; instanceId: string }[] {
  return Object.entries(document.compositions).flatMap(([compositionId, node]) => node.instances.filter(instance => instance.timing.composition === id).map(instance => ({ compositionId, instanceId: instance.id })))
}

export function removeTimeline(document: NativeNestedProject, id: string, removePlacements = false): { document: NativeNestedProject; neighbor: string; invalidatedIds: readonly string[] } {
  const order = orderedTimelineIds(document), index = order.indexOf(id)
  if (index < 0) throw new Error('Таймлайн не найден')
  if (order.length <= 1) throw new Error('Нельзя удалить последний таймлайн')
  const references = timelineReferences(document, id)
  if (references.length && !removePlacements) throw new Error('Таймлайн используется в других композициях. Разрешите удаление его размещений.')
  const remaining = order.filter(key => key !== id)
  const neighbor = order[index + 1] ?? order[index - 1]!
  const compositions = Object.fromEntries(Object.entries(document.compositions).filter(([key]) => key !== id).map(([key, node]) => [key, references.some(reference => reference.compositionId === key) ? { ...node, instances: node.instances.filter(instance => instance.timing.composition !== id) } : node]))
  const invalidatedIds = affectedTimelineIds(document, id)
  let root = document.root
  if (root === id) {
    const referenced = new Set(Object.values(compositions).flatMap(node => node.instances.map(instance => instance.timing.composition)))
    root = [neighbor, ...remaining].find(key => !referenced.has(key)) ?? ''
    if (!root) throw new Error('Не удалось выбрать корневой таймлайн')
  }
  return { document: { ...document, root, timelineOrder: remaining, compositions }, neighbor, invalidatedIds }
}

export function affectedTimelineIds(document: NativeNestedProject, id: string): string[] {
  const affected = new Set([id])
  let changed = true
  while (changed) {
    changed = false
    for (const [key, node] of Object.entries(document.compositions)) if (!affected.has(key) && node.instances.some(instance => affected.has(instance.timing.composition))) { affected.add(key); changed = true }
  }
  return [...affected]
}

export interface TimelineVideoSettings { width: number; height: number; numerator: number; denominator: number }
export function setTimelineVideo(document: NativeNestedProject, id: string, settings: TimelineVideoSettings): NativeNestedProject {
  const target = document.compositions[id]
  if (!Object.hasOwn(document.compositions, id) || !target) throw new Error('Таймлайн не найден')
  const { width, height, numerator, denominator } = settings
  if (!Number.isInteger(width) || width < 2 || width > 3840 || width % 2 || !Number.isInteger(height) || height < 2 || height > 2160 || height % 2) throw new Error('Размер кадра: чётные значения от 2×2 до 3840×2160')
  if (![numerator, denominator].every(value => Number.isSafeInteger(value) && value > 0 && value <= 0xffffffff)) throw new Error('Числитель и знаменатель FPS должны быть положительными 32-битными целыми числами')
  const n = BigInt(numerator), d = BigInt(denominator)
  if (n < d || n > 60n * d) throw new Error('Частота кадров должна быть от 1 до 60 FPS')
  const old = target.content.canvas, rate = old.fpsRate ?? { numerator: old.fpsMilli, denominator: 1000 }
  if (width === old.width && height === old.height && n * BigInt(rate.denominator) === BigInt(rate.numerator) * d) return document
  const canvas = { ...old, width, height, fpsMilli: Number((n * 1000n + d / 2n) / d), fpsRate: { numerator, denominator } }
  return { ...document, compositions: { ...document.compositions, [id]: { ...target, content: { ...target.content, canvas } } } }
}
export function timelineVideoReason(document: NativeNestedProject, id: string, settings: TimelineVideoSettings): string | null {
  try { setTimelineVideo(document, id, settings); return null }
  catch (error) { return error instanceof Error ? error.message : 'Некорректные настройки кадра' }
}

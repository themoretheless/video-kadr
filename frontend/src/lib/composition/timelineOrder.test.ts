import { expect, it } from 'vitest'
import { moveTimeline, orderedTimelineIds } from './timelineOrder'
import type { NativeNestedProject } from '$lib/api'
import { createComposition } from './commands'
import { buildCompositionWireDocument } from './payload'
const content = buildCompositionWireDocument(createComposition({ width: 16, height: 16, fps: 10, backgroundColor: '#000000' }))
const project: NativeNestedProject = { schemaVersion: 1, root: 'root', compositions: { root: { content, instances: [] }, child: { content, instances: [] }, another: { content, instances: [] } } }
it('moves and clamps tabs without changing content or the selected output root', () => {
  const before = JSON.stringify(project)
  expect(orderedTimelineIds(project)).toEqual(['another', 'child', 'root'])
  const moved = moveTimeline(project, 'root', 0)
  expect(orderedTimelineIds(moved)).toEqual(['root', 'another', 'child'])
  expect(moved.compositions).toBe(project.compositions)
  expect(moved.root).toBe('root')
  expect(orderedTimelineIds(moveTimeline(moved, 'root', 100))).toEqual(['another', 'child', 'root'])
  expect(moveTimeline(moved, 'root', 0)).toBe(moved)
  expect(JSON.stringify(project)).toBe(before)
})
it('rejects incomplete, duplicate and unknown orders and invalid positions', () => {
  for (const timelineOrder of [['root'], ['root', 'root', 'child'], ['root', 'child', 'unknown']]) expect(() => orderedTimelineIds({ ...project, timelineOrder })).toThrow()
  expect(() => moveTimeline(project, 'missing', 0)).toThrow()
  for (const at of [-1, 0.5, NaN]) expect(() => moveTimeline(project, 'root', at)).toThrow()
})

import { addTimeline } from './timelineOrder'
it('adds an empty independently named timeline with exact inherited canvas and four initial tracks', () => {
  const source = { ...project, compositions: { ...project.compositions, root: { ...project.compositions.root!, content: { ...content, canvas: { ...content.canvas, fpsMilli: 29970, fpsRate: { numerator: 30000, denominator: 1001 } }, metadata: { sequenceName: 'Таймлайн 1' } } } } }
  const before = JSON.stringify(source)
  const added = addTimeline(source, 'root', 'new-timeline')
  expect(added.root).toBe('root')
  expect(added.timelineOrder).toEqual(['another', 'child', 'root', 'new-timeline'])
  const fresh = added.compositions['new-timeline']!
  expect(fresh.content.canvas).toEqual(source.compositions.root.content.canvas)
  expect(fresh.content.canvas).not.toBe(source.compositions.root.content.canvas)
  expect(fresh.content.metadata?.sequenceName).toBe('Таймлайн 2')
  expect(fresh.content.tracks.map(t => t.kind)).toEqual(['video', 'video', 'audio', 'audio'])
  expect(fresh.content.tracks.every(t => t.clips.length === 0)).toBe(true)
  expect(fresh.content.sources).toEqual({})
  expect(fresh.instances).toEqual([])
  expect(JSON.stringify(source)).toBe(before)
  for (const id of ['root', '', 'bad id']) expect(() => addTimeline(source, 'root', id)).toThrow()
  expect(() => addTimeline(source, 'missing', 'fresh')).toThrow()
  const full = { ...project, compositions: Object.fromEntries(Array.from({ length: 1024 }, (_, i) => [`timeline-${i}`, project.compositions.root!])) }
  expect(() => addTimeline(full, 'timeline-0', 'fresh')).toThrow('1024')
})

import { removeTimeline, timelineReferences } from './timelineOrder'
it('removes a timeline and chooses the next or previous neighbor without mutating the project', () => {
  const before = JSON.stringify(project)
  const removed = removeTimeline(project, 'child')
  expect(removed.neighbor).toBe('root')
  expect(removed.document.timelineOrder).toEqual(['another', 'root'])
  expect(removed.document.compositions.child).toBeUndefined()
  expect(removed.document.root).toBe('root')
  expect(removed.invalidatedIds).toEqual(['child'])
  expect(removeTimeline(project, 'root').neighbor).toBe('child')
  expect(removeTimeline(project, 'root').document.root).toBe('child')
  expect(JSON.stringify(project)).toBe(before)
  const last = { ...project, compositions: { root: project.compositions.root! } }
  expect(() => removeTimeline(last, 'root')).toThrow('последний')
  expect(() => removeTimeline(project, 'absent')).toThrow()
})
const instance = (id: string, composition: string): import('$lib/api').NativeCompositionInstance => ({ id, timing: { composition, timelineStartUs: 0, sourceInUs: 0, sourceOutUs: 1_000_000 }, transform: { x: { mode: 'constant', value: 0 }, y: { mode: 'constant', value: 0 }, scaleX: { mode: 'constant', value: 1 }, scaleY: { mode: 'constant', value: 1 }, rotationDegrees: { mode: 'constant', value: 0 }, anchorX: 0.5, anchorY: 0.5 }, opacity: { mode: 'constant', value: 1 }, blendMode: 'normal' })
it('requires explicit removal of placements and invalidates all ancestors but retains unrelated cuts', () => {
  const d: NativeNestedProject = { ...project, compositions: { root: { ...project.compositions.root!, instances: [instance('outer', 'parent')] }, parent: { ...project.compositions.root!, instances: [instance('inner', 'target')] }, target: project.compositions.root!, other: project.compositions.root! }, timelineOrder: ['root', 'parent', 'target', 'other'] }
  const before = JSON.stringify(d)
  expect(timelineReferences(d, 'target')).toEqual([{ compositionId: 'parent', instanceId: 'inner' }])
  expect(() => removeTimeline(d, 'target')).toThrow('используется')
  const removed = removeTimeline(d, 'target', true)
  expect(removed.document.compositions.parent?.instances).toEqual([])
  expect(removed.document.compositions.root?.instances).toEqual(d.compositions.root?.instances)
  expect(removed.document.compositions.other).toBe(d.compositions.other)
  expect([...removed.invalidatedIds].sort()).toEqual(['parent', 'root', 'target'])
  expect(JSON.stringify(d)).toBe(before)
})
it('chooses an unreferenced root when the nearest neighbor is still used elsewhere', () => {
  const d: NativeNestedProject = { ...project, compositions: { root: project.compositions.root!, child: project.compositions.root!, parent: { ...project.compositions.root!, instances: [instance('shared-child', 'child')] } }, timelineOrder: ['root', 'child', 'parent'] }
  const removed = removeTimeline(d, 'root')
  expect(removed.neighbor).toBe('child')
  expect(removed.document.root).toBe('parent')
  expect(removed.document.compositions.parent?.instances).toEqual(d.compositions.parent?.instances)
})

import { setTimelineVideo, affectedTimelineIds } from './timelineOrder'
it('changes only the chosen canvas with exact fractional FPS and preserves clip clocks and other timelines', () => {
  const before = JSON.stringify(project)
  const changed = setTimelineVideo(project, 'child', { width: 32, height: 24, numerator: 30000, denominator: 1001 })
  const node = changed.compositions.child!
  expect(node.content.canvas).toMatchObject({ width: 32, height: 24, fpsMilli: 29970, fpsRate: { numerator: 30000, denominator: 1001 } })
  expect(node.content.tracks).toBe(project.compositions.child!.content.tracks)
  expect(node.instances).toBe(project.compositions.child!.instances)
  expect(node.content.sources).toBe(project.compositions.child!.content.sources)
  expect(node.content.canvas.background).toBe(project.compositions.child!.content.canvas.background)
  expect(changed.compositions.root).toBe(project.compositions.root)
  expect(changed.root).toBe(project.root)
  expect(setTimelineVideo(changed, 'child', { width: 32, height: 24, numerator: 60000, denominator: 2002 })).toBe(changed)
  expect(JSON.stringify(project)).toBe(before)
  expect(affectedTimelineIds(project, 'child')).toEqual(['child'])
})
it('refuses invalid canvas/rates and handles native boundary values with exact integer arithmetic', () => {
  const valid = { width: 32, height: 24, numerator: 30, denominator: 1 }
  for (const patch of [{ width: 33 }, { width: 3842 }, { height: 2162 }, { width: NaN }, { numerator: 0 }, { denominator: 0 }, { numerator: 0x100000000 }, { numerator: 1.5 }, { numerator: 61 }, { numerator: 1, denominator: 2 }]) expect(() => setTimelineVideo(project, 'root', { ...valid, ...patch })).toThrow()
  expect(() => setTimelineVideo(project, 'missing', valid)).toThrow()
  expect(setTimelineVideo(project, 'root', { width: 3840, height: 2160, numerator: 60, denominator: 1 }).compositions.root!.content.canvas.fpsMilli).toBe(60000)
  expect(setTimelineVideo(project, 'root', { ...valid, numerator: 0xffffffff, denominator: 0xffffffff }).compositions.root!.content.canvas.fpsMilli).toBe(1000)
})

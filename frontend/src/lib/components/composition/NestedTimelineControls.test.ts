// @ts-expect-error Tests need the Svelte client entry.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { beforeEach, expect, it, vi } from 'vitest'
import NestedTimelineControls from './NestedTimelineControls.svelte'
import { closeNestedComposition, nestedComposition, nestedHistory, undoNestedComposition } from '$lib/state/nestedComposition.svelte'
import { buildCompositionWireDocument } from '$lib/composition/payload'
import { createComposition } from '$lib/composition/commands'
vi.mock('$lib/api', () => ({ evaluateNestedFrame: vi.fn(async () => ({ entries: [] })) }))
beforeEach(() => closeNestedComposition())
it('selects, reorders and renames timelines using visible controls and one Undo per edit', async () => {
  const content = buildCompositionWireDocument(createComposition({ width: 16, height: 16, fps: 10, backgroundColor: '#000000' }))
  nestedComposition.document = { schemaVersion: 1, root: 'root', compositions: { child: { content, instances: [] }, root: { content, instances: [] } } }
  nestedComposition.activeCompositionId = 'root'
  const target = document.createElement('div'); document.body.append(target)
  const component = mount(NestedTimelineControls, { target })
  try {
    await tick()
    const select = target.querySelector<HTMLSelectElement>('select')!
    expect([...select.options].map(o => o.value)).toEqual(['child', 'root'])
    target.querySelector<HTMLButtonElement>('[aria-label="Переместить таймлайн раньше"]')!.click()
    await vi.waitFor(() => expect(nestedComposition.document?.timelineOrder).toEqual(['root', 'child']))
    await tick()
    expect([...select.options].map(o => o.value)).toEqual(['root', 'child'])
    expect(nestedHistory.past).toHaveLength(1)
    undoNestedComposition(); await tick()
    expect([...select.options].map(o => o.value)).toEqual(['child', 'root'])
    select.value = 'child'; select.dispatchEvent(new Event('change', { bubbles: true })); await tick()
    expect(nestedComposition.activeCompositionId).toBe('child')
    const name = target.querySelector<HTMLInputElement>('[aria-label="Имя таймлайна"]')!
    name.value = '  Alternate cut  '; name.dispatchEvent(new Event('change', { bubbles: true }))
    await vi.waitFor(() => expect(nestedComposition.document?.compositions.child?.content.metadata?.sequenceName).toBe('Alternate cut'))
    await tick()
    expect([...select.options].map(o => o.textContent)).toContain('Alternate cut')
    expect(nestedHistory.past).toHaveLength(1)
    undoNestedComposition(); await tick()
    expect(nestedComposition.document?.compositions.child?.content.metadata?.sequenceName).toBeUndefined()
  } finally { await unmount(component); target.remove() }
})
it('adds a timeline from the toolbar and restores the original selection with Undo', async () => {
  const content = buildCompositionWireDocument(createComposition({ width: 16, height: 16, fps: 10, backgroundColor: '#000000' }))
  nestedComposition.document = { schemaVersion: 1, root: 'root', compositions: { root: { content, instances: [] } } }
  nestedComposition.activeCompositionId = 'root'
  const target = document.createElement('div'); document.body.append(target)
  const component = mount(NestedTimelineControls, { target })
  try {
    await tick()
    const button = [...target.querySelectorAll<HTMLButtonElement>('button')].find(b => b.textContent === 'Добавить таймлайн')!
    button.click()
    await vi.waitFor(() => expect(nestedComposition.document?.timelineOrder).toHaveLength(2))
    const id = nestedComposition.activeCompositionId!
    expect(id).not.toBe('root')
    expect(nestedComposition.document?.compositions[id]?.content.canvas).toEqual(content.canvas)
    await tick()
    expect(target.querySelector<HTMLSelectElement>('select')?.value).toBe(id)
    expect(nestedHistory.past).toHaveLength(1)
    undoNestedComposition(); await tick()
    expect(target.querySelector<HTMLSelectElement>('select')?.value).toBe('root')
    expect(Object.keys(nestedComposition.document!.compositions)).toEqual(['root'])
  } finally { await unmount(component); target.remove() }
})
it('deletes the selected timeline and disables removal of the last remaining timeline', async () => {
  const content = buildCompositionWireDocument(createComposition({ width: 16, height: 16, fps: 10, backgroundColor: '#000000' }))
  nestedComposition.document = { schemaVersion: 1, root: 'root', timelineOrder: ['root', 'child'], compositions: { root: { content, instances: [] }, child: { content, instances: [] } } }
  nestedComposition.activeCompositionId = 'child'
  const target = document.createElement('div'); document.body.append(target)
  const component = mount(NestedTimelineControls, { target })
  try {
    await tick()
    const button = [...target.querySelectorAll<HTMLButtonElement>('button')].find(b => b.textContent === 'Удалить таймлайн')!
    expect(button.disabled).toBe(false)
    button.click()
    await vi.waitFor(() => expect(nestedComposition.document?.compositions.child).toBeUndefined())
    await tick()
    expect(target.querySelector<HTMLSelectElement>('select')?.value).toBe('root')
    expect(button.disabled).toBe(true)
    undoNestedComposition(); await tick()
    expect(target.querySelector<HTMLSelectElement>('select')?.value).toBe('child')
    expect(button.disabled).toBe(false)
  } finally { await unmount(component); target.remove() }
})
it('requires the explicit placement-removal checkbox before deleting a referenced timeline', async () => {
  const content = buildCompositionWireDocument(createComposition({ width: 16, height: 16, fps: 10, backgroundColor: '#000000' }))
  const placement: import('$lib/api').NativeCompositionInstance = { id: 'placed', timing: { composition: 'child', timelineStartUs: 0, sourceInUs: 0, sourceOutUs: 1_000_000 }, transform: { x: { mode: 'constant', value: 0 }, y: { mode: 'constant', value: 0 }, scaleX: { mode: 'constant', value: 1 }, scaleY: { mode: 'constant', value: 1 }, rotationDegrees: { mode: 'constant', value: 0 }, anchorX: 0.5, anchorY: 0.5 }, opacity: { mode: 'constant', value: 1 }, blendMode: 'normal' }
  const original = { schemaVersion: 1 as const, root: 'root', compositions: { root: { content, instances: [placement] }, child: { content, instances: [] } } }
  nestedComposition.document = original; nestedComposition.activeCompositionId = 'child'
  const target = document.createElement('div'); document.body.append(target)
  const component = mount(NestedTimelineControls, { target })
  try {
    await tick()
    const button = [...target.querySelectorAll<HTMLButtonElement>('button')].find(b => b.textContent === 'Удалить таймлайн')!
    expect(button.disabled).toBe(true)
    const checkbox = target.querySelector<HTMLInputElement>('[aria-label="Удалить также размещения таймлайна"]')!
    checkbox.checked = true; checkbox.dispatchEvent(new Event('change', { bubbles: true })); await tick()
    expect(button.disabled).toBe(false)
    button.click()
    await vi.waitFor(() => expect(nestedComposition.document?.compositions.child).toBeUndefined())
    expect(nestedComposition.document?.compositions.root?.instances).toEqual([])
    undoNestedComposition(); await tick()
    expect(nestedComposition.document).toEqual(original)
    expect(target.querySelector<HTMLInputElement>('[aria-label="Удалить также размещения таймлайна"]')?.checked).toBe(false)
  } finally { await unmount(component); target.remove() }
})

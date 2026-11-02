// @ts-expect-error Tests need the Svelte client entry.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { expect, it, vi } from 'vitest'
import NestedTimelineVideoSettings from './NestedTimelineVideoSettings.svelte'
import { closeNestedComposition, nestedComposition, nestedHistory, undoNestedComposition } from '$lib/state/nestedComposition.svelte'
import { buildCompositionWireDocument } from '$lib/composition/payload'
import { createComposition } from '$lib/composition/commands'
vi.mock('$lib/api', () => ({ evaluateNestedFrame: vi.fn(async () => ({ entries: [] })) }))
it('applies all canvas fields as one edit, restores the draft with Undo and refuses odd dimensions', async () => {
  closeNestedComposition()
  const content = buildCompositionWireDocument(createComposition({ width: 16, height: 16, fps: 10, backgroundColor: '#000000' }))
  const before = { schemaVersion: 1 as const, root: 'root', compositions: { root: { content, instances: [] } } }
  nestedComposition.document = before; nestedComposition.activeCompositionId = 'root'
  const target = document.createElement('div'); document.body.append(target)
  const component = mount(NestedTimelineVideoSettings, { target })
  const input = async (label: string, value: string) => { const field = target.querySelector<HTMLInputElement>(`[aria-label="${label}"]`)!; field.value = value; field.dispatchEvent(new Event('input', { bubbles: true })); await tick() }
  try {
    await tick()
    await input('Ширина кадра таймлайна', '32'); await input('Высота кадра таймлайна', '24'); await input('Числитель FPS таймлайна', '30000'); await input('Знаменатель FPS таймлайна', '1001')
    const button = target.querySelector<HTMLButtonElement>('button')!
    expect(button.disabled).toBe(false)
    button.click()
    await vi.waitFor(() => expect(nestedComposition.document?.compositions.root?.content.canvas.fpsRate).toEqual({ numerator: 30000, denominator: 1001 }))
    expect(nestedHistory.past).toHaveLength(1)
    undoNestedComposition(); await tick()
    expect(nestedComposition.document).toEqual(before)
    expect(target.querySelector<HTMLInputElement>('[aria-label="Ширина кадра таймлайна"]')?.value).toBe('16')
    await input('Ширина кадра таймлайна', '33')
    expect(button.disabled).toBe(true)
    expect(target.querySelector('[role="status"]')?.textContent).toContain('чётные')
  } finally { await unmount(component); target.remove(); closeNestedComposition() }
})
it('offers common fractional FPS as a preset without requiring manual numerator/denominator input', async () => {
  closeNestedComposition()
  const content = buildCompositionWireDocument(createComposition({ width: 16, height: 16, fps: 30, backgroundColor: '#000000' }))
  nestedComposition.document = { schemaVersion: 1, root: 'root', compositions: { root: { content, instances: [] } } }
  const target = document.createElement('div'); document.body.append(target)
  const component = mount(NestedTimelineVideoSettings, { target })
  try {
    await tick()
    expect(target.querySelector('details')?.open).toBe(false)
    const select = target.querySelector<HTMLSelectElement>('[aria-label="Частота кадров таймлайна"]')!
    expect(select.value).toBe('30/1')
    select.value = '30000/1001'; select.dispatchEvent(new Event('change', { bubbles: true })); await tick()
    target.querySelector<HTMLButtonElement>('button')!.click()
    await vi.waitFor(() => expect(nestedComposition.document?.compositions.root?.content.canvas.fpsRate).toEqual({ numerator: 30000, denominator: 1001 }))
    expect(nestedHistory.past).toHaveLength(1)
    undoNestedComposition(); await tick()
    expect(select.value).toBe('30/1')
  } finally { await unmount(component); target.remove(); closeNestedComposition() }
})

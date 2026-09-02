// @ts-expect-error Vitest's SSR condition exposes server-only mount; this test needs Svelte's client entry.
import { mount, tick, unmount } from '../../../node_modules/svelte/src/index-client.js'
import { afterEach, describe, expect, it } from 'vitest'
import OverlayHarness from './OverlayHarness.test.svelte'
import { overlayMiddleware } from './controller.js'

let target: HTMLDivElement
let component: ReturnType<typeof mount> | null = null

afterEach(async () => {
  if (component) await unmount(component)
  component = null
  target?.remove()
})

describe('Overlay', () => {
  it('uses collision middleware', () => {
    expect(overlayMiddleware().map((item) => item.name)).toEqual(['offset', 'flip', 'shift'])
  })

  it.each(['Escape', 'outside'] as const)('closes on %s and restores anchor focus', async (mode) => {
    target = document.createElement('div')
    document.body.append(target)
    component = mount(OverlayHarness, { target })
    await tick()
    const anchor = target.querySelector<HTMLButtonElement>('button')!
    anchor.click()
    await tick()
    expect(target.querySelector('[role="dialog"]')).not.toBeNull()
    if (mode === 'Escape') window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    else document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }))
    await tick()
    await Promise.resolve()
    expect(target.querySelector('[role="dialog"]')).toBeNull()
    expect(document.activeElement).toBe(anchor)
  })
})

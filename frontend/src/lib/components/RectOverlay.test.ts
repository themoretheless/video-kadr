// @ts-expect-error Vitest's SSR condition exposes server-only mount; this test needs Svelte's client entry.
import { mount, tick, unmount } from '../../../node_modules/svelte/src/index-client.js'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { state } from '$lib/state/store.svelte.js'
import RectOverlay from './RectOverlay.svelte'

let target: HTMLDivElement
let component: ReturnType<typeof mount> | null = null

beforeEach(() => {
  state.video = {
    id: 'overlay-source', url: '/files/source.mp4', filename: 'source.mp4', mediaType: 'video',
    duration: 10, width: 1280, height: 720,
  }
  target = document.createElement('div')
  document.body.append(target)
})

afterEach(async () => {
  if (component) await unmount(component)
  component = null
  state.video = null
  target.remove()
})

describe('RectOverlay accessibility', () => {
  it('moves and resizes with arrow keys as one bounded interaction', async () => {
    const onrectchange = vi.fn()
    const oninteractionstart = vi.fn()
    const oninteractionend = vi.fn()
    component = mount(RectOverlay, {
      target,
      props: {
        rect: { x: 100, y: 100, w: 320, h: 180 },
        onrectchange,
        oninteractionstart,
        oninteractionend,
      },
    })
    await tick()

    const move = target.querySelector<HTMLElement>('.crop-rect')
    const southEast = target.querySelector<HTMLElement>('.crop-handle.se')
    expect(move?.getAttribute('data-scene-layer')).toBe('overlays')
    expect(southEast?.getAttribute('data-scene-layer')).toBe('handles')
    if (!move || !southEast) throw new Error('Overlay controls not found')

    move.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true }))
    expect(onrectchange).toHaveBeenLastCalledWith({ x: 101, y: 100, w: 320, h: 180 })
    southEast.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', shiftKey: true, bubbles: true }))
    expect(onrectchange).toHaveBeenLastCalledWith({ x: 100, y: 100, w: 320, h: 190 })
    expect(oninteractionstart).toHaveBeenCalledTimes(2)
    expect(oninteractionend).toHaveBeenCalledTimes(2)
  })
})

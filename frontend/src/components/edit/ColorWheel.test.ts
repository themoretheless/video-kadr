import { createApp, defineComponent, h, nextTick, ref, type App } from 'vue'
import { afterEach, describe, expect, it } from 'vitest'
import type { ColorWheelChannels } from '../../types'
import ColorWheel from './ColorWheel.vue'

let app: App<Element> | null = null
let host: HTMLElement | null = null

afterEach(() => {
  app?.unmount()
  host?.remove()
  app = null
  host = null
})

function mountWheel() {
  const model = ref<ColorWheelChannels>({ master: 0, red: 0, green: 0, blue: 0 })
  const interactions: string[] = []
  const Root = defineComponent({
    setup: () => () => h(ColorWheel, {
      label: 'Lift',
      modelValue: model.value,
      'onUpdate:modelValue': (value: ColorWheelChannels) => { model.value = value },
      onInteractionStart: () => interactions.push('start'),
      onInteractionEnd: () => interactions.push('end'),
    }),
  })
  host = document.createElement('div')
  document.body.append(host)
  app = createApp(Root)
  app.mount(host)
  return { model, interactions, element: host }
}

describe('ColorWheel', () => {
  it('supports accessible keyboard, pointer, numeric, and neutral reset interactions', async () => {
    const { model, interactions, element } = mountWheel()
    const surface = element.querySelector<HTMLElement>('[role="slider"]')!
    expect(surface.getAttribute('aria-label')).toBe('Lift: оттенок и насыщенность')
    expect(surface.getAttribute('aria-valuetext')).toContain('насыщенность 0%')

    surface.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true }))
    await nextTick()
    expect(model.value).toEqual({ master: 0, red: 0.04, green: -0.02, blue: -0.02 })

    surface.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowUp', altKey: true, bubbles: true }))
    await nextTick()
    expect(model.value.blue).toBeGreaterThan(model.value.green)

    Object.defineProperty(surface, 'getBoundingClientRect', {
      value: () => ({ left: 0, top: 0, width: 100, height: 100, right: 100, bottom: 100, x: 0, y: 0, toJSON: () => ({}) }),
    })
    surface.dispatchEvent(new PointerEvent('pointerdown', {
      pointerId: 7, button: 0, clientX: 50, clientY: 50, bubbles: true,
    }))
    window.dispatchEvent(new PointerEvent('pointermove', {
      pointerId: 7, clientX: 100, clientY: 50,
    }))
    window.dispatchEvent(new PointerEvent('pointerup', { pointerId: 7 }))
    await nextTick()
    expect(model.value).toEqual({ master: 0, red: 1, green: -0.5, blue: -0.5 })

    const master = element.querySelector<HTMLInputElement>('input[aria-label="Lift: Master"]')!
    master.focus()
    master.value = '5'
    master.dispatchEvent(new InputEvent('input', { bubbles: true }))
    master.blur()
    await nextTick()
    expect(model.value.master).toBe(1)

    const beforeHomeStarts = interactions.filter(value => value === 'start').length
    const beforeHomeEnds = interactions.filter(value => value === 'end').length
    surface.dispatchEvent(new KeyboardEvent('keydown', { key: 'Home', bubbles: true }))
    await nextTick()
    expect(model.value).toEqual({ master: 0, red: 0, green: 0, blue: 0 })
    expect(interactions.filter(value => value === 'start')).toHaveLength(beforeHomeStarts + 1)
    expect(interactions.filter(value => value === 'end')).toHaveLength(beforeHomeEnds + 1)

    master.focus()
    master.value = '0.5'
    master.dispatchEvent(new InputEvent('input', { bubbles: true }))
    master.blur()
    surface.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true }))
    await nextTick()

    surface.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }))
    await nextTick()
    expect(model.value).toEqual({ master: 0, red: 0, green: 0, blue: 0 })
    expect(interactions.filter(value => value === 'start').length)
      .toBe(interactions.filter(value => value === 'end').length)
  })

  it('treats a native double click as one reset transaction without committing either click', async () => {
    const { model, interactions, element } = mountWheel()
    model.value = { master: 0.5, red: 0.2, green: -0.1, blue: -0.1 }
    await nextTick()
    const surface = element.querySelector<HTMLElement>('[role="slider"]')!
    Object.defineProperty(surface, 'getBoundingClientRect', {
      value: () => ({ left: 0, top: 0, width: 100, height: 100, right: 100, bottom: 100, x: 0, y: 0, toJSON: () => ({}) }),
    })

    for (const detail of [1, 2]) {
      surface.dispatchEvent(new PointerEvent('pointerdown', {
        pointerId: detail, button: 0, clientX: 100, clientY: 50, bubbles: true,
      }))
      window.dispatchEvent(new PointerEvent('pointerup', { pointerId: detail }))
    }
    surface.dispatchEvent(new MouseEvent('dblclick', { detail: 2, bubbles: true }))
    await nextTick()

    expect(model.value).toEqual({ master: 0, red: 0, green: 0, blue: 0 })
    expect(interactions).toEqual(['start', 'end'])
  })

  it('ignores pointer jitter and cancellation without changing the model or history', async () => {
    const { model, interactions, element } = mountWheel()
    const surface = element.querySelector<HTMLElement>('[role="slider"]')!
    Object.defineProperty(surface, 'getBoundingClientRect', {
      value: () => ({ left: 0, top: 0, width: 100, height: 100, right: 100, bottom: 100, x: 0, y: 0, toJSON: () => ({}) }),
    })
    surface.dispatchEvent(new PointerEvent('pointerdown', {
      pointerId: 9, button: 0, clientX: 50, clientY: 50, bubbles: true,
    }))
    window.dispatchEvent(new PointerEvent('pointermove', {
      pointerId: 9, clientX: 51, clientY: 51,
    }))
    window.dispatchEvent(new PointerEvent('pointercancel', { pointerId: 9 }))
    await nextTick()
    expect(model.value).toEqual({ master: 0, red: 0, green: 0, blue: 0 })
    expect(interactions).toEqual([])
  })
})

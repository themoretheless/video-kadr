import { createApp, defineComponent, h, nextTick, ref, type App } from 'vue'
import { afterEach, describe, expect, it } from 'vitest'
import {
  HSL_SELECTIVE_DEFAULTS,
  type HslSelective,
} from '../../domain/hsl-selective'
import HslSelectiveControl from './HslSelectiveControl.vue'

let app: App<Element> | null = null
let host: HTMLElement | null = null

afterEach(() => {
  app?.unmount()
  host?.remove()
  app = null
  host = null
})

function freshValue(): HslSelective {
  return {
    selection: { ...HSL_SELECTIVE_DEFAULTS.selection },
    adjustment: { ...HSL_SELECTIVE_DEFAULTS.adjustment },
  }
}

function mountControl() {
  const model = ref(freshValue())
  const maskPreview = ref(false)
  const interactions: string[] = []
  const Root = defineComponent({
    setup: () => () => h(HslSelectiveControl, {
      modelValue: model.value,
      maskPreview: maskPreview.value,
      'onUpdate:modelValue': (value: HslSelective) => { model.value = value },
      'onUpdate:maskPreview': (value: boolean) => { maskPreview.value = value },
      onInteractionStart: () => interactions.push('start'),
      onInteractionEnd: () => interactions.push('end'),
    }),
  })
  host = document.createElement('div')
  document.body.append(host)
  app = createApp(Root)
  app.mount(host)
  return { model, maskPreview, interactions, element: host }
}

describe('HslSelectiveControl', () => {
  it('exposes one-dimensional controls with circular keyboard behavior and grouped repeat undo', async () => {
    const { model, interactions, element } = mountControl()
    const ring = element.querySelector<HTMLElement>('[role="slider"]')!
    expect(ring.getAttribute('aria-label')).toBe('Центр диапазона оттенка')
    expect(ring.getAttribute('aria-valuetext')).toContain('core ±30°')

    ring.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowLeft', bubbles: true }))
    ring.dispatchEvent(new KeyboardEvent('keyup', { key: 'ArrowLeft', bubbles: true }))
    await nextTick()
    expect(model.value.selection.centerDegrees).toBe(359)

    ring.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', shiftKey: true, bubbles: true }))
    ring.dispatchEvent(new KeyboardEvent('keyup', { key: 'ArrowRight', shiftKey: true, bubbles: true }))
    await nextTick()
    expect(model.value.selection.centerDegrees).toBe(9)

    const hue = element.querySelector<HTMLInputElement>('input[type="range"][aria-label="Сдвиг оттенка"]')!
    hue.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true }))
    await nextTick()
    hue.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', repeat: true, bubbles: true }))
    await nextTick()
    expect(model.value.adjustment.hueDegrees).toBe(2)
    expect(interactions.filter(event => event === 'start')).toHaveLength(3)
    expect(interactions.filter(event => event === 'end')).toHaveLength(2)
    hue.dispatchEvent(new KeyboardEvent('keyup', { key: 'ArrowRight', bubbles: true }))
    expect(interactions.filter(event => event === 'start')).toHaveLength(3)
    expect(interactions.filter(event => event === 'end')).toHaveLength(3)

    ring.dispatchEvent(new KeyboardEvent('keydown', { key: 'Home', bubbles: true }))
    ring.dispatchEvent(new KeyboardEvent('keyup', { key: 'Home', bubbles: true }))
    await nextTick()
    expect(model.value.selection.centerDegrees).toBe(0)
  })

  it('sanitizes numeric sessions, supports Escape rollback, and keeps mask UI-only', async () => {
    const { model, maskPreview, interactions, element } = mountControl()
    const halfWidth = element.querySelector<HTMLInputElement>('input[aria-label="Полуширина core, числом"]')!
    halfWidth.focus()
    halfWidth.value = '170'
    halfWidth.dispatchEvent(new InputEvent('input', { bubbles: true }))
    halfWidth.blur()
    await nextTick()
    expect(model.value.selection).toEqual({ centerDegrees: 0, halfWidthDegrees: 170, featherDegrees: 10 })

    const saturation = element.querySelector<HTMLInputElement>('input[aria-label="Насыщенность, числом"]')!
    saturation.focus()
    saturation.value = '25'
    saturation.dispatchEvent(new InputEvent('input', { bubbles: true }))
    saturation.blur()
    await nextTick()
    expect(model.value.adjustment.saturation).toBe(0.25)

    saturation.focus()
    saturation.value = '80'
    saturation.dispatchEvent(new InputEvent('input', { bubbles: true }))
    saturation.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    await nextTick()
    expect(model.value.adjustment.saturation).toBe(0.25)

    const before = JSON.stringify(model.value)
    const beforeInteractions = interactions.length
    const mask = element.querySelector<HTMLButtonElement>('button[aria-controls="color-preview-surface"]')!
    mask.click()
    await nextTick()
    expect(maskPreview.value).toBe(true)
    expect(JSON.stringify(model.value)).toBe(before)
    expect(interactions).toHaveLength(beforeInteractions)
    expect(mask.getAttribute('aria-pressed')).toBe('true')
  })

  it('coalesces a ring drag and rolls back pointer cancellation', async () => {
    const { model, interactions, element } = mountControl()
    const ring = element.querySelector<HTMLElement>('[role="slider"]')!
    Object.defineProperty(ring, 'getBoundingClientRect', {
      value: () => ({ left: 0, top: 0, width: 100, height: 100, right: 100, bottom: 100, x: 0, y: 0, toJSON: () => ({}) }),
    })

    ring.dispatchEvent(new PointerEvent('pointerdown', {
      pointerId: 7, button: 0, clientX: 100, clientY: 50, bubbles: true,
    }))
    window.dispatchEvent(new PointerEvent('pointermove', {
      pointerId: 7, clientX: 50, clientY: 100,
    }))
    window.dispatchEvent(new PointerEvent('pointerup', { pointerId: 7 }))
    await nextTick()
    expect(model.value.selection.centerDegrees).toBe(180)
    expect(interactions).toEqual(['start', 'end'])

    ring.dispatchEvent(new PointerEvent('pointerdown', {
      pointerId: 8, button: 0, clientX: 0, clientY: 50, bubbles: true,
    }))
    expect(model.value.selection.centerDegrees).toBe(270)
    window.dispatchEvent(new PointerEvent('pointercancel', { pointerId: 8 }))
    await nextTick()
    expect(model.value.selection.centerDegrees).toBe(180)
    expect(interactions).toEqual(['start', 'end', 'start', 'end'])
  })
})

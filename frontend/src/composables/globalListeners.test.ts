import { describe, expect, it, vi } from 'vitest'
import { listenMany } from './globalListeners.js'
import { observeResize } from './resizeObserver.js'

describe('lifecycle-safe browser composables', () => {
  it('disposes global listeners exactly once', () => {
    const target = new EventTarget()
    const listener = vi.fn()
    const dispose = listenMany([[target, 'change', listener]])
    target.dispatchEvent(new Event('change'))
    dispose()
    dispose()
    target.dispatchEvent(new Event('change'))
    expect(listener).toHaveBeenCalledTimes(1)
  })

  it('disconnects resize observation', () => {
    const disconnect = vi.fn()
    const observe = vi.fn()
    class FakeObserver {
      constructor(callback: ResizeObserverCallback) { void callback }
      observe = observe
      disconnect = disconnect
    }
    const dispose = observeResize(document.body, vi.fn(), FakeObserver as never)
    expect(observe).toHaveBeenCalledWith(document.body)
    dispose()
    expect(disconnect).toHaveBeenCalledOnce()
  })
})

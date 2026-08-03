import { describe, expect, it, vi } from 'vitest'
import { boundedEnginePhase, BrowserEngineTimeoutError } from './browser-engine-guard'

describe('disposable browser engine watchdog', () => {
  it('terminates a stalled engine before rejecting the phase', async () => {
    vi.useFakeTimers()
    const terminate = vi.fn()
    const pending = boundedEnginePhase(new Promise<never>(() => undefined), 'exec', 500, terminate)
    const rejected = expect(pending).rejects.toEqual(expect.objectContaining<Partial<BrowserEngineTimeoutError>>({ phase: 'exec' }))
    await vi.advanceTimersByTimeAsync(500)
    await rejected
    expect(terminate).toHaveBeenCalledOnce()
    vi.useRealTimers()
  })

  it('does not terminate a completed phase', async () => {
    const terminate = vi.fn()
    await expect(boundedEnginePhase(Promise.resolve(7), 'read', 500, terminate)).resolves.toBe(7)
    expect(terminate).not.toHaveBeenCalled()
  })
})

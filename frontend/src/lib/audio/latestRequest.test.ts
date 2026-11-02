import { afterEach, describe, expect, it, vi } from 'vitest'
import { LatestRequest } from './latestRequest'
afterEach(() => vi.useRealTimers())
describe('latest native request queue', () => {
  it('does not starve during continuous changes and keeps only latest pending input', async () => {
    vi.useFakeTimers()
    const finished = vi.fn()
    let release: (value: number) => void = () => {}
    const run = vi.fn((input: number) => new Promise<number>(resolve => { release = resolve }))
    const queue = new LatestRequest(run, finished, vi.fn(), vi.fn())
    for (let i = 0; i < 6; i++) { queue.push(i); await vi.advanceTimersByTimeAsync(20) }
    expect(run.mock.calls.map(call => call[0])).toEqual([5])
    queue.push(6); queue.push(7); queue.push(8)
    await vi.advanceTimersByTimeAsync(240)
    expect(run).toHaveBeenCalledTimes(1)
    release(50); await vi.advanceTimersByTimeAsync(120)
    expect(finished).toHaveBeenCalledWith(50, 5)
    expect(run.mock.calls.map(call => call[0])).toEqual([5, 8])
    queue.cancel()
  })
  it('aborts closed work, ignores its late completion and permits reopening', async () => {
    vi.useFakeTimers()
    const finished = vi.fn()
    let release: (value: number) => void = () => {}
    const run = vi.fn((input: number, signal: AbortSignal) => new Promise<number>(resolve => { release = resolve }))
    const queue = new LatestRequest(run, finished, vi.fn(), vi.fn())
    queue.push(1); await vi.advanceTimersByTimeAsync(120)
    const oldRelease = release
    queue.cancel()
    expect(run.mock.calls[0]![1].aborted).toBe(true)
    queue.push(2); await vi.advanceTimersByTimeAsync(120)
    oldRelease(1); await vi.advanceTimersByTimeAsync(0)
    expect(finished).not.toHaveBeenCalled()
    release(2); await vi.advanceTimersByTimeAsync(0)
    expect(finished).toHaveBeenCalledExactlyOnceWith(2, 2)
    queue.cancel()
  })
})

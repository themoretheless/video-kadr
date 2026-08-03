import { describe, expect, it } from 'vitest'
import {
  IDB_MAX_BYTES, MEMFS_FALLBACK_MAX_INPUT_BYTES, MIB, OPFS_INGEST_OVERHEAD_BYTES,
  assertBoundedBrowserOutput, assertCompleteMediaDuration, browserMemoryBudget, isLikelyOutOfMemory, planBrowserExport, planBrowserImport,
} from './browser-resource-plan'

const video = { id: 'v', url: '', filename: '4k.mp4', duration: 600, width: 3840, height: 2160, fps: 30, sizeBytes: 1024 * MIB }
const capable = { worker: true, wasm: true, workerFs: true, opfs: true, webCrypto: true, deviceMemoryGiB: 8 }

describe('browser resource planning', () => {
  it('reserves two copies plus fixed overhead for staged OPFS publication', () => {
    const size = 100 * MIB
    expect(planBrowserImport(size, { usage: 0, quota: size * 2 }).risk).toBe('blocked')
    expect(planBrowserImport(size, { usage: 0, quota: size * 2 + OPFS_INGEST_OVERHEAD_BYTES }).risk).not.toBe('blocked')
  })

  it('documents the exact IDB and MEMFS fallback boundaries', () => {
    expect(IDB_MAX_BYTES).toBe(128 * MIB)
    expect(MEMFS_FALLBACK_MAX_INPUT_BYTES).toBe(128 * MIB)
    const fallback = { ...capable, workerFs: false }
    expect(planBrowserExport({ ...video, sizeBytes: MEMFS_FALLBACK_MAX_INPUT_BYTES }, {}, fallback).reason ?? '').not.toContain('WORKERFS')
    expect(planBrowserExport({ ...video, sizeBytes: MEMFS_FALLBACK_MAX_INPUT_BYTES + 1 }, {}, fallback).reason).toContain('WORKERFS')
  })

  it('blocks a ten-minute 4K reverse before starting FFmpeg', () => {
    const plan = planBrowserExport(video, { reverse: true, trim: { start: 0, end: 600 } }, capable)
    expect(plan.risk).toBe('blocked')
    expect(plan.estimatedPeakMemoryBytes).toBeGreaterThan(plan.memoryBudgetBytes)
  })

  it('allows a short worker-backed trim and rejects missing runtime support', () => {
    expect(planBrowserExport(video, { trim: { start: 0, end: 1 }, format: 'mp4' }, capable).risk).not.toBe('blocked')
    expect(planBrowserExport(video, {}, { ...capable, worker: false, workerFs: false }).reason).toContain('WebAssembly Worker')
  })

  it('classifies wasm allocation failures for runtime reset', () => {
    expect(isLikelyOutOfMemory(new WebAssembly.RuntimeError('memory access out of bounds'))).toBe(true)
    expect(isLikelyOutOfMemory(new Error('bad input'))).toBe(false)
  })

  it('blocks long audio reverse using conservative PCM buffering', () => {
    const audio = { id: 'a', url: '', filename: 'long.wav', duration: 3_600, width: 0, height: 0, mediaKind: 'audio' as const, sizeBytes: 20 * MIB }
    const plan = planBrowserExport(audio, { reverse: true, format: 'mp3' }, capable)
    expect(plan.risk).toBe('blocked')
    expect(plan.estimatedPeakMemoryBytes).toBeGreaterThan(plan.memoryBudgetBytes)
  })

  it('accounts for portrait padding and slow-motion output duration', () => {
    const hd = { ...video, duration: 20, width: 1920, height: 1080, sizeBytes: 20 * MIB }
    const plain = planBrowserExport(hd, { trim: { start: 0, end: 5 } }, capable)
    const expanded = planBrowserExport(hd, { trim: { start: 0, end: 5 }, pad: '9:16', speed: 0.5 }, capable)
    expect(expanded.estimatedPeakMemoryBytes).toBeGreaterThan(plain.estimatedPeakMemoryBytes)
  })

  it('rejects an output at the ffmpeg file-size guard instead of publishing a truncated result', () => {
    expect(() => assertBoundedBrowserOutput(98, 100)).not.toThrow()
    expect(() => assertBoundedBrowserOutput(99, 100)).toThrow('мог быть усечён')
  })

  it('rejects a parseable but shortened output container', () => {
    expect(() => assertCompleteMediaDuration(9.97, 10)).not.toThrow()
    expect(() => assertCompleteMediaDuration(9.96, 10)).toThrow('усечённый')
    expect(() => assertCompleteMediaDuration(Number.NaN, 10)).toThrow('усечённый')
  })

  it('compiles middle-cut duration from the same two-segment contract as FFmpeg', () => {
    const clip = { ...video, duration: 10, sizeBytes: 10 * MIB }
    const plan = planBrowserExport(clip, {
      trim: { start: 0, end: 10 }, speed: 2,
      segments: [{ start: 0, end: 3 }, { start: 7, end: 10 }],
    }, capable)
    expect(plan.selectedSeconds).toBe(6)
    expect(plan.estimatedOutputSeconds).toBe(3)
  })

  it('budgets the honest high size-v1 bound rather than the optimistic center', () => {
    const clip = { ...video, duration: 10, width: 1920, height: 1080, fps: 30, sizeBytes: 10 * MIB }
    const high = planBrowserExport(clip, { format: 'mp4', quality: 18, trim: { start: 0, end: 10 } }, capable)
    const compact = planBrowserExport(clip, { format: 'mp4', quality: 28, trim: { start: 0, end: 10 } }, capable)
    expect(high.estimatedOutputBytes).toBeGreaterThan(compact.estimatedOutputBytes)
  })

  it('uses a monotonic conservative budget across declared device-memory tiers', () => {
    const budgets = [0.25, 0.5, 1, 4, 8].map((deviceMemoryGiB) => browserMemoryBudget({ ...capable, deviceMemoryGiB }))
    expect(budgets).toEqual([...budgets].sort((left, right) => left - right))
    expect(browserMemoryBudget({ ...capable, deviceMemoryGiB: null })).toBe(384 * MIB)
    expect(budgets[0]).toBeLessThan(192 * MIB)
  })
})

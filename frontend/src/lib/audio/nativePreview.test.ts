import { expect, it, vi } from 'vitest'
import { NativeAudioPreview, type PreviewOutput } from './nativePreview'
import type { NativeNestedProject } from '../api'
const project = { schemaVersion: 1, root: 'editor', compositions: {} } as NativeNestedProject
const flush = async () => { for (let i = 0; i < 10; i++) await Promise.resolve() }
function setup() {
  const nodes: { onended: null | (() => void); start: ReturnType<typeof vi.fn>; stop: ReturnType<typeof vi.fn>; disconnect: ReturnType<typeof vi.fn> }[] = []
  const context = { currentTime: 0, state: 'running', resume: vi.fn().mockResolvedValue(undefined),
    createBuffer: vi.fn((_ch: number, frames: number) => ({ getChannelData: () => new Float32Array(frames) })),
    createBufferSource: () => { const node = { onended: null, start: vi.fn(), stop: vi.fn(), disconnect: vi.fn(), connect: vi.fn() }; nodes.push(node); return node },
  }
  const dispose = vi.fn()
  const pending: { start: number; frames: number; signal: AbortSignal; resolve: (v: Float32Array) => void; reject: (e: Error) => void }[] = []
  const fetch = vi.fn((_p, start, frames, _token, signal, _focus?: string) => new Promise<Float32Array>((resolve, reject) => pending.push({ start, frames, signal, resolve, reject })))
  const engine = new NativeAudioPreview(fetch, () => ({ context, input: {}, dispose }) as unknown as PreviewOutput)
  const resolve = async (i: number) => { pending[i].resolve(new Float32Array(pending[i].frames * 2)); await flush() }
  return { engine, context, nodes, pending, fetch, dispose, resolve }
}
it('queues only two blocks and advances from the audio clock', async () => {
  const t = setup(); t.engine.start(project, 0, 10e6); await flush()
  expect(t.pending).toHaveLength(1); expect(t.engine.ready).toBe(false)
  await t.resolve(0); expect(t.pending).toHaveLength(2)
  await t.resolve(1); expect(t.pending).toHaveLength(2)
  expect(t.nodes[0].start).toHaveBeenCalledWith(0.03)
  expect(t.nodes[1].start).toHaveBeenCalledWith(2.03)
  t.context.currentTime = 1.03
  expect(t.engine.ready).toBe(true); expect(t.engine.timeUs()).toBe(1e6)
  t.context.currentTime = 2.04; t.nodes[0].onended?.()
  expect(t.pending).toHaveLength(3); expect(t.pending[2].start).toBe(192000)
  t.engine.stop()
})
it('aborts seeks and never plays stale responses', async () => {
  const t = setup(); t.engine.start(project, 0, 10e6); await flush()
  t.engine.start(project, 1e6, 10e6); await flush()
  expect(t.pending[0].signal.aborted).toBe(true)
  await t.resolve(0); expect(t.nodes).toHaveLength(0)
  expect(t.pending[1].start).toBe(48000)
  await t.resolve(1); expect(t.nodes).toHaveLength(1)
  t.engine.stop(); expect(t.nodes[0].stop).toHaveBeenCalledOnce()
  expect(t.pending[2].signal.aborted).toBe(true)
  await t.resolve(2); expect(t.nodes).toHaveLength(1)
})
it('holds the playhead on an underrun then reanchors without skipping samples', async () => {
  const t = setup(); t.engine.start(project, 0, 10e6); await flush(); await t.resolve(0)
  t.context.currentTime = 4; t.nodes[0].onended?.()
  expect(t.engine.ready).toBe(false); expect(t.engine.timeUs()).toBe(2e6)
  await t.resolve(1)
  expect(t.nodes[1].start).toHaveBeenCalledWith(4.03)
  t.context.currentTime = 4.53
  expect(t.engine.timeUs()).toBe(2.5e6)
  t.engine.stop()
})
it('stops queued audio and preserves a request error', async () => {
  const t = setup(); t.engine.start(project, 0, 10e6); await flush(); await t.resolve(0)
  t.pending[1].reject(new Error('DSP unavailable')); await flush()
  expect(t.engine.error).toBe('DSP unavailable'); expect(t.engine.ready).toBe(false)
  expect(t.nodes[0].stop).toHaveBeenCalledOnce(); expect(t.dispose).toHaveBeenCalled()
})
it('finishes a fractional-duration tail without an endless transport loop', async () => {
  const t = setup(); t.engine.start(project, 0, 100001); await flush()
  expect(t.pending[0].frames).toBe(4800); await t.resolve(0)
  t.context.currentTime = 1; t.nodes[0].onended?.()
  expect(t.engine.finished).toBe(true); t.engine.stop()
})

it('sends the explicit focused timeline without replacing the project root', async () => {
  const t = setup(); t.engine.start(project, 0, 1000000, 'session', 'child'); await flush()
  expect(t.fetch.mock.calls[0][0]).toBe(project)
  expect(t.fetch.mock.calls[0][5]).toBe('child')
  t.engine.stop()
})

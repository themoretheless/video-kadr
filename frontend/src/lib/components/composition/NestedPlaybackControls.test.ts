// @ts-expect-error Tests need Svelte's client mount under Vitest SSR resolution.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { nestedDurations, type NativeNestedProject } from '$lib/api'
import { NativeAudioPreview } from '$lib/audio/nativePreview'
import { closeNestedComposition, nestedComposition, nestedPlayback, navigateNestedComposition } from '$lib/state/nestedComposition.svelte'
import { startSubtitleDrag } from '$lib/subtitles/timelineDrop'
import Controls from './NestedPlaybackControls.svelte'
// Match the component's browser untrack implementation under Vitest SSR conditions.
vi.mock('svelte', async importOriginal => {
  const original = await importOriginal<typeof import('svelte')>()
  // @ts-expect-error Svelte's client source entry has no declaration file.
  const client = await import('../../../../node_modules/svelte/src/index-client.js')
  return { ...original, untrack: client.untrack }
})
vi.mock('$lib/api', async importOriginal => ({ ...await importOriginal<typeof import('$lib/api')>(), nestedDurations: vi.fn() }))
let component: ReturnType<typeof mount> | undefined
let callback: FrameRequestCallback | undefined
let audioTime = 250000
let engine: NativeAudioPreview | undefined
beforeEach(() => {
  closeNestedComposition(); audioTime = 250000; engine = undefined
  nestedComposition.document = { schemaVersion: 1, root: 'root', compositions: { root: { content: {}, instances: [] }, child: { content: {}, instances: [] } } } as unknown as NativeNestedProject
  nestedComposition.activeCompositionId = 'root'
  vi.mocked(nestedDurations).mockResolvedValue({ root: 3000000, child: 1000000 })
  vi.spyOn(NativeAudioPreview.prototype, 'start').mockImplementation(function (this: NativeAudioPreview) { engine = this })
  vi.spyOn(NativeAudioPreview.prototype, 'stop').mockImplementation(() => undefined)
  vi.spyOn(NativeAudioPreview.prototype, 'ready', 'get').mockReturnValue(true)
  vi.spyOn(NativeAudioPreview.prototype, 'timeUs').mockImplementation(() => audioTime)
  vi.stubGlobal('requestAnimationFrame', (cb: FrameRequestCallback) => { callback = cb; return 1 })
  vi.stubGlobal('cancelAnimationFrame', vi.fn())
})
afterEach(async () => { if (component) await unmount(component); component = undefined; closeNestedComposition(); vi.restoreAllMocks(); vi.unstubAllGlobals(); vi.clearAllMocks(); document.body.innerHTML = '' })
async function setup() {
  component = mount(Controls, { target: document.body, props: { token: 'session' } })
  await tick(); await tick()
  await vi.waitFor(() => expect(document.querySelector('button')?.disabled).toBe(false))
}
async function play() { document.querySelector('button')!.click(); await tick() }
it('uses the selected composition clock, invalidates seeks and pauses on navigation', async () => {
  await setup(); await play()
  expect(NativeAudioPreview.prototype.start).toHaveBeenLastCalledWith(nestedComposition.document, 0, 3000000, 'session', 'root')
  callback?.(0); await tick()
  expect(nestedComposition.timeUs).toBe(250000)
  expect(NativeAudioPreview.prototype.start).toHaveBeenCalledTimes(1)
  nestedComposition.timeUs = 750000; await tick()
  expect(NativeAudioPreview.prototype.start).toHaveBeenLastCalledWith(nestedComposition.document, 750000, 3000000, 'session', 'root')
  const epoch = nestedPlayback.epoch
  navigateNestedComposition('child'); await tick(); await tick()
  expect(nestedPlayback.playing).toBe(false); expect(nestedPlayback.epoch).toBeGreaterThan(epoch)
  await vi.waitFor(() => expect(document.querySelector('button')?.disabled).toBe(false))
  await play()
  expect(NativeAudioPreview.prototype.start).toHaveBeenLastCalledWith(nestedComposition.document, 0, 1000000, 'session', 'child')
})
it('keeps an error visible, pauses on native failure, and completes the authored duration', async () => {
  await setup(); await play()
  engine!.error = 'private source unavailable'; callback?.(0); await tick()
  expect(nestedPlayback.playing).toBe(false)
  expect(document.querySelector('[role="alert"]')?.textContent).toContain('private source unavailable')
  engine!.error = ''; await play()
  vi.spyOn(NativeAudioPreview.prototype, 'finished', 'get').mockReturnValue(true)
  callback?.(0); await tick()
  expect(nestedComposition.timeUs).toBe(3000000); expect(nestedPlayback.playing).toBe(false)
  await play()
  expect(NativeAudioPreview.prototype.start).toHaveBeenLastCalledWith(nestedComposition.document, 0, 3000000, 'session', 'root')
})
it('aborts stale duration requests and refuses invalid server durations', async () => {
  let resolve!: (v: Record<string, number>) => void
  vi.mocked(nestedDurations).mockImplementationOnce(() => new Promise(done => { resolve = done }))
  component = mount(Controls, { target: document.body, props: { token: 'session' } }); await tick()
  const signal = vi.mocked(nestedDurations).mock.calls[0][1]!
  navigateNestedComposition('child'); await tick(); await tick()
  expect(signal.aborted).toBe(true)
  resolve({ root: 9000000 }); await tick()
  expect(document.body.textContent).toContain('1.00 с')
  vi.mocked(nestedDurations).mockResolvedValue({ child: -1 })
  nestedComposition.document = { ...nestedComposition.document! }; await tick(); await tick()
  await vi.waitFor(() => expect(document.querySelector('[role="alert"]')?.textContent).toContain('Некорректная длительность'))
  expect(document.querySelector('button')?.disabled).toBe(true)
})

it('keeps pause available while an edited project awaits its new duration', async () => {
  await setup(); await play()
  vi.mocked(nestedDurations).mockImplementationOnce(() => new Promise(() => undefined))
  nestedComposition.document = { ...nestedComposition.document! }; await tick()
  expect(nestedPlayback.playing).toBe(true)
  expect(document.querySelector('button')?.disabled).toBe(false)
  expect(document.querySelector('button')?.textContent).toContain('Пауза')
  document.querySelector('button')!.click(); await tick()
  expect(nestedPlayback.playing).toBe(false)
  expect(NativeAudioPreview.prototype.stop).toHaveBeenCalled()
})

it('places a dragged subtitle at the fraction of the focused composition duration', async () => {
  await setup()
  navigateNestedComposition('child')
  await tick()
  const input = document.querySelector<HTMLInputElement>('input[type="range"]')!
  await vi.waitFor(() => expect(input.max).toBe('1000000'))
  vi.spyOn(input, 'getBoundingClientRect').mockReturnValue(new DOMRect(200, 0, 400, 20))
  const insert = vi.fn()
  const transfer = new DataTransfer()
  const event = (type: string) => {
    const drag = new MouseEvent(type, { bubbles: true, cancelable: true, clientX: 300 }) as DragEvent
    Object.defineProperty(drag, 'dataTransfer', { value: transfer })
    return drag
  }
  const endDrag = startSubtitleDrag(event('dragstart'), insert)
  try {
    expect(input.dispatchEvent(event('dragover'))).toBe(false)
    const ruler = input.parentElement!
    expect(ruler.hasAttribute('data-subtitle-drop-preview')).toBe(true)
    expect(ruler.style.getPropertyValue('--subtitle-drop-position')).toBe('25%')
    endDrag()
    expect(ruler.hasAttribute('data-subtitle-drop-preview')).toBe(false)
    expect(insert).not.toHaveBeenCalled()
    const finish = startSubtitleDrag(event('dragstart'), insert)
    try {
      input.dispatchEvent(event('dragover'))
      expect(input.dispatchEvent(event('drop'))).toBe(false)
      expect(ruler.hasAttribute('data-subtitle-drop-preview')).toBe(false)
    } finally {
      finish()
    }
    expect(insert).toHaveBeenCalledExactlyOnceWith(250000)
    expect(nestedComposition.timeUs).toBe(0)
  } finally {
    endDrag()
  }
})

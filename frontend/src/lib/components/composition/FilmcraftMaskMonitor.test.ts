// @ts-expect-error Tests require the Svelte client entry under Vitest SSR resolution.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, expect, it, vi } from 'vitest'
import { editFilmcraftMaskMonitor, getFilmcraftPreviewFrame, type FilmcraftMaskOverlay, type FilmcraftImportResult } from '$lib/api'
import Monitor from './FilmcraftMaskMonitor.svelte'
// Vitest resolves the public Svelte entry to SSR; interaction effects need
// the client untrack implementation used by the actual browser component.
vi.mock('svelte', async importOriginal => {
  const actual = await importOriginal<typeof import('svelte')>()
  // @ts-expect-error The explicit client entry has no public declaration file.
  const client = await import('../../../../node_modules/svelte/src/index-client.js')
  return { ...actual, untrack: client.untrack }
})
vi.mock('$lib/api', () => ({ editFilmcraftMaskMonitor: vi.fn(), getFilmcraftPreviewFrame: vi.fn() }))
const address = { sequenceId: '18446744073709551615', clipId: '18446744073709551614', effectIndex: 0, effectKind: 'opacity', maskIndex: 0, parameter: 'path' as const }
const overlay: FilmcraftMaskOverlay = { canvas: [100, 100], matrix: [1, 0, 0, 1, 0, 0], sampleTicks: '9007199254740993', locked: false, visible: true, editable: true, closed: true, svgPath: 'M 10 10 L 70 10 L 70 70 L 10 70 Z', vertices: [[10, 10], [70, 10], [70, 70], [10, 70]].map((p, index) => ({ index, point: p as [number, number], incoming: p as [number, number], outgoing: p as [number, number] })) }
const changed: FilmcraftImportResult = { documentText: 'native changed document', originalSchema: 12, sequences: [], media: [] }
let component: ReturnType<typeof mount> | undefined
let box = { left: 0, top: 0, width: 200, height: 200 }
const svg = () => document.querySelector<SVGSVGElement>('svg')!
const handle = () => document.querySelector<SVGCircleElement>('[aria-label="Маска 1 vertex Filmcraft"]')!
function pointer(target: Element, name: string, x: number, y: number, extra = {}) { target.dispatchEvent(new PointerEvent(name, { bubbles: true, pointerId: 1, button: 0, clientX: x, clientY: y, ...extra })) }
afterEach(async () => { if (component) await unmount(component); component = undefined; vi.restoreAllMocks(); vi.resetAllMocks(); document.body.innerHTML = ''; box = { left: 0, top: 0, width: 200, height: 200 } })
async function open(value = overlay, createMask = false) {
  vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue({ createImageData: (w: number, h: number) => ({ data: new Uint8ClampedArray(w * h * 4) }), putImageData: vi.fn() } as unknown as CanvasRenderingContext2D)
  vi.mocked(getFilmcraftPreviewFrame).mockResolvedValue({ kind: 'sequence', sequenceId: address.sequenceId, scale: 1, effects: true, captions: true, width: 1, height: 1, rgbaHex: '000000ff', ptsTicks: '0', sampleTicks: '0' })
  vi.mocked(editFilmcraftMaskMonitor).mockImplementation(async input => ({ overlay: value, project: input.operation.kind === 'read' ? null : changed }))
  const onedit = vi.fn(), onbusy = vi.fn(), ondeselect = vi.fn()
  component = mount(Monitor, { target: document.body, props: { document: 'opaque original', createMask, address: createMask ? { ...address, maskIndex: undefined } : address, timeFrames: '0', timeTicks: '9007199254740993', sources: { '1': 'asset' }, token: 'owner', onedit, onbusy, ondeselect } })
  await tick(); expect(editFilmcraftMaskMonitor).not.toHaveBeenCalled()
  const details = document.querySelector('details')!; details.open = true; details.dispatchEvent(new Event('toggle')); await tick()
  await vi.waitFor(() => expect(svg()).toBeTruthy())
  Object.defineProperty(svg(), 'getBoundingClientRect', { value: () => box })
  const captured = new Set<number>()
  Object.defineProperty(svg(), 'setPointerCapture', { value: (id: number) => captured.add(id) })
  Object.defineProperty(svg(), 'hasPointerCapture', { value: (id: number) => captured.has(id) })
  Object.defineProperty(svg(), 'releasePointerCapture', { value: (id: number) => { captured.delete(id); svg().dispatchEvent(new PointerEvent('lostpointercapture', { pointerId: id })) } })
  return { onedit, onbusy, ondeselect }
}
it('previews a native drag then commits one document on release with exact source time', async () => {
  const { onedit } = await open()
  pointer(handle(), 'pointerdown', 20, 20); pointer(svg(), 'pointermove', 40, 40); await tick()
  await vi.waitFor(() => expect(editFilmcraftMaskMonitor).toHaveBeenCalledTimes(2))
  expect(onedit).not.toHaveBeenCalled()
  pointer(svg(), 'pointerup', 40, 40)
  await vi.waitFor(() => expect(onedit).toHaveBeenCalledExactlyOnceWith(changed))
  expect(vi.mocked(editFilmcraftMaskMonitor).mock.calls.at(-1)![0]).toEqual({ document: 'opaque original', address, timeTicks: '9007199254740993', operation: { kind: 'move', index: 0, part: 'vertex', point: [20, 20], breakHandles: false } })
})
it('cancels a preview draft on Escape without adding history', async () => {
  const { onedit } = await open(); pointer(handle(), 'pointerdown', 20, 20); pointer(svg(), 'pointermove', 40, 40); await tick()
  await vi.waitFor(() => expect(editFilmcraftMaskMonitor).toHaveBeenCalledTimes(2))
  svg().dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true })); await tick(); pointer(svg(), 'pointerup', 40, 40); await tick()
  expect(onedit).not.toHaveBeenCalled()
})
it('rejects a drag when viewport geometry changes', async () => {
  const { onedit } = await open(); pointer(handle(), 'pointerdown', 20, 20); box.width = 300; pointer(svg(), 'pointerup', 40, 40); await tick()
  expect(onedit).not.toHaveBeenCalled()
  expect(editFilmcraftMaskMonitor).toHaveBeenCalledTimes(1)
})
it('keeps the grab offset and submits keyboard nudge and topology commands natively', async () => {
  const { onedit } = await open(); pointer(handle(), 'pointerdown', 22, 22); pointer(svg(), 'pointerup', 42, 42)
  await vi.waitFor(() => expect(onedit).toHaveBeenCalledTimes(1))
  expect(vi.mocked(editFilmcraftMaskMonitor).mock.calls.at(-1)![0].operation).toMatchObject({ kind: 'move', point: [20, 20] })
  svg().dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', shiftKey: true, bubbles: true }))
  await vi.waitFor(() => expect(onedit).toHaveBeenCalledTimes(2))
  expect(vi.mocked(editFilmcraftMaskMonitor).mock.calls.at(-1)![0].operation).toMatchObject({ kind: 'move', point: [20, 10] })
  svg().dispatchEvent(new KeyboardEvent('keydown', { key: 'Delete', bubbles: true }))
  await vi.waitFor(() => expect(onedit).toHaveBeenCalledTimes(3))
  expect(vi.mocked(editFilmcraftMaskMonitor).mock.calls.at(-1)![0].operation).toEqual({ kind: 'delete', index: 0 })
})
it.each(['rectangle', 'ellipse'])('draws %s through native source geometry', async tool => {
  const { onedit } = await open(); const select = document.querySelector<HTMLSelectElement>('[aria-label="Инструмент маски в мониторе Filmcraft"]')!
  select.value = tool; select.dispatchEvent(new Event('change', { bubbles: true })); await tick(); pointer(svg(), 'pointerdown', 40, 40)
  pointer(svg(), 'pointerup', 120, 140)
  await vi.waitFor(() => expect(onedit).toHaveBeenCalledExactlyOnceWith(changed))
  expect(vi.mocked(editFilmcraftMaskMonitor).mock.calls.at(-1)![0].operation).toEqual({ kind: 'box', shape: tool, from: [20, 20], to: [60, 70] })
})
it('refuses locked overlays and never commits a late response after closure', async () => {
  const { onedit } = await open(); let resolve!: (value: { overlay: FilmcraftMaskOverlay; project: FilmcraftImportResult }) => void
  vi.mocked(editFilmcraftMaskMonitor).mockImplementation(() => new Promise(r => { resolve = r }))
  pointer(handle(), 'pointerdown', 20, 20); pointer(svg(), 'pointerup', 40, 40)
  await vi.waitFor(() => expect(editFilmcraftMaskMonitor).toHaveBeenCalledTimes(2))
  const signal = vi.mocked(editFilmcraftMaskMonitor).mock.calls[1]![1]!
  const details = document.querySelector('details')!; details.open = false; details.dispatchEvent(new Event('toggle')); await tick()
  resolve({ overlay, project: changed }); await tick(); await tick()
  expect(signal.aborted).toBe(true); expect(onedit).not.toHaveBeenCalled()
})
it('disables mutation for a noneditable overlay', async () => {
  const { onedit } = await open({ ...overlay, editable: false, locked: true })
  pointer(handle(), 'pointerdown', 20, 20); pointer(svg(), 'pointerup', 40, 40); await tick()
  expect(editFilmcraftMaskMonitor).toHaveBeenCalledTimes(1); expect(onedit).not.toHaveBeenCalled()
})

it('keeps pen clicks and tangent drags local until Enter commits the complete contour once', async () => {
  const { onedit } = await open()
  const tool = document.querySelector<HTMLSelectElement>('[aria-label="Инструмент маски в мониторе Filmcraft"]')!
  tool.value = 'pen'; tool.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  pointer(svg(), 'pointerdown', 40, 40); pointer(svg(), 'pointermove', 60, 40); pointer(svg(), 'pointerup', 60, 40); await tick()
  pointer(svg(), 'pointerdown', 120, 40); pointer(svg(), 'pointerup', 120, 40); await tick()
  pointer(svg(), 'pointerdown', 80, 140); pointer(svg(), 'pointermove', 60, 140); pointer(svg(), 'pointerup', 60, 140); await tick()
  expect(onedit).not.toHaveBeenCalled()
  svg().dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
  await vi.waitFor(() => expect(onedit).toHaveBeenCalledExactlyOnceWith(changed))
  expect(vi.mocked(editFilmcraftMaskMonitor).mock.calls.at(-1)![0].operation).toEqual({ kind: 'penDraft', closed: true, points: [{ point: [20, 20], handle: [30, 20] }, { point: [60, 20] }, { point: [40, 70], handle: [30, 70] }] })
  expect(tool.value).toBe('move')
})
it('discards the entire unfinished pen draft on Escape', async () => {
  const { onedit, onbusy } = await open()
  const tool = document.querySelector<HTMLSelectElement>('[aria-label="Инструмент маски в мониторе Filmcraft"]')!
  tool.value = 'pen'; tool.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  pointer(svg(), 'pointerdown', 40, 40); pointer(svg(), 'pointerup', 40, 40); await tick()
  pointer(svg(), 'pointerdown', 120, 40); pointer(svg(), 'pointerup', 120, 40); await tick()
  svg().dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true })); await tick()
  expect(onedit).not.toHaveBeenCalled(); expect(onbusy).toHaveBeenLastCalledWith(false)
  expect(tool.value).toBe('move')
})
it('uses native screen-space segment insertion for the insert tool and Ctrl-click', async () => {
  const { onedit } = await open()
  pointer(svg(), 'pointerdown', 80, 20, { ctrlKey: true }); await tick()
  await vi.waitFor(() => expect(onedit).toHaveBeenCalledTimes(1))
  expect(vi.mocked(editFilmcraftMaskMonitor).mock.calls.at(-1)![0].operation).toEqual({ kind: 'insertNear', point: [40, 10], pixelScale: [2, 2] })
  const tool = document.querySelector<HTMLSelectElement>('[aria-label="Инструмент маски в мониторе Filmcraft"]')!
  tool.value = 'insert'; tool.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  pointer(svg(), 'pointerdown', 80, 20)
  await vi.waitFor(() => expect(onedit).toHaveBeenCalledTimes(2))
  expect(vi.mocked(editFilmcraftMaskMonitor).mock.calls.at(-1)![0].operation).toEqual({ kind: 'insertNear', point: [40, 10], pixelScale: [2, 2] })
})
it('settles a native insertion miss without publishing or keeping controls busy', async () => {
  const { onedit, onbusy } = await open(); vi.mocked(editFilmcraftMaskMonitor).mockResolvedValue({ overlay, project: null })
  pointer(svg(), 'pointerdown', 180, 180, { metaKey: true }); await tick()
  await vi.waitFor(() => expect(editFilmcraftMaskMonitor).toHaveBeenCalledTimes(2))
  await vi.waitFor(() => expect(onbusy).toHaveBeenLastCalledWith(false))
  expect(onedit).not.toHaveBeenCalled()
})
it('supports original Alt-click smooth and Ctrl-click delete on a vertex', async () => {
  const { onedit } = await open(); pointer(handle(), 'pointerdown', 20, 20, { altKey: true }); pointer(svg(), 'pointerup', 20, 20, { altKey: true })
  await vi.waitFor(() => expect(onedit).toHaveBeenCalledTimes(1))
  expect(vi.mocked(editFilmcraftMaskMonitor).mock.calls.at(-1)![0].operation).toEqual({ kind: 'smooth', index: 0 })
  pointer(handle(), 'pointerdown', 20, 20, { ctrlKey: true })
  await vi.waitFor(() => expect(onedit).toHaveBeenCalledTimes(2))
  expect(vi.mocked(editFilmcraftMaskMonitor).mock.calls.at(-1)![0].operation).toEqual({ kind: 'delete', index: 0 })
})

it('creates a new mask only after closing the complete pen and returns its native index', async () => {
  const { onedit } = await open({ ...overlay, vertices: [], svgPath: '', closed: false, createdMaskIndex: 2 }, true)
  const tool = document.querySelector<HTMLSelectElement>('[aria-label="Инструмент маски в мониторе Filmcraft"]')!
  expect(tool.value).toBe('pen')
  expect(vi.mocked(editFilmcraftMaskMonitor).mock.calls[0][0].createMask).toBe(true)
  expect(vi.mocked(editFilmcraftMaskMonitor).mock.calls[0][0].address.maskIndex).toBeUndefined()
  for (const [x, y] of [[40, 40], [120, 40], [80, 140]]) {
    pointer(svg(), 'pointerdown', x, y); pointer(svg(), 'pointerup', x, y); await tick()
  }
  expect(onedit).not.toHaveBeenCalled()
  svg().dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
  await vi.waitFor(() => expect(onedit).toHaveBeenCalledExactlyOnceWith(changed, 2))
  expect(tool.value).toBe('move')
})
it('cancels new mask creation without publishing an empty mask', async () => {
  const { onedit, onbusy } = await open({ ...overlay, vertices: [], svgPath: '', closed: false, createdMaskIndex: 0 }, true)
  pointer(svg(), 'pointerdown', 40, 40); pointer(svg(), 'pointerup', 40, 40); await tick()
  svg().dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true })); await tick()
  expect(onedit).not.toHaveBeenCalled()
  expect(onbusy).toHaveBeenLastCalledWith(false)
  expect(document.querySelector<HTMLSelectElement>('[aria-label="Инструмент маски в мониторе Filmcraft"]')!.value).toBe('pen')
})

it.each(['feather', 'expansion'] as const)('submits native %s drag deltas and commits once on release', async parameter => {
  const { onedit } = await open({ ...overlay, falloff: { feather: 10, expansion: 0, expansionContour: [], featherContour: [[10, 5], [70, 5]], handles: [{ parameter, point: [20, 10], direction: [0, -1] }] } })
  const control = document.querySelector(`[aria-label="${parameter === 'feather' ? 'Растушёвка' : 'Расширение'} маски Filmcraft"]`)!
  pointer(control, 'pointerdown', 40, 20); pointer(svg(), 'pointermove', 60, -20); await tick()
  expect(onedit).not.toHaveBeenCalled()
  expect(vi.mocked(editFilmcraftMaskMonitor).mock.calls.at(-1)![0].operation).toEqual({ kind: 'falloff', parameter, delta: [10, -20] })
  pointer(svg(), 'pointerup', 60, -20)
  await vi.waitFor(() => expect(onedit).toHaveBeenCalledExactlyOnceWith(changed))
})
it('discards a falloff draft on Escape and ignores its late result', async () => {
  const { onedit, onbusy } = await open({ ...overlay, falloff: { feather: 10, expansion: 0, expansionContour: [], featherContour: [], handles: [{ parameter: 'feather', point: [20, 10], direction: [0, -1] }] } })
  let resolve!: (value: { overlay: FilmcraftMaskOverlay; project: FilmcraftImportResult }) => void
  vi.mocked(editFilmcraftMaskMonitor).mockImplementation(() => new Promise(done => resolve = done))
  pointer(document.querySelector('[aria-label="Растушёвка маски Filmcraft"]')!, 'pointerdown', 40, 20)
  pointer(svg(), 'pointermove', 60, -20); await tick()
  svg().dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true })); await tick()
  resolve({ overlay, project: changed }); await tick()
  expect(onedit).not.toHaveBeenCalled(); expect(onbusy).toHaveBeenLastCalledWith(false)
})

it('passes the initial body press to native fill testing and accepts one inside drag', async () => {
  const { onedit } = await open()
  pointer(svg(), 'pointerdown', 100, 100); pointer(svg(), 'pointermove', 120, 140); await tick()
  expect(onedit).not.toHaveBeenCalled()
  expect(vi.mocked(editFilmcraftMaskMonitor).mock.calls.at(-1)![0].operation).toEqual({ kind: 'translate', origin: [50, 50], delta: [10, 20] })
  pointer(svg(), 'pointerup', 120, 140)
  await vi.waitFor(() => expect(onedit).toHaveBeenCalledExactlyOnceWith(changed))
})
it('keeps history unchanged when native body hit testing rejects the initial press', async () => {
  const { onedit, onbusy } = await open()
  vi.mocked(editFilmcraftMaskMonitor).mockResolvedValue({ overlay, project: null })
  pointer(svg(), 'pointerdown', 190, 190); pointer(svg(), 'pointermove', 100, 100); await tick()
  pointer(svg(), 'pointerup', 100, 100)
  await vi.waitFor(() => expect(onbusy).toHaveBeenLastCalledWith(false))
  expect(vi.mocked(editFilmcraftMaskMonitor).mock.calls.at(-1)![0].operation).toEqual({ kind: 'translate', origin: [95, 95], delta: [-45, -45] })
  expect(onedit).not.toHaveBeenCalled()
})

it.each([false, true])('applies native click selection clear=%s without publishing a source document', async clear => {
  const { onedit, ondeselect } = await open()
  vi.mocked(editFilmcraftMaskMonitor).mockResolvedValue({ overlay: { ...overlay, deselectMask: clear }, project: null })
  pointer(svg(), 'pointerdown', 20, 100); pointer(svg(), 'pointerup', 20, 100)
  await vi.waitFor(() => expect(editFilmcraftMaskMonitor).toHaveBeenLastCalledWith(expect.objectContaining({ operation: { kind: 'bodyClick', point: [10, 50], pixelScale: [2, 2] } }), expect.any(AbortSignal)))
  await vi.waitFor(() => expect(document.querySelector('details')!.open).toBe(!clear))
  expect(onedit).not.toHaveBeenCalled()
  expect(ondeselect).toHaveBeenCalledTimes(Number(clear))
})

it('ignores a late selection-clear result after the monitor closes', async () => {
  const { onedit, ondeselect } = await open()
  let resolve!: (value: { overlay: FilmcraftMaskOverlay; project: null }) => void
  vi.mocked(editFilmcraftMaskMonitor).mockImplementation(() => new Promise(done => resolve = done))
  pointer(svg(), 'pointerdown', 20, 100); pointer(svg(), 'pointerup', 20, 100); await tick()
  const details = document.querySelector('details')!; details.open = false; details.dispatchEvent(new Event('toggle')); await tick()
  resolve({ overlay: { ...overlay, deselectMask: true }, project: null }); await tick()
  expect(ondeselect).not.toHaveBeenCalled(); expect(onedit).not.toHaveBeenCalled()
})

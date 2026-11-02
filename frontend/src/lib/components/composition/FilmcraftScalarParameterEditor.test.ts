// @ts-expect-error Tests require the Svelte client entry under Vitest SSR resolution.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, expect, it, vi } from 'vitest'
import { editFilmcraftAnimation, setFilmcraftEffectParameter, applyFilmcraftEffect, listFilmcraftEffects, editFilmcraftEffectRack, getFilmcraftParameters, editFilmcraftParameter, editFilmcraftMask, type FilmcraftParameterReport, type FilmcraftImportResult } from '$lib/api'
import Editor from './FilmcraftScalarParameterEditor.svelte'
import ContextHarness from './fixtures/FilmcraftParameterContextHarness.svelte'
import { writable } from 'svelte/store'
vi.mock('$lib/api', () => ({ startFilmcraftMaskTrackingJob: vi.fn(), readFilmcraftTrackingJob: vi.fn(), cancelFilmcraftTrackingJob: vi.fn(), importFilmcraftTrackingSnapshot: vi.fn(), editFilmcraftAnimation: vi.fn(), setFilmcraftEffectParameter: vi.fn(), applyFilmcraftEffect: vi.fn(), listFilmcraftEffects: vi.fn(), editFilmcraftEffectRack: vi.fn(), editFilmcraftMaskMonitor: vi.fn(), trackFilmcraftMask: vi.fn(), editFilmcraftMask: vi.fn(), getFilmcraftParameters: vi.fn(), editFilmcraftParameter: vi.fn() }))
let component: ReturnType<typeof mount> | undefined
const time = '9007199254740993'
const result: FilmcraftImportResult = { documentText: 'changed exact source', originalSchema: 12, sequences: [], media: [] }
const report: FilmcraftParameterReport = { locked: false, timelineTicks: '101', parameterTicks: time, frameTicks: time, generatorTicks: '1', effects: [{ index: 2, kind: 'gamma_correction', enabled: true, parameters: [{ name: 'gamma', value: { Float: 1 }, sampled: 1.5, keys: [{ time, value: { Float: 1.5 }, interp: 'Bezier' }] }] }] }
afterEach(async () => { if (component) await unmount(component); component = undefined; vi.resetAllMocks(); document.body.innerHTML = '' })
const button = (text: string) => [...document.querySelectorAll('button')].find(b => b.textContent === text)!
async function input(label: string, value: string) { const node = document.querySelector<HTMLInputElement>(`[aria-label="${label}"]`)!; node.value = value; node.dispatchEvent(new Event('input', { bubbles: true })); await tick() }
async function open(value = report, token?: string | null) {
  vi.mocked(getFilmcraftParameters).mockResolvedValue(value)
  vi.mocked(editFilmcraftParameter).mockResolvedValue(result)
  const onedit = vi.fn()
  component = mount(Editor, { target: document.body, props: { document: 'opaque u64 source', sequenceId: '18446744073709551615', clipId: '18446744073709551614', timeFrames: '101', token, onedit } })
  await tick()
  expect(getFilmcraftParameters).not.toHaveBeenCalled()
  const details = document.querySelector('details')!; details.open = true; details.dispatchEvent(new Event('toggle')); await tick()
  await vi.waitFor(() => expect(document.body.textContent).toContain('Время параметров:'))
  return onedit
}
it('keeps exact source ticks and effect identity across key editing', async () => {
  const onedit = await open()
  await input('Точное время ключа Filmcraft', time)
  await input('Значение параметра Filmcraft', '1.75')
  button('Сохранить ключ Filmcraft').click()
  await vi.waitFor(() => expect(onedit).toHaveBeenCalledWith(result))
  expect(editFilmcraftParameter).toHaveBeenLastCalledWith({ document: 'opaque u64 source', address: { sequenceId: '18446744073709551615', clipId: '18446744073709551614', effectIndex: 2, effectKind: 'gamma_correction', parameter: 'gamma' }, command: { kind: 'upsert', time, value: { Float: 1.75 } }, timeFrames: '101' }, expect.any(AbortSignal))
  await input('Новое время ключа Filmcraft', '9007199254740994')
  button('Переместить ключ Filmcraft').click()
  await vi.waitFor(() => expect(editFilmcraftParameter).toHaveBeenCalledTimes(2))
  expect(vi.mocked(editFilmcraftParameter).mock.calls[1]![0].command).toEqual({ kind: 'move', time, destination: '9007199254740994' })
})
it('delegates playhead clock calculation to Rust and rejects i64 overflow locally', async () => {
  await open()
  button('Ключ Filmcraft в позиции курсора').click()
  await vi.waitFor(() => expect(editFilmcraftParameter).toHaveBeenCalledTimes(1))
  expect(vi.mocked(editFilmcraftParameter).mock.calls[0]![0].command).toEqual({ kind: 'atPlayhead', value: { Float: 1 } })
  await input('Точное время ключа Filmcraft', '9223372036854775808')
  button('Удалить ключ Filmcraft').click(); await tick()
  expect(document.querySelector('[role=alert]')?.textContent).toContain('signed i64')
  expect(editFilmcraftParameter).toHaveBeenCalledTimes(1)
})
it('never publishes an edit after the editor is closed', async () => {
  const onedit = await open()
  let finish!: (value: FilmcraftImportResult) => void
  vi.mocked(editFilmcraftParameter).mockImplementation(() => new Promise(resolve => { finish = resolve }))
  button('Ключ Filmcraft в позиции курсора').click(); await tick()
  const details = document.querySelector('details')!; details.open = false; details.dispatchEvent(new Event('toggle')); await tick()
  finish(result); await tick(); await tick()
  expect(onedit).not.toHaveBeenCalled()
  expect(vi.mocked(editFilmcraftParameter).mock.calls[0]![1]?.aborted).toBe(true)
})
it('disables locked source tracks', async () => {
  await open({ ...report, locked: true })
  expect(document.querySelector('fieldset')?.disabled).toBe(true)
  button('Сохранить ключ Filmcraft').click(); await tick()
  expect(editFilmcraftParameter).not.toHaveBeenCalled()
})

it.each([{ Bool: true }, { Choice: 7 }])('retains scalar tags %j', async scalar => {
  const item = structuredClone(report)
  item.effects[0]!.parameters[0]!.value = scalar
  item.effects[0]!.parameters[0]!.keys = []
  await open(item)
  const text = 'Bool' in scalar ? 'false' : '9'
  await input('Значение параметра Filmcraft', text)
  button('Ключ Filmcraft в позиции курсора').click()
  await vi.waitFor(() => expect(editFilmcraftParameter).toHaveBeenCalledTimes(1))
  expect(vi.mocked(editFilmcraftParameter).mock.calls[0]![0].command).toEqual({ kind: 'atPlayhead', value: 'Bool' in scalar ? { Bool: false } : { Choice: 9 } })
})
it('aborts an in-flight mutation on unmount', async () => {
  const onedit = await open()
  let finish!: (value: FilmcraftImportResult) => void
  vi.mocked(editFilmcraftParameter).mockImplementation(() => new Promise(resolve => { finish = resolve }))
  button('Ключ Filmcraft в позиции курсора').click(); await tick()
  const signal = vi.mocked(editFilmcraftParameter).mock.calls[0]![1]!
  await unmount(component!); component = undefined
  finish(result); await tick(); await tick()
  expect(signal.aborted).toBe(true)
  expect(onedit).not.toHaveBeenCalled()
})
it('edits original Vec2 coordinates including auto without lowering exact source ticks', async () => {
  const item = structuredClone(report)
  item.effects[0]!.parameters[0]!.value = { Vec2: { x: null, y: 0.25 } }
  item.effects[0]!.parameters[0]!.keys = [{ time, value: { Vec2: { x: 0.125, y: -0.5 } }, interp: 'Bezier' }]
  await open(item)
  expect(document.querySelector<HTMLInputElement>('[aria-label="Компонент x Filmcraft"]')?.value).toBe('auto')
  await input('Компонент y Filmcraft', '0.123456789012345')
  await input('Точное время ключа Filmcraft', time)
  button('Сохранить ключ Filmcraft').click()
  await vi.waitFor(() => expect(editFilmcraftParameter).toHaveBeenCalledTimes(1))
  expect(vi.mocked(editFilmcraftParameter).mock.calls[0]![0].command).toEqual({ kind: 'upsert', time, value: { Vec2: { x: null, y: 0.123456789012345 } } })
  const select = document.querySelector<HTMLSelectElement>('[aria-label="Ключ параметра Filmcraft"]')!
  select.value = time; select.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  expect(document.querySelector<HTMLInputElement>('[aria-label="Компонент x Filmcraft"]')?.value).toBe('0.125')
  expect(document.querySelector<HTMLInputElement>('[aria-label="Компонент y Filmcraft"]')?.value).toBe('-0.5')
  await input('Компонент x Filmcraft', 'auto')
  button('Ключ Filmcraft в позиции курсора').click()
  await vi.waitFor(() => expect(editFilmcraftParameter).toHaveBeenCalledTimes(2))
  expect(vi.mocked(editFilmcraftParameter).mock.calls[1]![0].command).toEqual({ kind: 'atPlayhead', value: { Vec2: { x: null, y: -0.5 } } })
})
it('preserves all four HDR RGBA channels including alpha', async () => {
  const item = structuredClone(report)
  item.effects[0]!.parameters[0]!.value = { Color: [0.1, 0.2, 0.3, 0.4] }
  item.effects[0]!.parameters[0]!.sampledComponents = [0.2, 0.3, 0.4, 0.5]
  item.effects[0]!.parameters[0]!.keys = []
  await open(item)
  expect(document.body.textContent).toContain('[0.2,0.3,0.4,0.5]')
  await input('Компонент R Filmcraft', '-0.5')
  await input('Компонент G Filmcraft', '1.25')
  await input('Компонент A Filmcraft', '0.125')
  button('Изменить базовое значение').click()
  await vi.waitFor(() => expect(editFilmcraftParameter).toHaveBeenCalledTimes(1))
  expect(vi.mocked(editFilmcraftParameter).mock.calls[0]![0].command).toEqual({ kind: 'setBase', value: { Color: [-0.5, 1.25, 0.3, 0.125] } })
  await input('Компонент B Filmcraft', '1e100')
  button('Ключ Filmcraft в позиции курсора').click(); await tick()
  expect(document.querySelector('[role=alert]')?.textContent).toContain('f32')
  expect(editFilmcraftParameter).toHaveBeenCalledTimes(1)
})
it.each(['document', 'clipId', 'token'] as const)('rejects a late parameter edit after %s changes', async field => {
  vi.mocked(getFilmcraftParameters).mockResolvedValue(report)
  const onedit = vi.fn()
  const props = { document: 'original opaque source', sequenceId: '18446744073709551615', clipId: '18446744073709551614', token: 'owner', timeFrames: '101', onedit }
  const context = writable(props)
  component = mount(ContextHarness, { target: document.body, props: { context } })
  await tick()
  const details = document.querySelector('details')!; details.open = true; details.dispatchEvent(new Event('toggle')); await tick()
  await vi.waitFor(() => expect(button('Ключ Filmcraft в позиции курсора')).toBeTruthy())
  let finish!: (value: FilmcraftImportResult) => void
  vi.mocked(editFilmcraftParameter).mockImplementation(() => new Promise(resolve => { finish = resolve }))
  button('Ключ Filmcraft в позиции курсора').click(); await tick()
  const signal = vi.mocked(editFilmcraftParameter).mock.calls[0]![1]!
  context.set({ ...props, [field]: 'changed context' }); await tick()
  finish(result); await tick(); await tick()
  expect(signal.aborted).toBe(true)
  expect(onedit).not.toHaveBeenCalled()
})
it('edits multiline Unicode Text values and retains the source tag', async () => {
  const item = structuredClone(report)
  item.effects[0]!.parameters[0]!.value = { Text: 'Исходный\nтекст 🎬' }
  item.effects[0]!.parameters[0]!.keys = []
  await open(item)
  expect(document.querySelector<HTMLTextAreaElement>('[aria-label="Текст параметра Filmcraft"]')?.value).toBe('Исходный\nтекст 🎬')
  await input('Текст параметра Filmcraft', '字幕\nТекст é 🎬')
  button('Ключ Filmcraft в позиции курсора').click()
  await vi.waitFor(() => expect(editFilmcraftParameter).toHaveBeenCalledTimes(1))
  expect(vi.mocked(editFilmcraftParameter).mock.calls[0]![0].command).toEqual({ kind: 'atPlayhead', value: { Text: '字幕\nТекст é 🎬' } })
})
it('edits Curve rows without sorting or clamping shape geometry', async () => {
  const item = structuredClone(report)
  item.effects[0]!.parameters[0]!.value = { Curve: [[1.25, -0.5], [-2, 3]] }
  item.effects[0]!.parameters[0]!.keys = []
  await open(item)
  await input('Точка кривой 1 X Filmcraft', '2.75')
  button('Добавить точку кривой Filmcraft').click(); await tick()
  await input('Точка кривой 3 Y Filmcraft', '-0.75')
  button('Изменить базовое значение').click()
  await vi.waitFor(() => expect(editFilmcraftParameter).toHaveBeenCalledTimes(1))
  expect(vi.mocked(editFilmcraftParameter).mock.calls[0]![0].command).toEqual({ kind: 'setBase', value: { Curve: [[2.75, -0.5], [-2, 3], [0, -0.75]] } })
  button('Удалить точку кривой 2').click(); await tick()
  await input('Точка кривой 1 Y Filmcraft', '')
  button('Изменить базовое значение').click(); await tick()
  expect(document.querySelector('[role=alert]')?.textContent).toContain('конечных пар')
  expect(editFilmcraftParameter).toHaveBeenCalledTimes(1)
})
it('authors a mask Path with closure and tangent controls while preserving vertex extensions', async () => {
  const item = structuredClone(report)
  item.effects[0]!.masks = [{ index: 0, name: 'Bezier', parameters: [{ name: 'path', value: { Path: { closed: false, extension: 'path', vertices: [{ p: { x: 1.25, y: 2 }, extension: 'vertex' }, { p: { x: 4, y: 5 } }] } }, sampled: null, keys: [] }] }]
  await open(item)
  const select = document.querySelector<HTMLSelectElement>('[aria-label="Маска параметров Filmcraft"]')!
  select.value = '0'; select.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  await input('Вершина 1 X Filmcraft', '-0.123456789012345')
  button('Добавить t_out вершины 1').click(); await tick()
  await input('Вершина 1 t_out Y Filmcraft', '-0.75')
  const closed = document.querySelector<HTMLInputElement>('[aria-label="Замкнутый путь Filmcraft"]')!
  closed.checked = true; closed.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  button('Добавить вершину Filmcraft').click(); await tick()
  button('Удалить вершину 2').click(); await tick()
  button('Ключ Filmcraft в позиции курсора').click()
  await vi.waitFor(() => expect(editFilmcraftParameter).toHaveBeenCalledTimes(1))
  const call = vi.mocked(editFilmcraftParameter).mock.calls[0]![0]
  expect(call.address).toMatchObject({ effectIndex: 2, effectKind: 'gamma_correction', maskIndex: 0, parameter: 'path' })
  expect(call.command).toEqual({ kind: 'atPlayhead', value: { Path: { closed: true, extension: 'path', vertices: [{ p: { x: -0.123456789012345, y: 2 }, t_out: { x: 0, y: -0.75 }, extension: 'vertex' }, { p: { x: 0, y: 0 } }] } } })
})
it('rejects empty path coordinates instead of converting them to auto or zero', async () => {
  const item = structuredClone(report)
  item.effects[0]!.parameters[0]!.value = { Path: { vertices: [{ p: { x: 1, y: 2 } }] } }
  item.effects[0]!.parameters[0]!.keys = []
  await open(item)
  await input('Вершина 1 X Filmcraft', '')
  button('Сохранить ключ Filmcraft').click(); await tick()
  expect(document.querySelector('[role=alert]')?.textContent).toContain('конечными координатами')
  expect(editFilmcraftParameter).not.toHaveBeenCalled()
})
it('creates rectangle and ellipse masks through the native effect address', async () => {
  const onedit = await open()
  vi.mocked(editFilmcraftMask).mockResolvedValue(result)
  await input('Имя новой маски Filmcraft', 'Эллипс 🎬')
  await input('Граница маски X0 Filmcraft', '-1.25')
  await input('Граница маски Y0 Filmcraft', '2')
  await input('Граница маски X1 Filmcraft', '7')
  await input('Граница маски Y1 Filmcraft', '6')
  const shape = document.querySelector<HTMLSelectElement>('[aria-label="Форма новой маски Filmcraft"]')!
  shape.value = 'ellipse'; shape.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  button('Создать маску Filmcraft').click()
  await vi.waitFor(() => expect(onedit).toHaveBeenCalledWith(result))
  expect(editFilmcraftMask).toHaveBeenCalledWith({ document: 'opaque u64 source', address: { sequenceId: '18446744073709551615', clipId: '18446744073709551614', effectIndex: 2, effectKind: 'gamma_correction' }, command: { kind: 'create', name: 'Эллипс 🎬', shape: { kind: 'ellipse', x0: -1.25, y0: 2, x1: 7, y1: 6 } } }, expect.any(AbortSignal))
  expect(editFilmcraftParameter).not.toHaveBeenCalled()
})
it('creates a free-draw mask without inventing geometry and rejects invalid bounds', async () => {
  await open()
  vi.mocked(editFilmcraftMask).mockResolvedValue(result)
  await input('Граница маски X1 Filmcraft', '')
  button('Создать маску Filmcraft').click(); await tick()
  expect(editFilmcraftMask).not.toHaveBeenCalled()
  expect(document.querySelector('[role=alert]')?.textContent).toContain('возрастающими')
  const shape = document.querySelector<HTMLSelectElement>('[aria-label="Форма новой маски Filmcraft"]')!
  shape.value = 'freeDraw'; shape.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  button('Создать маску Filmcraft').click()
  await vi.waitFor(() => expect(editFilmcraftMask).toHaveBeenCalledTimes(1))
  expect(vi.mocked(editFilmcraftMask).mock.calls[0]![0].command).toEqual({ kind: 'create', name: 'Маска', shape: { kind: 'freeDraw' } })
})
async function openMaskTools() {
  const item = structuredClone(report)
  item.effects[0]!.masks = [{ index: 0, name: 'First', mode: 'Add', inverted: false, trackMethod: 'Position', parameters: [{ name: 'path', value: { Path: { closed: true, vertices: [] } }, sampled: null, keys: [] }] }, { index: 1, name: 'Second', parameters: [] }]
  const onedit = await open(item)
  vi.mocked(editFilmcraftMask).mockResolvedValue(result)
  const select = document.querySelector<HTMLSelectElement>('[aria-label="Маска параметров Filmcraft"]')!
  select.value = '0'; select.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  return onedit
}
it('updates mask name, combine mode, inversion and tracking configuration atomically', async () => {
  const onedit = await openMaskTools()
  await input('Имя маски Filmcraft', '字幕 🎬')
  const mode = document.querySelector<HTMLSelectElement>('[aria-label="Режим маски Filmcraft"]')!
  mode.value = 'Subtract'; mode.dispatchEvent(new Event('change', { bubbles: true }))
  const method = document.querySelector<HTMLSelectElement>('[aria-label="Метод трекинга маски Filmcraft"]')!
  method.value = 'PositionRotation'; method.dispatchEvent(new Event('change', { bubbles: true }))
  const inverted = document.querySelector<HTMLInputElement>('[aria-label="Инверсия маски Filmcraft"]')!
  inverted.checked = true; inverted.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  button('Сохранить настройки маски Filmcraft').click()
  await vi.waitFor(() => expect(onedit).toHaveBeenCalledTimes(1))
  expect(vi.mocked(editFilmcraftMask).mock.calls[0]![0].command).toEqual({ kind: 'settings', maskIndex: 0, name: '字幕 🎬', mode: 'Subtract', inverted: true, trackMethod: 'PositionRotation' })
})
it.each(['move', 'delete'] as const)('performs mask %s as a native document edit', async kind => {
  await openMaskTools()
  button(kind === 'move' ? 'Маску Filmcraft ниже' : 'Удалить маску Filmcraft').click()
  await vi.waitFor(() => expect(editFilmcraftMask).toHaveBeenCalledTimes(1))
  expect(vi.mocked(editFilmcraftMask).mock.calls[0]![0].command).toEqual(kind === 'move' ? { kind, maskIndex: 0, destination: 1 } : { kind, maskIndex: 0 })
})
it('never publishes a mask operation after its editor closes', async () => {
  const onedit = await open()
  let finish!: (value: FilmcraftImportResult) => void
  vi.mocked(editFilmcraftMask).mockImplementation(() => new Promise(resolve => { finish = resolve }))
  button('Создать маску Filmcraft').click(); await tick()
  const signal = vi.mocked(editFilmcraftMask).mock.calls[0]![1]!
  const details = document.querySelector('details')!; details.open = false; details.dispatchEvent(new Event('toggle')); await tick()
  finish(result); await tick(); await tick()
  expect(signal.aborted).toBe(true)
  expect(onedit).not.toHaveBeenCalled()
})
it.each(['effects.reset', 'effects.toggleEnabled', 'effects.remove'] as const)('adopts %s through the owner command with a base-snapshot fence', async command => {
  vi.mocked(editFilmcraftEffectRack).mockResolvedValue(result)
  const onedit = await open(report, 'owner')
  const label = command === 'effects.reset' ? 'Сбросить эффект Filmcraft' : command === 'effects.remove' ? 'Удалить эффект Filmcraft' : 'Переключить эффект Filmcraft'
  document.querySelector<HTMLButtonElement>(`[aria-label="${label}"]`)!.click()
  await vi.waitFor(() => expect(onedit).toHaveBeenCalledWith(result, { document: 'opaque u64 source', sequenceId: '18446744073709551615', clipId: '18446744073709551614', token: 'owner' }))
  expect(editFilmcraftEffectRack).toHaveBeenCalledWith('owner', { document: 'opaque u64 source', sequence: '18446744073709551615', clip: '18446744073709551614', index: 2, command }, expect.any(AbortSignal))
})
it('requires an owner for rack mutations', async () => {
  await open(report, null)
  expect(document.querySelector<HTMLButtonElement>('[aria-label="Сбросить эффект Filmcraft"]')!.disabled).toBe(true)
  expect(editFilmcraftEffectRack).not.toHaveBeenCalled()
})
it.each(['document', 'clipId', 'token'] as const)('rejects late rack adoption after %s changes', async field => {
  let resolve!: (value: FilmcraftImportResult) => void
  vi.mocked(editFilmcraftEffectRack).mockImplementation(() => new Promise(done => resolve = done))
  vi.mocked(getFilmcraftParameters).mockResolvedValue(report)
  const onedit = vi.fn()
  const props = { document: 'old', sequenceId: '21', clipId: '29', token: 'owner', timeFrames: '0', onedit }
  const context = writable(props)
  component = mount(ContextHarness, { target: document.body, props: { context } })
  const details = document.querySelector('details')!; details.open = true; details.dispatchEvent(new Event('toggle')); await tick()
  await vi.waitFor(() => expect(document.querySelector('[aria-label="Сбросить эффект Filmcraft"]')).toBeTruthy())
  document.querySelector<HTMLButtonElement>('[aria-label="Сбросить эффект Filmcraft"]')!.click()
  await vi.waitFor(() => expect(editFilmcraftEffectRack).toHaveBeenCalledTimes(1))
  const signal = vi.mocked(editFilmcraftEffectRack).mock.calls[0]![2]!
  context.set({ ...props, [field]: 'changed' }); await tick()
  expect(signal.aborted).toBe(true)
  resolve(result); await tick(); await tick()
  expect(onedit).not.toHaveBeenCalled()
})
it('keeps the source snapshot after an intrinsic-removal refusal', async () => {
  vi.mocked(editFilmcraftEffectRack).mockRejectedValue(new Error('intrinsic effects cannot be removed'))
  const onedit = await open(report, 'owner')
  document.querySelector<HTMLButtonElement>('[aria-label="Удалить эффект Filmcraft"]')!.click()
  await vi.waitFor(() => expect(document.querySelector('[role="alert"]')?.textContent).toContain('intrinsic effects cannot be removed'))
  expect(onedit).not.toHaveBeenCalled()
})
it('refreshes the enabled state after adopting an exact returned snapshot', async () => {
  vi.mocked(editFilmcraftEffectRack).mockResolvedValue(result)
  vi.mocked(getFilmcraftParameters).mockImplementation(async input => ({ ...report, effects: [{ ...report.effects[0]!, enabled: input.document !== result.documentText }] }))
  const initial = { document: 'old', sequenceId: '21', clipId: '29', token: 'owner', timeFrames: '0', onedit: (changed: FilmcraftImportResult) => { context.set({ ...initial, document: changed.documentText }) } }
  const context = writable(initial)
  component = mount(ContextHarness, { target: document.body, props: { context } })
  const details = document.querySelector('details')!; details.open = true; details.dispatchEvent(new Event('toggle')); await tick()
  await vi.waitFor(() => expect(document.querySelector('[aria-label="Переключить эффект Filmcraft"]')).toBeTruthy())
  const toggle = () => document.querySelector<HTMLButtonElement>('[aria-label="Переключить эффект Filmcraft"]')!
  expect(toggle().textContent).toBe('Выключить эффект')
  toggle().click()
  await vi.waitFor(() => expect(toggle().textContent).toBe('Включить эффект'))
  expect(getFilmcraftParameters).toHaveBeenLastCalledWith(expect.objectContaining({ document: result.documentText }), expect.any(AbortSignal))
})
function effectOption(id: string, kind: 'Video' | 'Audio' = 'Video') { return { id, name: id === 'gaussian_blur' ? 'Gaussian Blur' : id, kind, category: [kind === 'Video' ? 'Video Effects' : 'Audio Effects'], folder: null, path: kind === 'Video' ? 'Video Effects' : 'Audio Effects', params: [], badges: { accelerated: true, float32: true, yuv: false } } }
async function chooseNewEffect() { button('Добавить эффект…').click(); await tick(); await vi.waitFor(() => expect(document.querySelector<HTMLSelectElement>('[aria-label="Новый эффект Filmcraft"]')?.options.length).toBeGreaterThan(0)) }
it('adds an effect with exact clip identity and excludes dedicated controls from the chooser', async () => {
  vi.mocked(listFilmcraftEffects).mockResolvedValue([effectOption('motion'), effectOption('graphic_text'), effectOption('gaussian_blur'), effectOption('volume_a', 'Audio')])
  vi.mocked(applyFilmcraftEffect).mockResolvedValue(result)
  const onedit = await open(report, 'owner'); await chooseNewEffect()
  expect(listFilmcraftEffects).toHaveBeenCalledWith('owner', 'Video', expect.any(AbortSignal))
  expect([...document.querySelector<HTMLSelectElement>('[aria-label="Новый эффект Filmcraft"]')!.options].map(o => o.value)).toEqual(['gaussian_blur'])
  await input('Поиск эффекта Filmcraft', 'no matching effect')
  expect(document.querySelector<HTMLButtonElement>('[aria-label="Применить новый эффект Filmcraft"]')!.disabled).toBe(true)
  await input('Поиск эффекта Filmcraft', 'GAUSSIAN')
  document.querySelector<HTMLButtonElement>('[aria-label="Применить новый эффект Filmcraft"]')!.click()
  await vi.waitFor(() => expect(onedit).toHaveBeenCalledWith(result, { document: 'opaque u64 source', sequenceId: '18446744073709551615', clipId: '18446744073709551614', token: 'owner' }))
  expect(applyFilmcraftEffect).toHaveBeenCalledWith('owner', { document: 'opaque u64 source', sequence: '18446744073709551615', clip: '18446744073709551614', effect: 'gaussian_blur' }, expect.any(AbortSignal))
})
it('can add to an empty rack and keeps the snapshot after a refused apply', async () => {
  vi.mocked(listFilmcraftEffects).mockResolvedValue([effectOption('gaussian_blur')])
  vi.mocked(applyFilmcraftEffect).mockRejectedValue(new Error('apply refused'))
  const onedit = await open({ ...report, effects: [] }, 'owner'); await chooseNewEffect()
  document.querySelector<HTMLButtonElement>('[aria-label="Применить новый эффект Filmcraft"]')!.click()
  await vi.waitFor(() => expect(document.querySelector('[role="alert"]')?.textContent).toContain('apply refused'))
  expect(onedit).not.toHaveBeenCalled()
})
it('retries catalog failure without applying an unknown effect', async () => {
  vi.mocked(listFilmcraftEffects).mockRejectedValueOnce(new Error('catalog unavailable')).mockResolvedValue([effectOption('gaussian_blur')])
  await open(report, 'owner'); button('Добавить эффект…').click(); await tick()
  await vi.waitFor(() => expect(document.querySelector('[role="alert"]')?.textContent).toContain('catalog unavailable'))
  expect(applyFilmcraftEffect).not.toHaveBeenCalled()
  button('Повторить загрузку эффектов').click()
  await vi.waitFor(() => expect(document.querySelector<HTMLSelectElement>('[aria-label="Новый эффект Filmcraft"]')!.options.length).toBe(1))
})
it('rejects stale video catalog results after switching to an audio clip context', async () => {
  let resolve!: (value: ReturnType<typeof effectOption>[]) => void
  vi.mocked(listFilmcraftEffects).mockImplementationOnce(() => new Promise(done => resolve = done)).mockResolvedValue([effectOption('volume', 'Audio'), effectOption('volume_a', 'Audio')])
  vi.mocked(getFilmcraftParameters).mockResolvedValue(report)
  const props = { document: 'old', sequenceId: '21', clipId: '29', token: 'owner', timeFrames: '0', trackKind: 'Video' as 'Video' | 'Audio', onedit: vi.fn() }
  const context = writable(props); component = mount(ContextHarness, { target: document.body, props: { context } })
  const details = document.querySelector('details')!; details.open = true; details.dispatchEvent(new Event('toggle')); await tick()
  await vi.waitFor(() => expect(button('Добавить эффект…')).toBeTruthy()); button('Добавить эффект…').click(); await tick()
  await vi.waitFor(() => expect(listFilmcraftEffects).toHaveBeenCalledTimes(1))
  const oldSignal = vi.mocked(listFilmcraftEffects).mock.calls[0]![2]!
  context.set({ ...props, clipId: '30', trackKind: 'Audio' }); await tick()
  await vi.waitFor(() => expect(listFilmcraftEffects).toHaveBeenCalledTimes(2))
  expect(oldSignal.aborted).toBe(true)
  resolve([effectOption('gaussian_blur')]); await tick(); await tick()
  expect([...document.querySelector<HTMLSelectElement>('[aria-label="Новый эффект Filmcraft"]')!.options].map(o => o.value)).toEqual(['volume_a'])
})
it.each(['document', 'clipId', 'token'] as const)('rejects late effect application after %s changes', async field => {
  let resolve!: (value: FilmcraftImportResult) => void
  vi.mocked(applyFilmcraftEffect).mockImplementation(() => new Promise(done => resolve = done))
  vi.mocked(listFilmcraftEffects).mockResolvedValue([effectOption('gaussian_blur')]); vi.mocked(getFilmcraftParameters).mockResolvedValue(report)
  const onedit = vi.fn(); const props = { document: 'old', sequenceId: '21', clipId: '29', token: 'owner', timeFrames: '0', onedit }
  const context = writable(props); component = mount(ContextHarness, { target: document.body, props: { context } })
  const details = document.querySelector('details')!; details.open = true; details.dispatchEvent(new Event('toggle')); await tick()
  await vi.waitFor(() => expect(button('Добавить эффект…')).toBeTruthy()); await chooseNewEffect()
  document.querySelector<HTMLButtonElement>('[aria-label="Применить новый эффект Filmcraft"]')!.click()
  await vi.waitFor(() => expect(applyFilmcraftEffect).toHaveBeenCalledTimes(1))
  const signal = vi.mocked(applyFilmcraftEffect).mock.calls[0]![2]!
  context.set({ ...props, [field]: 'changed' }); await tick(); expect(signal.aborted).toBe(true)
  resolve(result); await tick(); await tick(); expect(onedit).not.toHaveBeenCalled()
})

it('applies source setParam at the exact cursor and passes the document fence to project history', async () => {
  vi.mocked(setFilmcraftEffectParameter).mockResolvedValue(result)
  const onedit = await open(report, 'owner')
  await input('Значение параметра Filmcraft', '2.25')
  button('Применить значение в позиции курсора').click()
  await vi.waitFor(() => expect(onedit).toHaveBeenCalledWith(result, { document: 'opaque u64 source', sequenceId: '18446744073709551615', clipId: '18446744073709551614', token: 'owner' }))
  expect(setFilmcraftEffectParameter).toHaveBeenCalledWith('owner', { document: 'opaque u64 source', sequence: '18446744073709551615', clip: '18446744073709551614', effect: 2, param: 'gamma', value: { Float: 2.25 }, timeFrames: '101' }, expect.any(AbortSignal))
  expect(editFilmcraftParameter).not.toHaveBeenCalled()
})
it('does not expose source parameter mutation without an authenticated owner', async () => {
  await open()
  expect(button('Применить значение в позиции курсора').disabled).toBe(true)
  button('Применить значение в позиции курсора').click()
  expect(setFilmcraftEffectParameter).not.toHaveBeenCalled()
})

it.each(['document', 'sequenceId', 'clipId', 'token', 'timeFrames'] as const)('refuses late source parameter adoption after %s changes', async field => {
  let resolve!: (value: FilmcraftImportResult) => void
  vi.mocked(setFilmcraftEffectParameter).mockImplementation(() => new Promise(done => resolve = done))
  vi.mocked(getFilmcraftParameters).mockResolvedValue(report)
  const onedit = vi.fn()
  const props = { document: 'old', sequenceId: '21', clipId: '29', token: 'owner', timeFrames: '0', onedit }
  const context = writable(props)
  component = mount(ContextHarness, { target: document.body, props: { context } })
  const details = document.querySelector('details')!; details.open = true; details.dispatchEvent(new Event('toggle')); await tick()
  await vi.waitFor(() => expect(button('Применить значение в позиции курсора')).toBeTruthy())
  button('Применить значение в позиции курсора').click()
  await vi.waitFor(() => expect(setFilmcraftEffectParameter).toHaveBeenCalledTimes(1))
  const signal = vi.mocked(setFilmcraftEffectParameter).mock.calls[0]![2]!
  context.set({ ...props, [field]: 'changed' }); await tick()
  expect(signal.aborted).toBe(true)
  resolve(result); await tick(); await tick()
  expect(onedit).not.toHaveBeenCalled()
})
it('preserves the project on source setParam refusal', async () => {
  vi.mocked(setFilmcraftEffectParameter).mockRejectedValue(new Error('source edit refused'))
  const onedit = await open(report, 'owner')
  button('Применить значение в позиции курсора').click()
  await vi.waitFor(() => expect(document.querySelector('[role="alert"]')?.textContent).toContain('source edit refused'))
  expect(onedit).not.toHaveBeenCalled()
})
it.each([
  ['effects.toggleAnimation', 'Отключить анимацию'],
  ['effects.addKeyframe', 'Добавить/удалить ключ в позиции курсора'],
] as const)('routes %s with the native report cursor, even if the value draft is invalid', async (command, label) => {
  vi.mocked(editFilmcraftAnimation).mockResolvedValue(result)
  const onedit = await open({ ...report, timelineTicks: '9007199254740995' }, 'owner')
  await input('Значение параметра Filmcraft', 'invalid draft')
  button(label).click()
  await vi.waitFor(() => expect(onedit).toHaveBeenCalledWith(result, { document: 'opaque u64 source', sequenceId: '18446744073709551615', clipId: '18446744073709551614', token: 'owner' }))
  expect(editFilmcraftAnimation).toHaveBeenCalledWith('owner', { command, document: 'opaque u64 source', sequence: '18446744073709551615', clip: '18446744073709551614', effect: 2, param: 'gamma', playhead: '9007199254740995' }, expect.any(AbortSignal))
  expect(setFilmcraftEffectParameter).not.toHaveBeenCalled()
})
it('exposes animation activation for a static parameter', async () => {
  const staticReport = structuredClone(report); staticReport.effects[0]!.parameters[0]!.keys = []
  await open(staticReport, 'owner')
  expect(button('Включить анимацию')).toBeTruthy()
  expect(button('Отключить анимацию')).toBeUndefined()
})
it.each(['effects.toggleAnimation', 'effects.addKeyframe'] as const)('rejects late %s after a document revision changes', async command => {
  let resolve!: (value: FilmcraftImportResult) => void
  vi.mocked(editFilmcraftAnimation).mockImplementation(() => new Promise(done => resolve = done))
  vi.mocked(getFilmcraftParameters).mockResolvedValue(report)
  const onedit = vi.fn()
  const props = { document: 'old', sequenceId: '21', clipId: '29', token: 'owner', timeFrames: '0', onedit }
  const context = writable(props); component = mount(ContextHarness, { target: document.body, props: { context } })
  const details = document.querySelector('details')!; details.open = true; details.dispatchEvent(new Event('toggle')); await tick()
  await vi.waitFor(() => expect(button('Отключить анимацию')).toBeTruthy())
  button(command === 'effects.toggleAnimation' ? 'Отключить анимацию' : 'Добавить/удалить ключ в позиции курсора').click()
  await vi.waitFor(() => expect(editFilmcraftAnimation).toHaveBeenCalledTimes(1))
  const signal = vi.mocked(editFilmcraftAnimation).mock.calls[0]![2]!
  context.set({ ...props, document: 'new revision' }); await tick(); expect(signal.aborted).toBe(true)
  resolve(result); await tick(); await tick(); expect(onedit).not.toHaveBeenCalled()
})

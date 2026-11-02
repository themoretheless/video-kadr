import { writeFileSync } from 'node:fs'
// @ts-expect-error Mounted tests require the Svelte client entry.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { beforeEach, afterEach, it, expect, vi } from 'vitest'
import { evaluateMotionPath, evaluateNestedFrame, type NativeMotionPathResponse } from '$lib/api'
import { nestedComposition, nestedHistory, closeNestedComposition, undoNestedComposition, redoNestedComposition } from '$lib/state/nestedComposition.svelte'
import { authState } from '$lib/state/auth.svelte'
import { createComposition, addTrack, addClip } from '$lib/composition/commands'
import { buildCompositionWireDocument } from '$lib/composition/payload'
import Inspector from './NestedMotionPathInspector.svelte'
vi.mock('$lib/api', async original => ({ ...await original<typeof import('$lib/api')>(), evaluateMotionPath: vi.fn(), evaluateNestedFrame: vi.fn() }))
type NativeMotionPath = NativeMotionPathResponse['path']
let component: ReturnType<typeof mount> | undefined
const node = (id: string, x: number) => ({ id, point: { x, y: 15 }, inHandle: { x: 0, y: 0 }, outHandle: { x: 0, y: 0 }, segment: 'line' as const, tangentMode: 'corner' as const })
const initialPath: NativeMotionPath = { nodes: [node('start', 5), node('end', 45)] }
beforeEach(() => {
  closeNestedComposition(); authState.token = null
  vi.mocked(evaluateMotionPath).mockReset(); vi.mocked(evaluateNestedFrame).mockReset(); vi.mocked(evaluateNestedFrame).mockResolvedValue({ entries: [] })
  let doc = createComposition({ width: 64, height: 48, fps: 30, backgroundColor: '#000000FF' })
  doc = addTrack(doc, { id: 'text', kind: 'text', name: 'Text', locked: false, hidden: false, clips: [] })
  doc = addClip(doc, 'text', { id: 'title', kind: 'text', text: 'Path', timelineStartTicks: 1000000, durationTicks: 2000000, x: 17, y: 23, opacity: 1, style: { fontSizePx: 24, color: '#FFFFFFFF', align: 'center' } })
  const content = buildCompositionWireDocument(doc), clip = content.tracks[0]!.clips[0]!
  if (!('transform' in clip)) throw new Error('Visual fixture required')
  Object.assign(clip.transform, { position: { mode: 'path', path: structuredClone(initialPath), progress: { mode: 'keyframes', track: { timeBase: 1000, interpolation: 'linear', sampleClock: { rateNum: 2, rateDen: 1, offsetNum: 250000 }, keyframes: [{ tick: 250, value: 0 }, { tick: 1250, value: 1 }] } } } })
  nestedComposition.document = { schemaVersion: 1, root: 'root', compositions: { root: { content, instances: [] } } }
  nestedComposition.timeUs = 1250000; nestedComposition.renderedOutputs = { root: 'old-render' }
})
afterEach(async () => { if (component) await unmount(component); component = undefined; closeNestedComposition(); authState.token = null; document.body.innerHTML = '' })
function clip() { const value = nestedComposition.document!.compositions.root!.content.tracks[0]!.clips[0]!; if (!('transform' in value)) throw new Error('Visual clip required'); return value }
function button(text: string) { return [...document.querySelectorAll<HTMLButtonElement>('button')].find(item => item.textContent === text)! }
function input(label: string, value: string) { const field = document.querySelector<HTMLInputElement>(`input[aria-label="${label}"]`)!; field.value = value; field.dispatchEvent(new Event('input', { bubbles: true })); field.dispatchEvent(new Event('change', { bubbles: true })) }
it('sends node coordinates to Rust, preserves clock/shadow XY and commits one Undo/Redo', async () => {
  const before = JSON.stringify(nestedComposition.document), progress = JSON.stringify(clip().transform.position), shadow = clip().transform.x
  const edited = { nodes: [node('start', 5), node('end', 55)] }
  vi.mocked(evaluateMotionPath).mockResolvedValue({ path: edited, length: 50, points: [] })
  component = mount(Inspector, { target: document.body }); await tick(); input('Узел 2: Точка X', '55')
  await vi.waitFor(() => expect(nestedHistory.past).toHaveLength(1))
  expect(evaluateMotionPath).toHaveBeenCalledWith({ operation: 'edit', index: 1, part: 'point', value: { x: 55, y: 15 }, path: initialPath })
  expect(clip().transform.position).toMatchObject({ path: edited, progress: { track: { sampleClock: { rateNum: 2, rateDen: 1, offsetNum: 250000 } } } })
  expect(clip().transform.x).toEqual(shadow); expect(nestedComposition.renderedOutputs).toEqual({})
  expect(evaluateNestedFrame).toHaveBeenCalledOnce(); undoNestedComposition(); expect(JSON.stringify(nestedComposition.document)).toBe(before)
  redoNestedComposition(); expect(JSON.stringify(clip().transform.position)).not.toBe(progress)
})
it('appends through native solve and records the exact request/accepted scene for cross-language validation', async () => {
  vi.mocked(evaluateMotionPath).mockImplementationOnce(async request => { if (request.operation !== 'solve') throw new Error('Native solve required'); return { path: request.path, length: 140, points: [] } })
  component = mount(Inspector, { target: document.body }); await tick(); button('Добавить конечный узел').click()
  await vi.waitFor(() => expect(nestedHistory.past).toHaveLength(1))
  const request = vi.mocked(evaluateMotionPath).mock.calls[0]![0]
  expect(request).toMatchObject({ operation: 'solve', path: { nodes: [{ id: 'start' }, { id: 'end' }, { point: { x: 145, y: 15 }, inHandle: { x: 0, y: 0 }, outHandle: { x: 0, y: 0 }, segment: 'line', tangentMode: 'corner' }] } })
  const position = clip().transform.position; if (position?.mode !== 'path') throw new Error('Path required')
  expect(position.path.nodes).toHaveLength(3); expect(position.progress).toMatchObject({ track: { sampleClock: { rateNum: 2, rateDen: 1, offsetNum: 250000 } } })
  const oracle = process.env.NATIVE_PATH_GEOMETRY_ORACLE
  if (oracle) writeFileSync(oracle, JSON.stringify({ schemaVersion: 1, request, composition: nestedComposition.document!.compositions.root!.content }))
})
it.each(['channel', 'session', 'unmount'] as const)('rejects a pending geometry reply after context change: %s', async kind => {
  let finish!: (value: { path: NativeMotionPath; length: number; points: [] }) => void
  vi.mocked(evaluateMotionPath).mockImplementationOnce(() => new Promise(resolve => { finish = resolve }))
  const before = nestedComposition.document
  component = mount(Inspector, { target: document.body }); await tick(); input('Узел 2: Точка X', '55')
  await vi.waitFor(() => expect(finish).toBeDefined())
  if (kind === 'channel') nestedComposition.activeCompositionId = 'other'
  if (kind === 'session') authState.token = 'different-session'
  if (kind === 'unmount') { await unmount(component); component = undefined }
  finish({ path: { nodes: [node('start', 5), node('end', 55)] }, length: 50, points: [] })
  await vi.waitFor(() => expect(nestedHistory.busy).toBe(false)); expect(nestedHistory.past).toHaveLength(0); expect(nestedComposition.document).toBe(before)
})
it('keeps the project when native geometry or complete-project validation refuses the edit', async () => {
  const before = nestedComposition.document
  vi.mocked(evaluateMotionPath).mockRejectedValueOnce(new Error('invalid geometry'))
  component = mount(Inspector, { target: document.body }); await tick(); input('Узел 2: Точка X', '55')
  await vi.waitFor(() => expect(nestedHistory.error).toBe('invalid geometry')); expect(evaluateNestedFrame).not.toHaveBeenCalled(); expect(nestedComposition.document).toBe(before)
  await tick(); expect(document.querySelector<HTMLInputElement>('[aria-label="Узел 2: Точка X"]')!.value).toBe('45')
  vi.mocked(evaluateMotionPath).mockResolvedValue({ path: initialPath, length: 40, points: [] }); vi.mocked(evaluateNestedFrame).mockRejectedValueOnce(new Error('invalid scene'))
  input('Узел 2: Точка X', '55'); await vi.waitFor(() => expect(nestedHistory.error).toBe('invalid scene'))
  expect(nestedHistory.past).toHaveLength(0); expect(nestedComposition.document).toBe(before)
})

it.each(['translate','toggle','insert','remove','mode','handle','solve'] as const)('routes the geometry control to the native operation: %s',async action=>{
  const accepted={nodes:[node('start',6),node('end',46)]}
  vi.mocked(evaluateMotionPath).mockResolvedValue({path:accepted,length:40,points:[]})
  component=mount(Inspector,{target:document.body});await tick()
  if(action==='translate'){input('Смещение траектории X','3');input('Смещение траектории Y','4');button('Перенести путь').click()}
  if(action==='toggle')document.querySelector<HTMLButtonElement>('[aria-label="Переключить сегмент траектории 1"]')!.click()
  if(action==='insert')document.querySelector<HTMLButtonElement>('[aria-label="Разделить сегмент траектории 1"]')!.click()
  if(action==='remove')document.querySelector<HTMLButtonElement>('[aria-label="Удалить узел траектории 2"]')!.click()
  if(action==='mode'){const select=document.querySelector<HTMLSelectElement>('[aria-label="Узел 1: режим ручек"]')!;select.value='auto';select.dispatchEvent(new Event('change',{bubbles:true}))}
  if(action==='handle')input('Узел 1: Исходящая ручка Y','9')
  if(action==='solve')button('Пересчитать ручки').click()
  await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1))
  const expected={translate:{operation:'translate',delta:{x:3,y:4}},toggle:{operation:'toggle_segment',index:0},insert:{operation:'insert',segment:0,t:0.5,id:expect.any(String)},remove:{operation:'remove',index:1},mode:{operation:'mode',index:0,mode:'auto'},handle:{operation:'edit',index:0,part:'out_handle',value:{x:0,y:9}},solve:{operation:'solve'}}[action]
  expect(evaluateMotionPath).toHaveBeenCalledWith({...expected,path:initialPath})
  expect(clip().transform.position).toMatchObject({path:accepted,progress:{track:{sampleClock:{rateNum:2,rateDen:1,offsetNum:250000}}}})
})
it('creates a path from static XY and refuses direct switching from animated XY',async()=>{
  Object.assign(clip().transform,{position:undefined})
  component=mount(Inspector,{target:document.body});await tick();button('Создать путь из статической позиции').click()
  await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1))
  expect(clip().transform.position).toMatchObject({mode:'path',path:{nodes:[{point:{x:17,y:23}}]},progress:{mode:'constant',value:0}})
  expect(evaluateMotionPath).not.toHaveBeenCalled();expect(evaluateNestedFrame).toHaveBeenCalledOnce()
  undoNestedComposition();Object.assign(clip().transform,{x:{mode:'keyframes',track:{timeBase:1000,interpolation:'linear',keyframes:[{tick:0,value:17},{tick:1000,value:23}]}}})
  await tick();expect(button('Создать путь из статической позиции').disabled).toBe(true)
})

it('arrives at a node using native distance progress and the retained key clock',async()=>{
  const position=clip().transform.position;if(position?.mode!=='path')throw new Error('Path required')
  Object.assign(position,{path:{nodes:[node('start',5),node('middle',45),node('end',145)]}})
  vi.mocked(evaluateMotionPath).mockResolvedValue({path:position.path,length:140,points:[],nodeProgresses:[0,2/7,1]})
  const before=JSON.stringify(nestedComposition.document)
  component=mount(Inspector,{target:document.body});await tick()
  document.querySelector<HTMLButtonElement>('[aria-label="Прибыть к узлу 2 сейчас"]')!.click()
  await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1))
  expect(evaluateMotionPath).toHaveBeenCalledWith({operation:'measure',path:position.path})
  expect(clip().transform.position).toMatchObject({progress:{track:{sampleClock:{rateNum:2,rateDen:1,offsetNum:250000},keyframes:[{tick:250},{tick:750,value:2/7},{tick:1250}]}}})
  undoNestedComposition();expect(JSON.stringify(nestedComposition.document)).toBe(before)
})

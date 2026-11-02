import { writeFileSync } from 'node:fs'
// @ts-expect-error Mounted tests require Svelte client entry.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { beforeEach, afterEach, it, expect, vi } from 'vitest'
import { evaluateNestedFrame, renderNestedPreview, type NativeNestedPreviewFrame } from '$lib/api'
import NestedCompositionPreview from './NestedCompositionPreview.svelte'
import { authState } from '$lib/state/auth.svelte'
import { nestedEffectPreview } from '$lib/state/nestedEffectPreview.svelte'
import { nestedComposition, nestedHistory, closeNestedComposition, undoNestedComposition, redoNestedComposition } from '$lib/state/nestedComposition.svelte'
import { createComposition, addTrack, addClip } from '$lib/composition/commands'
import { buildCompositionWireDocument } from '$lib/composition/payload'
import { agentSession } from '$lib/agent/session.svelte'
import { readNativeKeyframeValue } from '$lib/agent/nativeKeyframes'
import { sampleAnimatableValue } from '$lib/composition/keyframes'
import Inspector from './NestedScalarKeyInspector.svelte'
vi.mock('$lib/api', async original => ({ ...await original<typeof import('$lib/api')>(), evaluateNestedFrame: vi.fn(), renderNestedPreview: vi.fn() }))
let component: ReturnType<typeof mount> | undefined
beforeEach(() => {
  closeNestedComposition(); authState.token=null; vi.mocked(renderNestedPreview).mockReset(); vi.mocked(evaluateNestedFrame).mockReset(); vi.mocked(evaluateNestedFrame).mockResolvedValue({ entries: [] })
  let doc = createComposition({ width: 64, height: 48, fps: 30, backgroundColor: '#000000FF' })
  doc = addTrack(doc, { id: 't', name: 'Text', kind: 'text', locked: false, hidden: false, clips: [] })
  doc = addClip(doc, 't', { id: 'title', kind: 'text', text: 'Keys', timelineStartTicks: 1000000, durationTicks: 2000000, x: 0, y: 0, opacity: 1, style: { fontSizePx: 24, color: '#FFFFFFFF', align: 'center' }, animation: { x: { mode: 'keyframes', track: { timeBase: 1000, interpolation: 'linear', sampleClock: { rateNum: 2, rateDen: 1, offsetNum: 250000 }, keyframes: [{ tick: 250, value: 0 }, { tick: 1250, value: 10 }] } } } })
  nestedComposition.document = { schemaVersion: 1, root: 'root', compositions: { root: { content: buildCompositionWireDocument(doc), instances: [] } } }
  nestedComposition.timeUs = 1100000; nestedComposition.renderedOutputs = { root: 'old-render' }
})
afterEach(async () => { if (component) await unmount(component); component = undefined; closeNestedComposition(); authState.token=null; vi.restoreAllMocks(); document.body.innerHTML = '' })
function button(label: string) { return document.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`)! }
it('shows mapped local times, inserts the sampled value at the cursor and supports Undo/Redo', async () => {
  const before = JSON.stringify(nestedComposition.document)
  component = mount(Inspector, { target: document.body }); await tick()
  expect(document.querySelector<HTMLInputElement>('input[aria-label="Скалярный ключ 1250: время"]')!.value).toBe('0.5')
  button('Добавить скалярный ключ по курсору').click()
  await vi.waitFor(() => expect(nestedHistory.past).toHaveLength(1)); await tick()
  expect(document.querySelector<HTMLInputElement>('input[aria-label="Скалярный ключ 450: значение"]')!.value).toBe('2')
  expect(nestedComposition.renderedOutputs).toEqual({})
  expect(evaluateNestedFrame).toHaveBeenCalledOnce()
  undoNestedComposition(); expect(JSON.stringify(nestedComposition.document)).toBe(before)
  expect(nestedComposition.renderedOutputs).toEqual({ root: 'old-render' })
  redoNestedComposition(); await tick(); expect(document.querySelector('input[aria-label="Скалярный ключ 450: значение"]')).not.toBeNull()
})
it('edits time in local seconds while retaining authored clock and key identities', async () => {
  component = mount(Inspector, { target: document.body }); await tick()
  const input = document.querySelector<HTMLInputElement>('input[aria-label="Скалярный ключ 1250: время"]')!
  input.value = '0.6'; input.dispatchEvent(new Event('change', { bubbles: true }))
  await vi.waitFor(() => expect(nestedHistory.past).toHaveLength(1)); await tick()
  expect(document.querySelector<HTMLInputElement>('input[aria-label="Скалярный ключ 1450: время"]')!.value).toBe('0.6')
  const track = nestedComposition.document!.compositions.root!.content.tracks[0]!
  const clip = track.clips[0]!
  expect('transform' in clip && clip.transform.x).toMatchObject({ track: { sampleClock: { rateNum: 2, rateDen: 1, offsetNum: 250000 } } })
})
it('keeps the original document if validation fails', async () => {
  vi.mocked(evaluateNestedFrame).mockRejectedValueOnce(new Error('invalid curve'))
  const before = nestedComposition.document
  component = mount(Inspector, { target: document.body }); await tick(); button('Добавить скалярный ключ по курсору').click()
  await vi.waitFor(() => expect(nestedHistory.error).toBe('invalid curve'))
  expect(nestedComposition.document).toBe(before); expect(nestedHistory.past).toHaveLength(0)
})
it('disables changes on a locked track', async () => {
  Object.assign(nestedComposition.document!.compositions.root!.content.tracks[0]!, { locked: true })
  component = mount(Inspector, { target: document.body }); await tick()
  expect(button('Добавить скалярный ключ по курсору').closest('fieldset')!.disabled).toBe(true)
  button('Добавить скалярный ключ по курсору').click(); await tick()
  expect(evaluateNestedFrame).not.toHaveBeenCalled(); expect(nestedHistory.past).toHaveLength(0)
})

it('rejects a pending edit when the selected channel changes during validation', async () => {
  let finish!: (value: { entries: [] }) => void
  vi.mocked(evaluateNestedFrame).mockImplementationOnce(() => new Promise(resolve => { finish = resolve }))
  const before = nestedComposition.document
  component = mount(Inspector, { target: document.body }); await tick()
  button('Добавить скалярный ключ по курсору').click()
  await vi.waitFor(() => expect(finish).toBeDefined())
  const channel = document.querySelector<HTMLSelectElement>('select[aria-label="Канал скалярных ключей"]')!
  channel.value = 'opacity'; channel.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  finish({ entries: [] })
  await vi.waitFor(() => expect(nestedHistory.busy).toBe(false))
  expect(nestedHistory.error).toBe('Выбранный канал изменился')
  expect(nestedComposition.document).toBe(before); expect(nestedHistory.past).toHaveLength(0)
})

it('does not commit if an agent acquires history ownership during validation', async () => {
  let finish!: (value: { entries: [] }) => void
  vi.mocked(evaluateNestedFrame).mockImplementationOnce(() => new Promise(resolve => { finish = resolve }))
  const before = nestedComposition.document
  component = mount(Inspector, { target: document.body }); await tick()
  button('Добавить скалярный ключ по курсору').click()
  await vi.waitFor(() => expect(finish).toBeDefined())
  agentSession.lock = { owner: 'agent', reason: 'editing', expiresAt: Date.now() + 60000 }
  finish({ entries: [] })
  await vi.waitFor(() => expect(nestedHistory.busy).toBe(false))
  expect(nestedHistory.error).toContain('История занята агентом')
  expect(nestedComposition.document).toBe(before); expect(nestedHistory.past).toHaveLength(0)
})

function xCurve() {
  return readNativeKeyframeValue(nestedComposition.document!, 'root', { clipId: 'title', property: 'x', envelope: 'visual' })
}
function tangentInput(label: string, value: string) {
  const input = document.querySelector<HTMLInputElement | HTMLSelectElement>(`[aria-label="Скалярный ключ 250 tangent ${label}"]`)!
  input.value = value; input.dispatchEvent(new Event('change', { bubbles: true }))
}
it('changes a clocked native curve through custom tangent controls and restores it with Undo/Redo', async () => {
  const before = JSON.stringify(nestedComposition.document)
  const originalSample = sampleAnimatableValue(xCurve(), 250000)
  component = mount(Inspector, { target: document.body }); await tick()
  tangentInput('outgoing x', '0.8')
  await vi.waitFor(() => expect(nestedHistory.past).toHaveLength(1)); await tick()
  const changed = xCurve()
  expect(changed).toMatchObject({ track: { timeBase: 1000, sampleClock: { rateNum: 2, rateDen: 1, offsetNum: 250000 }, keyframes: [{ tick: 250, value: 0, interpolation: { kind: 'bezier' }, tangents: { outgoing: { x: 0.8, mode: 'free' } } }, { tick: 1250, value: 10 }] } })
  expect(sampleAnimatableValue(changed, 250000)).toBeLessThan(originalSample)
  expect(nestedComposition.renderedOutputs).toEqual({}); expect(evaluateNestedFrame).toHaveBeenCalledOnce()
  const oracle = process.env.NATIVE_SCALAR_TANGENT_ORACLE
  if(oracle)writeFileSync(oracle,JSON.stringify({schemaVersion:1,composition:nestedComposition.document!.compositions.root!.content,samples:[0,1,10000,123457,250000,400000,500000,700000].map(localUs=>({localUs,value:sampleAnimatableValue(changed,localUs)}))}))
  const serialized = JSON.stringify(nestedComposition.document)
  undoNestedComposition(); expect(JSON.stringify(nestedComposition.document)).toBe(before)
  redoNestedComposition(); expect(JSON.stringify(nestedComposition.document)).toBe(serialized)
})
it('retains Auto/Free solved shapes and edits tangent continuity without changing clock coordinates', async () => {
  component = mount(Inspector, { target: document.body }); await tick()
  tangentInput('mode', 'auto')
  await vi.waitFor(() => expect(nestedHistory.past).toHaveLength(1)); await tick()
  expect(xCurve()).toMatchObject({ track: { keyframes: [{ tangents: { incoming: { mode: 'auto' }, outgoing: { mode: 'auto' }, continuity: 'smooth' } }, {}] } })
  const samples = [0, 10000, 123457, 250000, 400000, 500000].map(time => sampleAnimatableValue(xCurve(), time))
  tangentInput('mode', 'free')
  await vi.waitFor(() => expect(nestedHistory.past).toHaveLength(2)); await tick()
  expect([0, 10000, 123457, 250000, 400000, 500000].map(time => sampleAnimatableValue(xCurve(), time))).toEqual(samples)
  tangentInput('continuity', 'broken')
  await vi.waitFor(() => expect(nestedHistory.past).toHaveLength(3))
  expect(xCurve()).toMatchObject({ track: { sampleClock: { rateNum: 2, rateDen: 1, offsetNum: 250000 }, keyframes: [{ tick: 250, tangents: { continuity: 'broken' } }, { tick: 1250 }] } })
})
it('rejects invalid custom tangent coordinates before validation or history mutation', async () => {
  const before = JSON.stringify(nestedComposition.document)
  component = mount(Inspector, { target: document.body }); await tick()
  tangentInput('outgoing x', '1.5'); await tick()
  expect(document.querySelector('[role="alert"]')?.textContent).toContain('Invalid')
  expect(evaluateNestedFrame).not.toHaveBeenCalled(); expect(nestedHistory.past).toHaveLength(0)
  expect(JSON.stringify(nestedComposition.document)).toBe(before)
})

function prepareBezierCurve() {
  const value = xCurve()
  if(value.mode !== 'keyframes')throw new Error('Expected curve')
  Object.assign(value.track, { interpolation: { kind: 'bezier', p1: [0.25, 0.1], p2: [0.75, 0.9] } })
}
function graphHandle(point: 'p1' | 'p2') {
  const handle = document.querySelector<SVGCircleElement>(`[aria-label="Переместить Bézier ${point}"]`)!
  const svg = handle.ownerSVGElement!
  svg.getBoundingClientRect = () => new DOMRect(0, 0, 100, 100)
  handle.setPointerCapture = () => {}
  handle.hasPointerCapture = () => false
  const x = Number(handle.getAttribute('cx')), y = Number(handle.getAttribute('cy'))
  handle.dispatchEvent(new MouseEvent('pointerdown', { bubbles: true, button: 0, clientX: x, clientY: y }))
  return { handle, svg, x, y }
}
it.each(['p1', 'p2'] as const)('drags graph %s with an isolated preview and one native Undo entry', async point => {
  prepareBezierCurve(); const before = JSON.stringify(nestedComposition.document)
  component = mount(Inspector, { target: document.body }); await tick()
  const {handle, svg, x, y} = graphHandle(point)
  const points = svg.querySelector('polyline')!.getAttribute('points')
  svg.dispatchEvent(new MouseEvent('pointermove', { bubbles: true, clientX: x + 8, clientY: y - 8 })); await tick()
  expect(svg.querySelector('polyline')!.getAttribute('points')).not.toBe(points)
  expect(Number(handle.getAttribute('cx'))).toBeCloseTo(x + 8, 8)
  expect(JSON.stringify(nestedComposition.document)).toBe(before)
  expect(nestedHistory.past).toHaveLength(0); expect(evaluateNestedFrame).not.toHaveBeenCalled()
  svg.dispatchEvent(new MouseEvent('pointerup', { bubbles: true, clientX: x + 8, clientY: y - 8 }))
  await vi.waitFor(() => expect(nestedHistory.past).toHaveLength(1)); await tick()
  const value = xCurve(); if(value.mode !== 'keyframes')throw new Error('Expected curve')
  const owner = point === 'p1' ? 0 : 1, side = point === 'p1' ? 'outgoing' : 'incoming'
  expect(value.track.keyframes[owner]!.tangents![side].x).toBeCloseTo(point === 'p1' ? 0.35 : 0.85, 12)
  expect(value.track.keyframes[owner]!.tangents![side].y).toBeCloseTo(point === 'p1' ? 0.2 : 1, 12)
  expect(value.track.sampleClock).toEqual({rateNum:2,rateDen:1,offsetNum:250000})
  const oracle = process.env[point === 'p1' ? 'NATIVE_SCALAR_GRAPH_P1_ORACLE' : 'NATIVE_SCALAR_GRAPH_P2_ORACLE']
  if(oracle)writeFileSync(oracle,JSON.stringify({schemaVersion:1,composition:nestedComposition.document!.compositions.root!.content,samples:[0,1,10000,123457,250000,400000,500000,700000].map(localUs=>({localUs,value:sampleAnimatableValue(value,localUs)}))}))
  undoNestedComposition(); expect(JSON.stringify(nestedComposition.document)).toBe(before)
})
it.each(['escape', 'pointercancel', 'capture', 'channel', 'project', 'lock'] as const)('cancels a graph preview on %s without a native write', async kind => {
  prepareBezierCurve(); const before = JSON.stringify(nestedComposition.document)
  component = mount(Inspector, { target: document.body }); await tick()
  const {handle, svg, x, y} = graphHandle('p1')
  svg.dispatchEvent(new MouseEvent('pointermove', { bubbles: true, clientX: x + 8, clientY: y - 8 })); await tick()
  if(kind === 'escape')window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
  if(kind === 'pointercancel')svg.dispatchEvent(new MouseEvent('pointercancel', { bubbles: true }))
  if(kind === 'capture')handle.dispatchEvent(new Event('lostpointercapture'))
  if(kind === 'channel'){
    const select = document.querySelector<HTMLSelectElement>('[aria-label="Канал скалярных ключей"]')!
    select.value = 'opacity'; select.dispatchEvent(new Event('change', { bubbles: true }))
  }
  if(kind === 'project')nestedComposition.document = JSON.parse(before)
  if(kind === 'lock')agentSession.lock = {owner:'other',reason:'editing',expiresAt:Date.now()+60000}
  await tick()
  svg.dispatchEvent(new MouseEvent('pointerup', { bubbles: true, clientX: x + 8, clientY: y - 8 })); await tick()
  expect(JSON.stringify(nestedComposition.document)).toBe(before)
  expect(nestedHistory.past).toHaveLength(0); expect(evaluateNestedFrame).not.toHaveBeenCalled()
  expect(document.querySelector('[role="status"]')).toBeNull()
})
it('edits a native graph handle with keyboard steps and ignores clicks without movement', async () => {
  prepareBezierCurve()
  component = mount(Inspector, { target: document.body }); await tick()
  const {handle, svg, x, y} = graphHandle('p1')
  svg.dispatchEvent(new MouseEvent('pointerup', { bubbles: true, clientX: x, clientY: y })); await tick()
  expect(nestedHistory.past).toHaveLength(0); expect(evaluateNestedFrame).not.toHaveBeenCalled()
  handle.dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, key: 'ArrowRight' }))
  await vi.waitFor(() => expect(nestedHistory.past).toHaveLength(1))
  expect(xCurve()).toMatchObject({track:{keyframes:[{tangents:{outgoing:{x:0.26,y:0.1,mode:'free'}}},{}]}})
})

it('renders coalesced tangent drafts in the native canvas and drops a late cancelled frame', async () => {
  prepareBezierCurve(); authState.token='preview-token'
  const base=nestedComposition.document!,before=JSON.stringify(base)
  const pending:Array<(frame:NativeNestedPreviewFrame)=>void>=[],put=vi.fn()
  vi.mocked(renderNestedPreview).mockImplementation(()=>new Promise(resolve=>pending.push(resolve)))
  vi.spyOn(HTMLCanvasElement.prototype,'getContext').mockReturnValue({clearRect:vi.fn(),createImageData:()=>({data:new Uint8ClampedArray(4)}),putImageData:put} as unknown as CanvasRenderingContext2D)
  component=mount(Inspector,{target:document.body})
  const renderer=mount(NestedCompositionPreview,{target:document.body,props:{project:base,timeUs:1100000,renderedOutputs:{root:'old-render'},token:'preview-token',compositionId:'root',effectDraft:true}})
  const frame=(value:number)=>({width:1,height:1,rgba:new Uint8ClampedArray([value,0,0,255])})
  try{
    await tick();await vi.waitFor(()=>expect(pending).toHaveLength(1))
    const {svg,x,y}=graphHandle('p1')
    svg.dispatchEvent(new MouseEvent('pointermove',{bubbles:true,clientX:x+8,clientY:y-8}));await tick()
    svg.dispatchEvent(new MouseEvent('pointermove',{bubbles:true,clientX:x+16,clientY:y-16}));await tick()
    expect(pending).toHaveLength(1);expect(nestedEffectPreview.value?.base).toBe(base)
    expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedHistory.past).toHaveLength(0)
    pending[0]!(frame(1));await vi.waitFor(()=>expect(pending).toHaveLength(2))
    expect(put).not.toHaveBeenCalled();const request=vi.mocked(renderNestedPreview).mock.calls[1]!
    expect(request[0]).toMatchObject({compositions:{root:{content:{tracks:[{clips:[{transform:{x:{track:{sampleClock:{rateNum:2,rateDen:1,offsetNum:250000},keyframes:[{tangents:{outgoing:{x:0.45,y:0.3}}},{}]}}}}]}]}}}})
    expect(request[2]).toEqual({});expect(request[3]).toBe('preview-token')
    svg.dispatchEvent(new MouseEvent('pointermove',{bubbles:true,clientX:x+24,clientY:y-24}));await tick()
    pending[1]!(frame(10));await vi.waitFor(()=>expect(pending).toHaveLength(3));expect(put.mock.calls[0]![0].data[0]).toBe(10)
    window.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape'}));await tick()
    expect(nestedEffectPreview.value).toBeNull();pending[2]!(frame(20))
    await vi.waitFor(()=>expect(pending).toHaveLength(4));expect(put).toHaveBeenCalledTimes(1)
    expect(vi.mocked(renderNestedPreview).mock.calls[3]![0]).toBe(base);expect(vi.mocked(renderNestedPreview).mock.calls[3]![2]).toEqual({root:'old-render'})
    pending[3]!(frame(30));await vi.waitFor(()=>expect(put).toHaveBeenCalledTimes(2))
    expect(put.mock.calls[1]![0].data[0]).toBe(30);expect(JSON.stringify(nestedComposition.document)).toBe(before)
    expect(nestedHistory.past).toHaveLength(0);expect(evaluateNestedFrame).not.toHaveBeenCalled()
  }finally{await unmount(renderer)}
})
it.each([false,true])('retains the graph draft until native validation settles and then clears it: refused=%s',async refused=>{
  prepareBezierCurve();authState.token='preview-token';const before=JSON.stringify(nestedComposition.document)
  let finish!:(value:{entries:[]})=>void,reject!:(reason:Error)=>void
  vi.mocked(evaluateNestedFrame).mockImplementationOnce(()=>new Promise((resolve,fail)=>{finish=resolve;reject=fail}))
  component=mount(Inspector,{target:document.body});await tick()
  const {svg,x,y}=graphHandle('p1')
  svg.dispatchEvent(new MouseEvent('pointermove',{bubbles:true,clientX:x+8,clientY:y-8}));await tick()
  const draft=nestedEffectPreview.value;expect(draft).not.toBeNull()
  svg.dispatchEvent(new MouseEvent('pointerup',{bubbles:true,clientX:x+8,clientY:y-8}))
  await vi.waitFor(()=>expect(finish).toBeDefined());await tick()
  expect(nestedEffectPreview.value?.gesture).toBe(draft!.gesture)
  expect(JSON.stringify(nestedComposition.document)).toBe(before)
  if(refused)reject(new Error('native curve refused'));else finish({entries:[]})
  await vi.waitFor(()=>expect(nestedHistory.busy).toBe(false));await tick()
  expect(nestedEffectPreview.value).toBeNull();expect(nestedHistory.past).toHaveLength(refused?0:1)
  if(refused)expect(JSON.stringify(nestedComposition.document)).toBe(before)
})
it.each(['clock','session'] as const)('discards a live graph draft when the %s changes',async kind=>{
  prepareBezierCurve();authState.token='preview-token';const before=JSON.stringify(nestedComposition.document)
  component=mount(Inspector,{target:document.body});await tick()
  const {svg,x,y}=graphHandle('p1')
  svg.dispatchEvent(new MouseEvent('pointermove',{bubbles:true,clientX:x+8,clientY:y-8}));await tick()
  expect(nestedEffectPreview.value).not.toBeNull()
  if(kind==='clock')nestedComposition.timeUs=1200000;else authState.token='another-session'
  await tick();expect(nestedEffectPreview.value).toBeNull()
  svg.dispatchEvent(new MouseEvent('pointerup',{bubbles:true,clientX:x+8,clientY:y-8}));await tick()
  expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(evaluateNestedFrame).not.toHaveBeenCalled()
})

it.each(['channel','session'] as const)('clears a pending graph preview and refuses its edit after a %s change',async kind=>{
  prepareBezierCurve();authState.token='preview-token';const before=JSON.stringify(nestedComposition.document)
  let finish!:(value:{entries:[]})=>void
  vi.mocked(evaluateNestedFrame).mockImplementationOnce(()=>new Promise(resolve=>{finish=resolve}))
  component=mount(Inspector,{target:document.body});await tick()
  const {svg,x,y}=graphHandle('p1')
  svg.dispatchEvent(new MouseEvent('pointermove',{bubbles:true,clientX:x+8,clientY:y-8}));await tick()
  svg.dispatchEvent(new MouseEvent('pointerup',{bubbles:true,clientX:x+8,clientY:y-8}))
  await vi.waitFor(()=>expect(finish).toBeDefined());expect(nestedEffectPreview.value).not.toBeNull()
  if(kind==='channel'){
    const select=document.querySelector<HTMLSelectElement>('[aria-label="Канал скалярных ключей"]')!
    select.value='opacity';select.dispatchEvent(new Event('change',{bubbles:true}))
  }else authState.token='new-session'
  await tick();expect(nestedEffectPreview.value).toBeNull()
  finish({entries:[]});await vi.waitFor(()=>expect(nestedHistory.busy).toBe(false))
  expect(nestedHistory.past).toHaveLength(0);expect(JSON.stringify(nestedComposition.document)).toBe(before)
})

function preparePathProgress(bezier=false) {
  const project=nestedComposition.document!,clip=project.compositions.root!.content.tracks[0]!.clips[0]!
  if(!('transform' in clip))throw new Error('Expected visual clip')
  const original=xCurve();if(original.mode!=='keyframes')throw new Error('Expected keyed fixture')
  const progress={...original,track:{...original.track,interpolation:bezier?{kind:'bezier' as const,p1:[0.25,0.1] as const,p2:[0.75,0.9] as const}:'linear' as const,keyframes:original.track.keyframes.map(key=>({...key,value:key.value/10}))}}
  const node=(id:string,x:number)=>({id,point:{x,y:15},inHandle:{x:0,y:0},outHandle:{x:0,y:0},segment:'line' as const,tangentMode:'corner' as const})
  Object.assign(clip.transform,{position:{mode:'path',path:{nodes:[node('start',5),node('end',45)]},progress}})
  return JSON.stringify(clip.transform.x)
}
function shadowPathX(){const clip=nestedComposition.document!.compositions.root!.content.tracks[0]!.clips[0]!;if(!('transform' in clip))throw new Error('Visual clip required');return JSON.stringify(clip.transform.x)}
function progressCurve(){return readNativeKeyframeValue(nestedComposition.document!,'root',{clipId:'title',property:'progress',envelope:'path'})}
it('exposes active path progress, inserts at its clocked cursor and retains shadow XY animation',async()=>{
  const shadow=preparePathProgress(),before=JSON.stringify(nestedComposition.document)
  component=mount(Inspector,{target:document.body});await tick()
  const channels=document.querySelector<HTMLSelectElement>('[aria-label="Канал скалярных ключей"]')!
  expect(channels.value).toBe('progress');expect([...channels.options].map(option=>option.value)).not.toContain('x')
  expect([...channels.options].map(option=>option.value)).not.toContain('y')
  button('Добавить скалярный ключ по курсору').click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1))
  expect(progressCurve()).toMatchObject({track:{sampleClock:{rateNum:2,rateDen:1,offsetNum:250000},keyframes:[{tick:250,value:0},{tick:450,value:0.2},{tick:1250,value:1}]}})
  expect(shadowPathX()).toBe(shadow)
  undoNestedComposition();expect(JSON.stringify(nestedComposition.document)).toBe(before)
})
it('edits path progress through graph handles, publishes the active preview and keeps clocked authoring',async()=>{
  const shadow=preparePathProgress(true);authState.token='path-preview-token'
  const before=JSON.stringify(nestedComposition.document)
  component=mount(Inspector,{target:document.body});await tick()
  const {svg,x,y}=graphHandle('p1')
  svg.dispatchEvent(new MouseEvent('pointermove',{bubbles:true,clientX:x+44,clientY:y}));await tick()
  expect(nestedEffectPreview.value?.project).toMatchObject({compositions:{root:{content:{tracks:[{clips:[{transform:{position:{mode:'path',progress:{track:{keyframes:[{tangents:{outgoing:{x:0.8,y:0.1}}},{}]}}}}}]}]}}}})
  expect(JSON.stringify(nestedComposition.document)).toBe(before)
  svg.dispatchEvent(new MouseEvent('pointerup',{bubbles:true,clientX:x+44,clientY:y}))
  await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
  expect(progressCurve()).toMatchObject({track:{sampleClock:{rateNum:2,rateDen:1,offsetNum:250000},keyframes:[{tick:250,tangents:{outgoing:{x:0.8,y:0.1}}},{tick:1250}]}})
  expect(shadowPathX()).toBe(shadow)
  const oracle=process.env.NATIVE_PATH_PROGRESS_ORACLE
  if(oracle)writeFileSync(oracle,JSON.stringify({schemaVersion:1,composition:nestedComposition.document!.compositions.root!.content,samples:[0,1,10000,123457,250000,400000,500000,700000].map(localUs=>({localUs,value:sampleAnimatableValue(progressCurve(),localUs)}))}))
  undoNestedComposition();expect(JSON.stringify(nestedComposition.document)).toBe(before)
})

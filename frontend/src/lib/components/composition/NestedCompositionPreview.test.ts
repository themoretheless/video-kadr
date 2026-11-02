// @ts-expect-error Tests need Svelte's client mount under Vitest SSR resolution.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, expect, it, vi } from 'vitest'
import { renderNestedPreview, type NativeNestedProject, type NativeNestedPreviewFrame } from '$lib/api'
import NestedCompositionPreview from './NestedCompositionPreview.svelte'
vi.mock('$lib/api', () => ({ renderNestedPreview: vi.fn() }))
afterEach(() => { vi.restoreAllMocks(); vi.clearAllMocks(); document.body.innerHTML = '' })
const project: NativeNestedProject = { schemaVersion: 1, root: 'root', compositions: {} }

it('puts native RGBA into canvas and aborts the request on disposal', async () => {
  const putImageData = vi.fn()
  vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue({ clearRect: vi.fn(), createImageData: () => ({ data: new Uint8ClampedArray(4) }), putImageData } as unknown as CanvasRenderingContext2D)
  vi.mocked(renderNestedPreview).mockResolvedValue({ width: 1, height: 1, rgba: new Uint8ClampedArray([0,0,255,128]) })
  const component = mount(NestedCompositionPreview, { target: document.body, props: { project, timeUs: 400000, renderedOutputs: { child: 'movie' }, token: 'session', compositionId: 'child' } })
  await tick()
  await vi.waitFor(() => expect(putImageData).toHaveBeenCalled())
  expect(putImageData.mock.calls[0]![0].data).toEqual(new Uint8ClampedArray([0,0,255,128]))
  const signal = vi.mocked(renderNestedPreview).mock.calls[0]![4]!
  expect(vi.mocked(renderNestedPreview).mock.calls[0]![5]).toBe('child')
  await unmount(component)
  expect(signal.aborted).toBe(true)
})

it('does not draw a response arriving after unmount', async () => {
  const putImageData = vi.fn()
  vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue({ clearRect: vi.fn(), createImageData: () => ({ data: new Uint8ClampedArray(4) }), putImageData } as unknown as CanvasRenderingContext2D)
  let resolve!: (frame: NativeNestedPreviewFrame) => void
  vi.mocked(renderNestedPreview).mockReturnValue(new Promise((done) => { resolve = done }))
  const component = mount(NestedCompositionPreview, { target: document.body, props: { project, timeUs: 0, renderedOutputs: {}, token: 'session' } })
  await tick()
  await unmount(component)
  resolve({ width: 1, height: 1, rgba: new Uint8ClampedArray(4) })
  await tick()
  expect(putImageData).not.toHaveBeenCalled()
})

it('coalesces pointer drafts, renders intermediate frames, and refuses a late cancelled draft',async()=>{
 const {nestedEffectPreview,clearEffectPreview}=await import('$lib/state/nestedEffectPreview.svelte');const owner=Symbol(),gesture={};const pending:Array<(frame:NativeNestedPreviewFrame)=>void>=[];const put=vi.fn();vi.spyOn(HTMLCanvasElement.prototype,'getContext').mockReturnValue({clearRect:vi.fn(),createImageData:()=>({data:new Uint8ClampedArray(4)}),putImageData:put}as unknown as CanvasRenderingContext2D);vi.mocked(renderNestedPreview).mockImplementation(()=>new Promise(done=>pending.push(done)))
 const component=mount(NestedCompositionPreview,{target:document.body,props:{project,timeUs:0,renderedOutputs:{child:'old-movie'},token:'first',effectDraft:true}})
 const frame=(v:number)=>({width:1,height:1,rgba:new Uint8ClampedArray([v,0,0,255])});const publish=(candidate:NativeNestedProject)=>{nestedEffectPreview.value={owner,gesture,base:project,project:candidate,timeUs:0,compositionId:'root',token:'first'}}
 try{await tick();await vi.waitFor(()=>expect(pending).toHaveLength(1));const a={...project},b={...project},c={...project};publish(a);await tick();publish(b);await tick();expect(pending).toHaveLength(1);expect(vi.mocked(renderNestedPreview).mock.calls[0]![4]!.aborted).toBe(false);pending[0]!(frame(1));await vi.waitFor(()=>expect(pending).toHaveLength(2));expect(put).not.toHaveBeenCalled();expect(vi.mocked(renderNestedPreview).mock.calls[1]![0]).toBe(b);expect(vi.mocked(renderNestedPreview).mock.calls[1]![2]).toEqual({});publish(c);await tick();pending[1]!(frame(10));await vi.waitFor(()=>expect(pending).toHaveLength(3));expect(put.mock.calls[0]![0].data[0]).toBe(10);clearEffectPreview(owner,gesture);await tick();pending[2]!(frame(20));await vi.waitFor(()=>expect(pending).toHaveLength(4));expect(put).toHaveBeenCalledTimes(1);pending[3]!(frame(30));await vi.waitFor(()=>expect(put).toHaveBeenCalledTimes(2));expect(put.mock.calls[1]![0].data[0]).toBe(30);expect(vi.mocked(renderNestedPreview).mock.calls[3]![2]).toEqual({child:'old-movie'})}finally{await unmount(component);clearEffectPreview(owner)}
})
it('keeps the reference view on the committed document',async()=>{
 const {nestedEffectPreview,clearEffectPreview}=await import('$lib/state/nestedEffectPreview.svelte');const owner=Symbol();vi.spyOn(HTMLCanvasElement.prototype,'getContext').mockReturnValue({clearRect:vi.fn(),createImageData:()=>({data:new Uint8ClampedArray(4)}),putImageData:vi.fn()}as unknown as CanvasRenderingContext2D);vi.mocked(renderNestedPreview).mockResolvedValue({width:1,height:1,rgba:new Uint8ClampedArray(4)});const component=mount(NestedCompositionPreview,{target:document.body,props:{project,timeUs:0,renderedOutputs:{},token:'first'}})
 try{await tick();await vi.waitFor(()=>expect(renderNestedPreview).toHaveBeenCalledTimes(1));nestedEffectPreview.value={owner,gesture:{},base:project,project:{...project},timeUs:0,compositionId:'root',token:'first'};await tick();expect(renderNestedPreview).toHaveBeenCalledTimes(1)}finally{await unmount(component);clearEffectPreview(owner)}
})
it('aborts and clears private-session frames before accepting a new account',async()=>{
 const {default:Harness}=await import('./fixtures/EffectPreviewHarness.svelte');const put=vi.fn(),clear=vi.fn();const pending:Array<(frame:NativeNestedPreviewFrame)=>void>=[];vi.spyOn(HTMLCanvasElement.prototype,'getContext').mockReturnValue({clearRect:clear,createImageData:()=>({data:new Uint8ClampedArray(4)}),putImageData:put}as unknown as CanvasRenderingContext2D);vi.mocked(renderNestedPreview).mockImplementation(()=>new Promise(done=>pending.push(done)));const component=mount(Harness,{target:document.body,props:{base:project}})
 try{await tick();await vi.waitFor(()=>expect(pending).toHaveLength(1));const signal=vi.mocked(renderNestedPreview).mock.calls[0]![4]!;component.changeSession();await tick();await vi.waitFor(()=>expect(pending).toHaveLength(2));expect(signal.aborted).toBe(true);expect(clear).toHaveBeenCalledTimes(2);pending[0]!({width:1,height:1,rgba:new Uint8ClampedArray([100,0,0,255])});await tick();expect(put).not.toHaveBeenCalled();pending[1]!({width:1,height:1,rgba:new Uint8ClampedArray([200,0,0,255])});await vi.waitFor(()=>expect(put).toHaveBeenCalledTimes(1));expect(put.mock.calls[0]![0].data[0]).toBe(200);expect(vi.mocked(renderNestedPreview).mock.calls[1]![3]).toBe('second')}finally{await unmount(component)}
})

it('coalesces continuous playback frames without starvation and cancels explicit seeks', async () => {
  const { default: Harness } = await import('./fixtures/EffectPreviewHarness.svelte')
  const put = vi.fn()
  vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue({ clearRect: vi.fn(), createImageData: () => ({ data: new Uint8ClampedArray(4) }), putImageData: put } as unknown as CanvasRenderingContext2D)
  const pending: ((frame: NativeNestedPreviewFrame) => void)[] = []
  vi.mocked(renderNestedPreview).mockImplementation(() => new Promise(done => pending.push(done)))
  const component = mount(Harness, { target: document.body, props: { base: project } })
  const frame = (v: number) => ({ width: 1, height: 1, rgba: new Uint8ClampedArray([v, 0, 0, 255]) })
  try {
    await tick(); component.playTime(1000); await tick()
    const playingSignal = vi.mocked(renderNestedPreview).mock.calls[1][4]!
    component.playTime(2000); await tick(); component.playTime(3000); await tick()
    expect(pending).toHaveLength(2); expect(playingSignal.aborted).toBe(false)
    pending[1](frame(10)); await vi.waitFor(() => expect(pending).toHaveLength(3))
    expect(put).toHaveBeenCalledTimes(1)
    expect(vi.mocked(renderNestedPreview).mock.calls[2][1]).toBe(3000)
    const old = vi.mocked(renderNestedPreview).mock.calls[2][4]!
    component.seekTime(7000); await tick()
    expect(old.aborted).toBe(true); expect(pending).toHaveLength(4)
    pending[2](frame(20)); await tick(); expect(put).toHaveBeenCalledTimes(1)
    pending[3](frame(30)); await vi.waitFor(() => expect(put).toHaveBeenCalledTimes(2))
    expect(vi.mocked(renderNestedPreview).mock.calls[3][1]).toBe(7000)
    pending[0](frame(90)); await tick(); expect(put).toHaveBeenCalledTimes(2)
  } finally { await unmount(component) }
})

it('passes saved project identity to the shared preview for bound video masks',async()=>{
 vi.spyOn(HTMLCanvasElement.prototype,'getContext').mockReturnValue({clearRect:vi.fn(),createImageData:()=>({data:new Uint8ClampedArray(4)}),putImageData:vi.fn()} as unknown as CanvasRenderingContext2D)
 vi.mocked(renderNestedPreview).mockResolvedValue({width:1,height:1,rgba:new Uint8ClampedArray(4)})
 const savedProject={projectId:'saved',expectedRevision:9}
 const component=mount(NestedCompositionPreview,{target:document.body,props:{project,timeUs:0,renderedOutputs:{},token:'owner',savedProject}})
 try{await vi.waitFor(()=>expect(renderNestedPreview).toHaveBeenCalled());expect(vi.mocked(renderNestedPreview).mock.calls[0]![6]).toEqual(savedProject)}finally{await unmount(component)}
})

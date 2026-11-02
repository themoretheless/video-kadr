// @ts-expect-error Mounted component needs client entry.
import { mount,tick,unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { expect,it,vi } from 'vitest'
import * as api from '$lib/api'
import { addClip,addTrack,createComposition } from '$lib/composition/commands'
import { commitDocument,compositionState,resetCompositionForTests,undoComposition } from '$lib/state/composition.svelte'
import { positionConversionPreview } from '$lib/state/positionConversionPreview.svelte'
import type { TextClip } from '$lib/composition/types'
import Conversion from './CompositionPositionConversion.svelte'
it('calculates transient conversion automatically and applies in one Undo', async () => {
  resetCompositionForTests()
  const clip: TextClip = { id: 'c',kind: 'text',timelineStartTicks: 0,durationTicks: 1_000_000,text: 'Path',x: 5,y: 7,opacity: 1,style: { fontSizePx: 24,color: '#FFFFFFFF',align: 'center' } }
  let document = createComposition({ width: 128,height: 128,fps: 20,backgroundColor: '#000000' })
  document = addTrack(document,{ id: 't',kind: 'text',name: 'Text',hidden: false,locked: false,clips: [] })
  commitDocument(addClip(document,'t',clip))
  const before = JSON.stringify(compositionState.document),depth = compositionState.history.past.length
  const spy = vi.spyOn(api,'previewPositionConversion').mockResolvedValue({ nodes: [{ point: { x: 5,y: 7 },incoming: { x: 0,y: 0 },outgoing: { x: 0,y: 0 },cubic: false }],tracks: [{ keys: [{ t_us: 0,value: 0,in_: [2/3,2/3],out: [1/3,1/3],segment: { kind: 'Linear' } }],extrapolate: { before: 'Hold',after: 'Hold' } }],sampleCount: 21,nodeCount: 1,maxErrorPx: 0,checkCount: 81,withinTolerance: true,limit: null })
  const target = window.document.createElement('div');window.document.body.append(target)
  const component = mount(Conversion,{ target,props: { clip } })
  try {
    target.querySelector('button')!.click();await tick()
    await new Promise(resolve => setTimeout(resolve,200));await tick()
    const start = target.querySelector<HTMLInputElement>('[aria-label="Начальный кадр конвертации"]')!
    const end = target.querySelector<HTMLInputElement>('[aria-label="Конечный кадр конвертации"]')!
    const change = async (input: HTMLInputElement,value: string) => { input.value = value;input.dispatchEvent(new Event('input',{ bubbles: true }));await tick();await new Promise(resolve => setTimeout(resolve,200));await tick() }
    await change(end,'21')
    expect(target.textContent).toContain('Выберите диапазон кадров внутри клипа')
    expect(positionConversionPreview.document).toBeNull()
    expect(spy).toHaveBeenCalledOnce()
    await change(end,'15');await change(start,'5')
    expect(spy.mock.calls.at(-1)![0].options).toMatchObject({ startFrame: 5,endFrame: 15,everyFrames: 1 })
    expect(spy).toHaveBeenCalledTimes(3)
    expect(JSON.stringify(compositionState.document)).toBe(before)
    expect(positionConversionPreview.document).not.toBeNull()
    target.querySelector('button')!.click();await tick()
    expect(positionConversionPreview.document).toBeNull()
    expect(JSON.stringify(compositionState.document)).toBe(before)
    expect(compositionState.history.past).toHaveLength(depth)
    target.querySelector('button')!.click();await tick()
    await new Promise(resolve => setTimeout(resolve,200));await tick()
    const apply = Array.from(target.querySelectorAll('button')).find(button => button.textContent === 'Применить конвертацию')!
    expect(apply.disabled).toBe(false)
    apply.click();await tick()
    expect(compositionState.history.past).toHaveLength(depth+1)
    expect(compositionState.document.tracks[0]!.clips[0]).toMatchObject({ position: { mode: 'path',path: { nodes: [{ point: { x: 5,y: 7 } }] } } })
    undoComposition();expect(JSON.stringify(compositionState.document)).toBe(before)
  } finally { spy.mockRestore();await unmount(component);target.remove();resetCompositionForTests() }
  expect(positionConversionPreview.document).toBeNull()
})

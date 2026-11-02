// @ts-expect-error Mounted component needs client entry.
import { mount,tick,unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { expect,it,vi } from 'vitest'
import * as api from '$lib/api'
import { addClip,addTrack,createComposition } from '$lib/composition/commands'
import { commitDocument,compositionState,resetCompositionForTests,undoComposition } from '$lib/state/composition.svelte'
import type { TextClip,CompositionPathNode } from '$lib/composition/types'
import Overlay from './CompositionMotionPathOverlay.svelte'

it('maps canvas double click to native hit test and inserts in one Undo', async () => {
  resetCompositionForTests()
  const node = (id: string,x: number): CompositionPathNode => ({ id,point: { x,y: 0 },inHandle: { x: 0,y: 0 },outHandle: { x: 0,y: 0 },segment: 'line',tangentMode: 'corner' })
  const path = { nodes: [node('a',0),node('b',100)] }
  const clip: TextClip = { id: 'c',kind: 'text',timelineStartTicks: 0,durationTicks: 1_000_000,text: 'Path',x: 0,y: 0,opacity: 1,style: { fontSizePx: 24,color: '#FFFFFFFF',align: 'center' },position: { mode: 'path',path,progress: { mode: 'constant',value: 0.4 } } }
  let document = createComposition({ width: 200,height: 100,fps: 20,backgroundColor: '#000000' })
  document = addTrack(document,{ id: 't',kind: 'text',name: 'Text',hidden: false,locked: false,clips: [] })
  commitDocument(addClip(document,'t',clip))
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  const native = vi.spyOn(api,'evaluateMotionPath').mockResolvedValueOnce({ path,length: 100,points: [],nearest: { segment: 0,t: 0.5,distance: 0 } }).mockResolvedValueOnce({ path: { nodes: [node('a',0),node('new',50),node('b',100)] },length: 100,points: [] })
  const target = window.document.createElement('div');window.document.body.append(target)
  const component = mount(Overlay,{ target,props: { clip,width: 200,height: 100 } })
  try {
    const svg = target.querySelector('svg')!
    vi.spyOn(svg,'getBoundingClientRect').mockReturnValue({ x: 10,y: 20,left: 10,top: 20,right: 410,bottom: 220,width: 400,height: 200,toJSON: () => ({}) })
    expect(target.querySelector('path')!.getAttribute('d')).toBe('M 0 0 L 100 0')
    svg.dispatchEvent(new MouseEvent('dblclick',{ clientX: 310,clientY: 120,bubbles: true }))
    await tick();await tick();await tick()
    expect(native.mock.calls[0]![0]).toEqual({ operation: 'nearest',path,point: { x: 50,y: 0 } })
    expect(native.mock.calls[1]![0]).toMatchObject({ operation: 'insert',segment: 0,t: 0.5 })
    expect(compositionState.history.past).toHaveLength(depth+1)
    expect(compositionState.document.tracks[0]!.clips[0]).toMatchObject({ position: { path: { nodes: [node('a',0),node('new',50),node('b',100)] },progress: { value: 0.4 } } })
    undoComposition();expect(JSON.stringify(compositionState.document)).toBe(before)
    const pointer = (element: Element,type: string,x: number,y: number) => {
      const event = new Event(type,{ bubbles: true });Object.assign(event,{ pointerId: 3,button: 0,clientX: x,clientY: y });element.dispatchEvent(event)
    }
    const circle = target.querySelector('[aria-label="Узел траектории 1"]')!
    const calls = native.mock.calls.length
    pointer(circle,'pointerdown',210,120);pointer(svg,'pointermove',250,120)
    await tick()
    expect(JSON.stringify(compositionState.document)).toBe(before)
    expect(target.querySelector('path')!.getAttribute('d')).toBe('M 20 0 L 100 0')
    window.dispatchEvent(new KeyboardEvent('keydown',{ key: 'Escape',bubbles: true }))
    await tick()
    expect(native.mock.calls).toHaveLength(calls)
    expect(target.querySelector('path')!.getAttribute('d')).toBe('M 0 0 L 100 0')
    native.mockResolvedValueOnce({ path: { nodes: [node('a',20),node('b',100)] },length: 80,points: [] })
    pointer(circle,'pointerdown',210,120);pointer(svg,'pointermove',250,120);pointer(svg,'pointerup',250,120)
    await tick();await tick()
    expect(native.mock.calls.at(-1)?.[0]).toMatchObject({ operation: 'edit',index: 0,value: { x: 20,y: 0 } })
    expect(compositionState.history.past).toHaveLength(depth+1)
    undoComposition();expect(JSON.stringify(compositionState.document)).toBe(before)
    native.mockResolvedValueOnce({ path: { nodes: [{ ...node('a',0),outHandle: { x: 0,y: -10 } },node('b',100)] },length: 100,points: [] })
    target.querySelector('[aria-label="Ручка траектории 1 out_handle"]')!.dispatchEvent(new KeyboardEvent('keydown',{ key: 'ArrowUp',shiftKey: true,bubbles: true }))
    await tick();await tick()
    expect(native.mock.calls.at(-1)?.[0]).toMatchObject({ operation: 'edit',index: 0,part: 'out_handle',value: { x: 0,y: -10 } })
    undoComposition();expect(JSON.stringify(compositionState.document)).toBe(before)
    native.mockResolvedValueOnce({ path: { nodes: [node('a',10),node('b',100)] },length: 90,points: [] })
    target.querySelector('[aria-label="Узел траектории 1"]')!.dispatchEvent(new KeyboardEvent('keydown',{ key: 'ArrowRight',shiftKey: true,bubbles: true }))
    await tick();await tick()
    expect(native.mock.calls.at(-1)?.[0]).toEqual({ operation: 'edit',path,index: 0,part: 'point',value: { x: 10,y: 0 } })
    expect(compositionState.history.past).toHaveLength(depth+1)
    expect(compositionState.document.tracks[0]!.clips[0]).toMatchObject({ position: { progress: { value: 0.4 } } })
    undoComposition();expect(JSON.stringify(compositionState.document)).toBe(before)

  } finally { native.mockRestore();await unmount(component);target.remove();resetCompositionForTests() }
})

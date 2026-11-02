// @ts-expect-error Mounted component requires Svelte client entry under Vitest SSR conditions.
import { mount,tick,unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { expect,it,vi } from 'vitest'
import * as api from '$lib/api'
import { addClip,addTrack,createComposition } from '$lib/composition/commands'
import { compositionState,commitDocument,resetCompositionForTests,undoComposition } from '$lib/state/composition.svelte'
import type { TextClip } from '$lib/composition/types'
import Editor from './CompositionMotionPathEditor.svelte'

it('creates a persisted path and rejects native edits after a concurrent document commit', async () => {
  resetCompositionForTests()
  let document = createComposition({ width: 128,height: 128,fps: 20,backgroundColor: '#000000' })
  document = addTrack(document,{ id: 't',kind: 'text',name: 'Text',hidden: false,locked: false,clips: [] })
  const clip: TextClip = { id: 'c',kind: 'text',timelineStartTicks: 0,durationTicks: 1_000_000,text: 'Path',x: 10,y: 20,opacity: 1,style: { fontSizePx: 24,color: '#FFFFFFFF',align: 'center' } }
  document = addClip(document,'t',clip)
  commitDocument(document)
  const target = window.document.createElement('div')
  window.document.body.append(target)
  let component = mount(Editor,{ target,props: { clip } })
  let spy: ReturnType<typeof vi.spyOn> | undefined
  try {
    Array.from(target.querySelectorAll('button')).find(button => button.textContent === 'Создать траекторию')!.click()
    await tick()
    const authored = compositionState.document.tracks[0]!.clips[0] as TextClip
    expect(authored.position).toMatchObject({ mode: 'path',progress: { mode: 'constant',value: 0 },path: { nodes: [{ point: { x: 10,y: 20 } }] } })
    await unmount(component)
    component = mount(Editor,{ target,props: { clip: authored } })
    let complete!: (result: api.NativeMotionPathResponse) => void
    spy = vi.spyOn(api,'evaluateMotionPath').mockImplementation(() => new Promise(resolve => { complete = resolve }))
    const x = target.querySelector<HTMLInputElement>('[aria-label="Выходящая ручка узла 1 X"]')!
    x.value = '25';x.dispatchEvent(new Event('change',{ bubbles: true }))
    await tick()
    expect(spy).toHaveBeenCalledOnce()
    expect(spy.mock.calls[0]![0]).toMatchObject({ operation: 'edit',index: 0,part: 'out_handle',value: { x: 25,y: 0 } })
    commitDocument({ ...compositionState.document,canvas: { ...compositionState.document.canvas,backgroundColor: '#123456' } })
    if (authored.position?.mode !== 'path') throw new Error('path required')
    complete({ path: { nodes: authored.position.path.nodes.map(node => ({ ...node,point: { x: 25,y: 20 } })) },length: 0,points: [] })
    await tick();await tick()
    expect(compositionState.document.canvas.backgroundColor).toBe('#123456')
    expect((compositionState.document.tracks[0]!.clips[0] as TextClip).position).toEqual(authored.position)
    expect(compositionState.ui.message).toContain('Документ изменился')
  } finally { spy?.mockRestore();await unmount(component);target.remove();resetCompositionForTests() }
})


it('commits a keyboard progress curve edit once, restores exact Undo and disables locked tracks', async () => {
  resetCompositionForTests()
  let document = createComposition({ width: 128,height: 128,fps: 20,backgroundColor: '#000000' })
  document = addTrack(document,{ id: 't',kind: 'text',name: 'Text',hidden: false,locked: false,clips: [] })
  const clip: TextClip = { id: 'c',kind: 'text',timelineStartTicks: 0,durationTicks: 1_000_000,text: 'Path',x: 10,y: 20,opacity: 1,style: { fontSizePx: 24,color: '#FFFFFFFF',align: 'center' },position: { mode: 'path',path: { nodes: [{ id: 'node',point: { x: 10,y: 20 },inHandle: { x: 0,y: 0 },outHandle: { x: 0,y: 0 },segment: 'line',tangentMode: 'corner' }] },progress: { mode: 'keyframes',track: { timeBase: 1000,interpolation: { kind: 'bezier',p1: [0.25,0.1],p2: [0.75,0.9] },keyframes: [{ tick: 0,value: 0 },{ tick: 1000,value: 1 }] } } } }
  document = addClip(document,'t',clip)
  commitDocument(document)
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  const target = window.document.createElement('div')
  window.document.body.append(target)
  const component = mount(Editor,{ target,props: { clip } })
  try {
    const control = target.querySelector('[aria-label="Переместить Bézier p1"]')!
    control.dispatchEvent(new KeyboardEvent('keydown',{ key: 'ArrowRight',bubbles: true }))
    await tick()
    expect(compositionState.history.past).toHaveLength(depth+1)
    expect(compositionState.document.tracks[0]!.clips[0]).toMatchObject({ position: { path: { nodes: [{ point: { x: 10,y: 20 } }] },progress: { track: { interpolation: { p1: [0.26,0.1] } } } } })
    undoComposition()
    expect(JSON.stringify(compositionState.document)).toBe(before)
    target.querySelector<HTMLButtonElement>('[aria-label="Auto ключа progress 1"]')!.click()
    await tick()
    expect(compositionState.document.tracks[0]!.clips[0]).toMatchObject({ position: { progress: { track: { keyframes: [{ tangents: { outgoing: { mode: 'auto' } } },{}] } } } })
    undoComposition()
    const continuity = target.querySelector<HTMLSelectElement>('[aria-label="Связь ручек ключа progress 1"]')!
    continuity.value = 'smooth';continuity.dispatchEvent(new Event('change',{ bubbles: true }))
    await tick()
    expect(compositionState.document.tracks[0]!.clips[0]).toMatchObject({ position: { progress: { track: { keyframes: [{ tangents: { continuity: 'smooth' } },{}] } } } })
    undoComposition()
    expect(JSON.stringify(compositionState.document)).toBe(before)
    const handle = target.querySelector<HTMLInputElement>('[aria-label="Ручка progress 1 outgoing y"]')!
    handle.value = '-0.2';handle.dispatchEvent(new Event('change',{ bubbles: true }))
    await tick()
    expect(compositionState.document.tracks[0]!.clips[0]).toMatchObject({ position: { progress: { track: { keyframes: [{ tangents: { outgoing: { y: -0.2,mode: 'free' } } },{}] } } } })
    undoComposition()
    expect(JSON.stringify(compositionState.document)).toBe(before)
    const keyValue = target.querySelector<HTMLInputElement>('[aria-label="Значение ключа progress 2"]')!
    keyValue.value = '1.2';keyValue.dispatchEvent(new Event('change',{ bubbles: true }))
    await tick()
    expect(compositionState.document.tracks[0]!.clips[0]).toMatchObject({ position: { progress: { track: { keyframes: [{ value: 0 },{ tick: 1000,value: 1.2 }] } } } })
    undoComposition()
    const keyTime = target.querySelector<HTMLInputElement>('[aria-label="Время ключа progress 2"]')!
    keyTime.value = '0.75';keyTime.dispatchEvent(new Event('change',{ bubbles: true }))
    await tick()
    expect(compositionState.document.tracks[0]!.clips[0]).toMatchObject({ position: { progress: { track: { timeBase: 1000,keyframes: [{ tick: 0 },{ tick: 750 }] } } } })
    undoComposition()
    target.querySelector<HTMLButtonElement>('[aria-label="Удалить ключ progress 2"]')!.click()
    await tick()
    expect(compositionState.document.tracks[0]!.clips[0]).toMatchObject({ position: { progress: { track: { keyframes: [{ tick: 0,value: 0 }] } } } })
    undoComposition()
    expect(JSON.stringify(compositionState.document)).toBe(before)
    const numeric = target.querySelector<HTMLInputElement>('[aria-label="Bézier progress p2 Y"]')!
    numeric.value = '1.4';numeric.dispatchEvent(new Event('change',{ bubbles: true }))
    await tick()
    expect(compositionState.document.tracks[0]!.clips[0]).toMatchObject({ position: { progress: { track: { interpolation: { p2: [0.75,1.4] } } } } })
    expect(compositionState.history.past).toHaveLength(depth+1)
    undoComposition()
    expect(JSON.stringify(compositionState.document)).toBe(before)
    const svg = control.closest('svg')!
    const rect = vi.spyOn(svg,'getBoundingClientRect').mockReturnValue({ x: 0,y: 0,left: 0,top: 0,right: 100,bottom: 100,width: 100,height: 100,toJSON: () => ({}) })
    const pointer = (element: Element,type: string,x: number,y: number) => {
      const event = new Event(type,{ bubbles: true })
      Object.assign(event,{ pointerId: 7,button: 0,clientX: x,clientY: y })
      element.dispatchEvent(event)
    }
    pointer(control,'pointerdown',30,82)
    pointer(svg,'pointermove',50,50)
    await tick()
    expect(JSON.stringify(compositionState.document)).toBe(before)
    pointer(svg,'pointerup',50,50)
    await tick()
    expect(compositionState.history.past).toHaveLength(depth+1)
    expect(compositionState.document.tracks[0]!.clips[0]).toMatchObject({ position: { progress: { track: { interpolation: { p1: [0.5,0.5] } } } } })
    undoComposition()
    pointer(control,'pointerdown',30,82)
    pointer(svg,'pointermove',60,40)
    window.dispatchEvent(new KeyboardEvent('keydown',{ key: 'Escape',bubbles: true }))
    await tick()
    expect(JSON.stringify(compositionState.document)).toBe(before)
    expect(compositionState.history.past).toHaveLength(depth)
    rect.mockRestore()
    commitDocument({ ...compositionState.document,tracks: compositionState.document.tracks.map(track => ({ ...track,locked: true })) })
    await tick()
    const lockedBefore = JSON.stringify(compositionState.document)
    expect(control.getAttribute('aria-disabled')).toBe('true')
    control.dispatchEvent(new KeyboardEvent('keydown',{ key: 'ArrowUp',bubbles: true }))
    await tick()
    expect(JSON.stringify(compositionState.document)).toBe(lockedBefore)
  } finally { await unmount(component);target.remove();resetCompositionForTests() }
})

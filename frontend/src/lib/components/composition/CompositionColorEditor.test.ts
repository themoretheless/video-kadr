// @ts-expect-error Mounted component needs the Svelte client entry.
import { mount,tick,unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { expect,it,vi } from 'vitest'
import { addClip,addTrack,createComposition } from '$lib/composition/commands'
import { commitDocument,compositionState,resetCompositionForTests,undoComposition } from '$lib/state/composition.svelte'
import type { TextClip } from '$lib/composition/types'
import Editor from './CompositionColorEditor.svelte'
import { colorCurvePreview } from '$lib/state/colorCurvePreview.svelte'

it('authors and edits color keys through reachable controls with one Undo and locked-track refusal', async () => {
  resetCompositionForTests()
  const clip: TextClip = { id: 'c',kind: 'text',timelineStartTicks: 0,durationTicks: 1_000_000,text: 'Color',x: 0,y: 0,opacity: 1,style: { fontSizePx: 24,color: '#FF0000FF',align: 'center' } }
  const document = addClip(addTrack(createComposition({ width: 128,height: 128,fps: 20,backgroundColor: '#000000' }),{ id: 't',kind: 'text',name: 'Text',hidden: false,locked: false,clips: [] }),'t',clip)
  commitDocument(document)
  compositionState.transport.playheadTicks = 500_000
  const before = JSON.stringify(compositionState.document),depth = compositionState.history.past.length
  const target = window.document.createElement('div');window.document.body.append(target)
  const component = mount(Editor,{ target,props: { clip } })
  const change = async (label: string,value: string,event = 'change') => {
    const input = target.querySelector<HTMLInputElement>(`[aria-label="${label}"]`)!
    input.value = value;input.dispatchEvent(new Event(event,{ bubbles: true }));await tick()
  }
  try {
    await tick()
    await change('Цвет нового ключа','#00FF0080','input')
    target.querySelector('button')!.click();await tick()
    expect(compositionState.history.past).toHaveLength(depth+1)
    expect(compositionState.document.tracks[0]!.clips[0]).toMatchObject({ colorAnimation: { track: { keyframes: [{ tick: 0,value: '#FF0000FF' },{ tick: 500000,value: '#00FF0080' }] } } })
    undoComposition();await tick();expect(JSON.stringify(compositionState.document)).toBe(before)
    target.querySelector('button')!.click();await tick()
    const keyed = JSON.stringify(compositionState.document)
    target.querySelector<HTMLButtonElement>('[aria-label="Auto цветового ключа 0"]')!.click();await tick()
    expect(compositionState.document.tracks[0]!.clips[0]).toMatchObject({ colorAnimation: { track: { keyframes: [{ tangents: { outgoing: { x: 1/3,y: 1/3,mode: 'auto' },continuity: 'smooth' } },{ tick: 500000 }] } } })
    const auto = JSON.stringify(compositionState.document)
    target.querySelector<HTMLButtonElement>('[aria-label="Free цветового ключа 0"]')!.click();await tick()
    expect(compositionState.document.tracks[0]!.clips[0]).toMatchObject({ colorAnimation: { track: { keyframes: [{ tangents: { outgoing: { x: 1/3,y: 1/3,mode: 'free' } } },{ tick: 500000 }] } } })
    undoComposition();await tick();expect(JSON.stringify(compositionState.document)).toBe(auto)
    undoComposition();await tick();expect(JSON.stringify(compositionState.document)).toBe(keyed)
    await change('Связь ручек цвета 0','smooth')
    expect(compositionState.document.tracks[0]!.clips[0]).toMatchObject({ colorAnimation: { track: { keyframes: [{ tangents: { continuity: 'smooth' } },{ tick: 500000 }] } } })
    undoComposition();await tick();expect(JSON.stringify(compositionState.document)).toBe(keyed)
    await change('Ручка цвета 0 outgoing y','-0.2')
    expect(compositionState.document.tracks[0]!.clips[0]).toMatchObject({ colorAnimation: { track: { keyframes: [{ interpolation: { kind: 'bezier' },tangents: { outgoing: { x: 1/3,y: -0.2,mode: 'free' },incoming: { mode: 'free' } } },{ tick: 500000 }] } } })
    undoComposition();await tick();expect(JSON.stringify(compositionState.document)).toBe(keyed)
    await change('Кривая цветовой дорожки','ease_in_quad')
    expect(compositionState.document.tracks[0]!.clips[0]).toMatchObject({ colorAnimation: { track: { interpolation: { kind: 'bezier',p1: [1/3,0],p2: [2/3,1/3] },keyframes: [{ tangents: { outgoing: { x: 1/3,y: 0,mode: 'free' } } },{ tangents: { incoming: { x: 2/3,y: 1/3,mode: 'free' } } }] } } })
    const preset = JSON.stringify(compositionState.document)
    const graph = target.querySelector('[aria-label="График цветовой дорожки"]')!
    const handle = graph.querySelector('[aria-label="Переместить Bézier p1"]')!
    const svg = graph.querySelector('svg')!
    handle.dispatchEvent(new KeyboardEvent('keydown',{ key: 'ArrowRight',bubbles: true }));await tick()
    expect(JSON.stringify(compositionState.document)).not.toBe(preset)
    undoComposition();await tick();expect(JSON.stringify(compositionState.document)).toBe(preset)
    const rect = vi.spyOn(svg,'getBoundingClientRect').mockReturnValue({ x: 0,y: 0,left: 0,top: 0,right: 100,bottom: 100,width: 100,height: 100,toJSON: () => ({}) })
    const pointer = (element: Element,type: string,x: number,y: number) => {
      const event = new Event(type,{ bubbles: true });Object.assign(event,{ pointerId: 7,button: 0,clientX: x,clientY: y });element.dispatchEvent(event)
    }
    const gestureDepth = compositionState.history.past.length
    pointer(handle,'pointerdown',37,90);pointer(svg,'pointermove',50,50);await tick()
    expect(JSON.stringify(compositionState.document)).toBe(preset)
    expect(colorCurvePreview.document).not.toBeNull()
    expect(colorCurvePreview.document!.tracks[0]!.clips[0]).toMatchObject({ colorAnimation: { track: { interpolation: { p1: [0.5,0.5] } } } })
    pointer(svg,'pointerup',50,50);await tick()
    expect(colorCurvePreview.document).toBeNull()
    expect(compositionState.history.past).toHaveLength(gestureDepth+1)
    expect(compositionState.document.tracks[0]!.clips[0]).toMatchObject({ colorAnimation: { track: { interpolation: { p1: [0.5,0.5] } } } })
    undoComposition();await tick();expect(JSON.stringify(compositionState.document)).toBe(preset)
    pointer(handle,'pointerdown',37,90);pointer(svg,'pointermove',60,40)
    window.dispatchEvent(new KeyboardEvent('keydown',{ key: 'Escape',bubbles: true }));await tick()
    expect(JSON.stringify(compositionState.document)).toBe(preset)
    expect(compositionState.history.past).toHaveLength(gestureDepth)
    expect(colorCurvePreview.document).toBeNull()
    pointer(handle,'pointerdown',37,90);pointer(svg,'pointermove',60,40)
    commitDocument({ ...compositionState.document,canvas: { ...compositionState.document.canvas,backgroundColor: '#FFFFFF' } });await tick()
    const concurrent = JSON.stringify(compositionState.document)
    pointer(svg,'pointerup',60,40);await tick()
    expect(JSON.stringify(compositionState.document)).toBe(concurrent)
    expect(compositionState.ui.message).toContain('во время жеста')
    undoComposition();await tick();expect(JSON.stringify(compositionState.document)).toBe(preset)
    rect.mockRestore()
    await change('Кривая цвета p2Y','1.4')
    expect(compositionState.document.tracks[0]!.clips[0]).toMatchObject({ colorAnimation: { track: { interpolation: { p2: [2/3,1.4] },keyframes: [{ tick: 0 },{ tangents: { incoming: { y: 1.4 } } }] } } })
    undoComposition();await tick();expect(JSON.stringify(compositionState.document)).toBe(preset)
    undoComposition();await tick();expect(JSON.stringify(compositionState.document)).toBe(keyed)
    await change('Кривая цветового ключа 0','hold')
    expect(compositionState.document.tracks[0]!.clips[0]).toMatchObject({ colorAnimation: { track: { keyframes: [{ interpolation: 'hold' },{ tick: 500000 }] } } })
    undoComposition();await tick();expect(JSON.stringify(compositionState.document)).toBe(keyed)
    await change('Кривая цветового ключа 0','ease_out_elastic')
    const elastic = JSON.stringify(compositionState.document)
    await change('Цветовой сегмент 0 амплитуда','1.5')
    expect(compositionState.document.tracks[0]!.clips[0]).toMatchObject({ colorAnimation: { track: { keyframes: [{ interpolation: { kind: 'elastic',amplitude: 1.5,period: 0.3 } },{ tick: 500000 }] } } })
    undoComposition();await tick();expect(JSON.stringify(compositionState.document)).toBe(elastic)
    await change('Цветовой сегмент 0 период','0')
    expect(JSON.stringify(compositionState.document)).toBe(elastic)
    undoComposition();await tick();expect(JSON.stringify(compositionState.document)).toBe(keyed)
    await change('Экстраполяция цвета after','ping_pong')
    expect(compositionState.document.tracks[0]!.clips[0]).toMatchObject({ colorAnimation: { track: { extrapolation: { before: 'hold',after: 'ping_pong' } } } })
    undoComposition();await tick();expect(JSON.stringify(compositionState.document)).toBe(keyed)
    await change('Цвет ключа 500000','#0000FFFF')
    expect(compositionState.document.tracks[0]!.clips[0]).toMatchObject({ colorAnimation: { track: { keyframes: [{ value: '#FF0000FF' },{ value: '#0000FFFF' }] } } })
    undoComposition();await tick();expect(JSON.stringify(compositionState.document)).toBe(keyed)
    await change('Время цветового ключа 500000','0.75')
    expect(target.querySelector('[aria-label="Цвет ключа 750000"]')).not.toBeNull()
    undoComposition();await tick();expect(JSON.stringify(compositionState.document)).toBe(keyed)
    await change('Время цветового ключа 500000','0')
    expect(JSON.stringify(compositionState.document)).toBe(keyed)
    target.querySelector<HTMLButtonElement>('[aria-label="Удалить цветовой ключ 500000"]')!.click();await tick()
    undoComposition();await tick();expect(JSON.stringify(compositionState.document)).toBe(keyed)
    commitDocument({ ...compositionState.document,tracks: compositionState.document.tracks.map(track => ({ ...track,locked: true })) });await tick()
    expect(target.querySelector('fieldset')!.disabled).toBe(true)
    const locked = JSON.stringify(compositionState.document)
    target.querySelector('button')!.click();await tick()
    expect(JSON.stringify(compositionState.document)).toBe(locked)
  } finally { await unmount(component);target.remove();resetCompositionForTests() }
})

import {writeFileSync} from 'node:fs'
import { detachCompositionVideoAudio } from '$lib/state/composition.svelte.js'
import { importPreferences } from '$lib/state/importPreferences.svelte.js'
// @ts-expect-error Vitest's SSR condition exposes server-only mount; this test needs Svelte's client entry.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { COMPOSITION_TIME_BASE, type CompositionClip } from '$lib/composition/types.js'
import { buildCompositionRenderRequest } from '$lib/composition/payload.js'
import {
  addCompositionTrack,
  addTextToComposition,
  addCompositionVideoMask,
  addMediaInfoToComposition,
  compositionState,
  applyCompositionVisualAnimation,
  setCompositionScaleLinked,
  setCompositionClipLocked,
  toggleCompositionTrackFlag,
  compositionRenderOutput,
  resetCompositionForTests,
  setCompositionPlayhead,
  setCompositionAutomationKeyframe,
  splitSelectedCompositionClip,
  undoComposition,
  redoComposition,
  updateCompositionChromaKey,
} from '$lib/state/composition.svelte.js'
import { state as legacyState } from '$lib/state/store.svelte.js'
import CompositionInspector from './CompositionInspector.svelte'
import { clearSpaceBrandKit, spaceBrandState } from '$lib/state/spaceBrand.svelte.js'
import { spacesState } from '$lib/state/spaces.svelte.js'

let target: HTMLDivElement

beforeEach(() => {
  // Exercise embedded-audio authoring independently of the default import policy.
  importPreferences.autoPairAudio = false
  legacyState.capabilities = {
    schemaVersion: 1,
    toolFingerprint: 'test',
    formats: [],
    codecs: [],
    filters: [
      { id: 'audio-ducking', label: 'Auto ducking', available: true },
      { id: 'audio-voice-echo', label: 'Voice echo', available: true },
      { id: 'audio-voice-robot', label: 'Robot voice', available: true },
      { id: 'audio-tone', label: 'Bass/treble tone', available: true },
    ],
    hardware: [],
    features: [
      { id: 'composition-v1', label: 'Composition v1', available: true },
      { id: 'stabilization', label: 'Stabilization', available: true },
      { id: 'speed-ramp', label: 'Speed ramp', available: true },
      { id: 'composition-audio-mp3', label: 'Composition MP3', available: true },
      { id: 'composition-audio-wav', label: 'Composition WAV', available: true },
      { id: 'composition-audio-aac', label: 'Composition AAC', available: true },
      { id: 'composition-audio-flac', label: 'Composition FLAC', available: true },
    ],
  }
  resetCompositionForTests()
  clearSpaceBrandKit()
  spacesState.selectedId = ''
  addMediaInfoToComposition({
    id: 'inspector-video',
    url: '/files/sources/inspector.mp4',
    filename: 'inspector.mp4',
    mediaType: 'video',
    duration: 10,
    width: 1280,
    height: 720,
    acodec: 'aac',
  })
  setCompositionPlayhead(4 * COMPOSITION_TIME_BASE)
  splitSelectedCompositionClip()
  target = document.createElement('div')
  document.body.append(target)
})

afterEach(() => {
  legacyState.capabilities = null
  target?.remove()
})

function change(control: HTMLInputElement | HTMLSelectElement, value: string): void {
  control.value = value
  control.dispatchEvent(new Event('input', { bubbles: true }))
  control.dispatchEvent(new Event('change', { bubbles: true }))
}

function control<T extends HTMLInputElement | HTMLSelectElement>(selector: string): T {
  const result = target.querySelector<T>(selector)
  if (!result) throw new Error(`Missing inspector control: ${selector}`)
  return result
}

function button(text: string): HTMLButtonElement {
  const result = [...target.querySelectorAll('button')].find((candidate) => candidate.textContent?.includes(text))
  if (!result) throw new Error(`Missing inspector button: ${text}`)
  return result
}

function fieldset(legend: string): HTMLFieldSetElement {
  const result = [...target.querySelectorAll('fieldset')].find(
    (candidate) => candidate.querySelector(':scope > legend')?.textContent?.trim() === legend,
  )
  if (!result) throw new Error(`Missing inspector fieldset: ${legend}`)
  return result
}

describe('CompositionInspector static authoring', () => {
  it('keeps clip unlock available on a locked track while content remains read-only', async () => {
    const id = compositionState.ui.selectedClipId!
    setCompositionClipLocked(id, true)
    toggleCompositionTrackFlag(compositionState.ui.selectedTrackId!, 'locked')
    const component = mount(CompositionInspector, { target })
    await tick()
    const control = target.querySelector<HTMLInputElement>('input[aria-label="Заблокировать клип"]')!
    expect(control.checked).toBe(true)
    expect(control.disabled).toBe(false)
    expect(target.querySelector<HTMLFieldSetElement>('.composition-clip-controls')!.disabled).toBe(true)
    control.click()
    await tick()
    expect(compositionState.document.tracks[0]!.clips.find((clip) => clip.id === id)!.locked).toBe(false)
    expect(target.querySelector<HTMLFieldSetElement>('.composition-clip-controls')!.disabled).toBe(true)
    await unmount(component)
  })

  it('toggles only the inspected clip during multi-selection', async () => {
    const rightId = compositionState.ui.selectedClipId!
    const clips = compositionState.document.tracks[0]!.clips
    compositionState.ui.selectedClipIds = clips.map((clip) => clip.id)
    const component = mount(CompositionInspector, { target })
    await tick()
    const control = target.querySelector<HTMLInputElement>('input[aria-label="Клип включён"]')!
    expect(control.checked).toBe(true)
    control.checked = false
    control.dispatchEvent(new Event('change', { bubbles: true }))
    await tick()
    expect(compositionState.document.tracks[0]!.clips.find((clip) => clip.id === rightId)!.enabled).toBe(false)
    expect(compositionState.document.tracks[0]!.clips.find((clip) => clip.id !== rightId)!.enabled).toBeUndefined()
    undoComposition()
    await tick()
    expect(control.checked).toBe(true)
    await unmount(component)
  })

  it('links entire scale tracks in one undoable action and clears the link on divergence', async () => {
    const id = compositionState.ui.selectedClipId!
    const scaleX = { mode: 'keyframes' as const, track: { timeBase: COMPOSITION_TIME_BASE, interpolation: 'linear' as const, keyframes: [
      { tick: 0, value: 1, interpolation: 'ease_in' as const }, { tick: COMPOSITION_TIME_BASE, value: 2 },
    ] } }
    applyCompositionVisualAnimation(id, { scaleX, scaleY: { mode: 'constant', value: 3 } })
    const before = JSON.stringify(compositionState.document)
    const component = mount(CompositionInspector, { target })
    await tick()
    const checkbox = control<HTMLInputElement>('input[aria-label="Связать масштаб X и Y"]')
    const history = compositionState.history.past.length
    checkbox.checked = true
    checkbox.dispatchEvent(new Event('change', { bubbles: true }))
    await tick()
    const current = () => compositionState.document.tracks.flatMap((track) => [...track.clips]).find((clip) => clip.id === id)!
    const linked = current()
    expect(linked.kind).toBe('video')
    if (linked.kind === 'audio') throw new Error('visual clip required')
    expect(linked.scaleLinked).toBe(true)
    expect(linked.animation?.scaleY).toEqual(scaleX)
    expect(linked.animation?.scaleY).not.toBe(linked.animation?.scaleX)
    expect(compositionState.history.past.length).toBe(history + 1)
    undoComposition()
    expect(JSON.stringify(compositionState.document)).toBe(before)
    checkbox.checked = true
    checkbox.dispatchEvent(new Event('change', { bubbles: true }))
    await tick()
    applyCompositionVisualAnimation(id, { scaleY: { mode: 'constant', value: 4 } })
    expect(current().kind !== 'audio' && (current() as typeof linked).scaleLinked).toBe(false)
    undoComposition()
    expect((current() as typeof linked).scaleLinked).toBe(true)
    await unmount(component)
  })

  it('exposes arriving controls on a key whose outgoing segment is linear', async () => {
    const id = compositionState.ui.selectedClipId!
    applyCompositionVisualAnimation(id, { x: { mode: 'keyframes', track: { timeBase: 1_000_000, interpolation: 'linear', keyframes: [{ tick: 0, value: 0, interpolation: { kind: 'bezier', p1: [0.2,0.3], p2: [0.7,0.8] } }, { tick: 1_000_000, value: 10 }] } } })
    const component = mount(CompositionInspector, { target })
    await tick()
    const select = control<HTMLSelectElement>('select[aria-label="Параметр keyframes"]')
    change(select, 'x')
    await tick()
    const editor = select.closest('fieldset')!
    const arriving = editor.querySelector<HTMLInputElement>('input[aria-label="Касательная incoming y keyframe 1000000"]')!
    expect(arriving).not.toBeNull()
    expect(arriving.value).toBe('0.8')
    const before = JSON.stringify(compositionState.document)
    const depth = compositionState.history.past.length
    change(arriving, '0.6')
    await tick()
    expect(compositionState.history.past.length).toBe(depth + 1)
    undoComposition()
    await tick()
    expect(JSON.stringify(compositionState.document)).toBe(before)
    await unmount(component)
  })

  it('edits a linked Scale through the inspector with atomic twin writes', async () => {
    const id = compositionState.ui.selectedClipId!
    setCompositionScaleLinked(id, true)
    const component = mount(CompositionInspector, { target })
    await tick()
    const select = control<HTMLSelectElement>('select[aria-label="Параметр keyframes"]')
    expect(select.querySelector('option[value="scaleY"]')).toBeNull()
    expect(select.querySelector('option[value="scaleX"]')?.textContent).toBe('Scale')
    change(select, 'scaleX')
    await tick()
    const editor = select.closest('fieldset')!
    const current = () => compositionState.document.tracks.flatMap((track) => [...track.clips]).find((clip) => clip.id === id)!
    const twins = () => {
      const clip = current()
      if (clip.kind === 'audio') throw new Error('visual required')
      expect(clip.scaleLinked).toBe(true)
      expect(clip.animation?.scaleY).toEqual(clip.animation?.scaleX)
      return clip.animation!.scaleX!
    }
    const history = compositionState.history.past.length
    const before = JSON.stringify(compositionState.document)
    Array.from(editor.querySelectorAll('button')).find((button) => button.textContent?.includes('+ Ключ'))!.click()
    await tick()
    expect(twins().mode).toBe('keyframes')
    expect(compositionState.history.past.length).toBe(history + 1)
    change(editor.querySelector<HTMLInputElement>('input[aria-label="Значение keyframe Scale X"]')!, '2')
    await tick()
    expect(twins()).toMatchObject({ track: { keyframes: [{ value: 2 }] } })
    change(editor.querySelector<HTMLSelectElement>('select[aria-label="Интерполяция keyframes"]')!, 'ease_out')
    await tick()
    expect(twins()).toMatchObject({ track: { interpolation: 'ease_out' } })
    const bezierBefore = JSON.stringify(compositionState.document)
    change(editor.querySelector<HTMLSelectElement>('select[aria-label="Интерполяция keyframes"]')!, 'bezier')
    await tick()
    change(editor.querySelector<HTMLInputElement>('input[aria-label="Bézier p1 Y"]')!, '-2')
    await tick()
    expect(twins()).toMatchObject({ track: { interpolation: { kind: 'bezier', p1: [0.25, -2], p2: [0.25, 1] } } })
    const bezierWire = buildCompositionRenderRequest(compositionState.document).composition.tracks.map((track) => track.clips.find((clip) => clip.id === id)).find((clip) => clip !== undefined)
    expect(bezierWire).toMatchObject({ transform: { scaleX: { track: { interpolation: { kind: 'bezier', p1: [0.25, -2] } } } } })
    const invalidBezierBefore = JSON.stringify(compositionState.document)
    const tangentDepth = compositionState.history.past.length
    change(editor.querySelector<HTMLSelectElement>('select[aria-label="Режим outgoing keyframe 0"]')!, 'auto')
    await tick()
    expect(twins()).toMatchObject({ track: { keyframes: [{ tangents: { outgoing: { mode: 'auto', x: 0.25, y: -2 } } }] } })
    expect(compositionState.history.past.length).toBe(tangentDepth + 1)
    change(editor.querySelector<HTMLSelectElement>('select[aria-label="Непрерывность keyframe 0"]')!, 'smooth')
    await tick()
    expect(twins()).toMatchObject({ track: { keyframes: [{ tangents: { continuity: 'smooth' } }] } })
    expect(compositionState.history.past.length).toBe(tangentDepth + 2)
    change(editor.querySelector<HTMLInputElement>('input[aria-label="Касательная outgoing y keyframe 0"]')!, '-1.5')
    await tick()
    expect(twins()).toMatchObject({ track: { keyframes: [{ tangents: { outgoing: { mode: 'free', x: 0.25, y: -1.5 } } }] } })
    expect(compositionState.history.past.length).toBe(tangentDepth + 3)
    const tangentDocument = JSON.stringify(compositionState.document)
    change(editor.querySelector<HTMLInputElement>('input[aria-label="Касательная outgoing x keyframe 0"]')!, '-0.1')
    await tick()
    expect(JSON.stringify(compositionState.document)).toBe(tangentDocument)
    expect(compositionState.history.past.length).toBe(tangentDepth + 3)
    undoComposition(); undoComposition(); undoComposition()
    await tick()
    expect(JSON.stringify(compositionState.document)).toBe(invalidBezierBefore)
    const controlDepth = compositionState.history.past.length
    const controlGraph = editor.querySelector<SVGSVGElement>('svg[aria-label="Контрольные точки Bézier"]')!
    vi.spyOn(controlGraph, 'getBoundingClientRect').mockReturnValue(new DOMRect(0, 0, 100, 100))
    const handle = controlGraph.querySelector<SVGCircleElement>('[aria-label="Переместить Bézier p1"]')!
    handle.dispatchEvent(new MouseEvent('pointerdown', { bubbles: true, button: 0, clientX: 30, clientY: 90 }))
    controlGraph.dispatchEvent(new MouseEvent('pointermove', { bubbles: true, clientX: 50, clientY: 50 }))
    await tick()
    expect(JSON.stringify(compositionState.document)).toBe(invalidBezierBefore)
    expect(compositionState.history.past.length).toBe(controlDepth)
    // The handle remains under the pointer even when the draft changes Y extents.
    expect(Number(handle.getAttribute('cy'))).toBeCloseTo(50)
    expect(compositionState.ui.keyframePreviews).toHaveLength(2)
    expect(compositionState.ui.keyframePreviews[0]?.value).toMatchObject({ track: { interpolation: { kind: 'bezier', p1: [0.5, -0.5] } } })
    controlGraph.dispatchEvent(new MouseEvent('pointerup', { bubbles: true, clientX: 50, clientY: 50 }))
    await tick()
    expect(compositionState.history.past.length).toBe(controlDepth + 1)
    expect(twins()).toMatchObject({ track: { interpolation: { p1: [0.5, -0.5] } } })
    expect(compositionState.ui.keyframePreviews).toEqual([])
    undoComposition()
    await tick()
    expect(JSON.stringify(compositionState.document)).toBe(invalidBezierBefore)
    handle.dispatchEvent(new MouseEvent('pointerdown', { bubbles: true, button: 0, clientX: 30, clientY: 90 }))
    controlGraph.dispatchEvent(new MouseEvent('pointermove', { bubbles: true, clientX: 50, clientY: 50 }))
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    controlGraph.dispatchEvent(new MouseEvent('pointerup', { bubbles: true, clientX: 50, clientY: 50 }))
    await tick()
    expect(compositionState.ui.keyframePreviews).toEqual([])
    expect(JSON.stringify(compositionState.document)).toBe(invalidBezierBefore)
    expect(compositionState.history.past.length).toBe(controlDepth)
    handle.dispatchEvent(new MouseEvent('pointerdown', { bubbles: true, button: 0, clientX: 30, clientY: 90 }))
    controlGraph.dispatchEvent(new MouseEvent('pointerup', { bubbles: true, clientX: 30, clientY: 90 }))
    await tick()
    expect(JSON.stringify(compositionState.document)).toBe(invalidBezierBefore)
    expect(compositionState.history.past.length).toBe(controlDepth)
    handle.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true }))
    await tick()
    expect(twins()).toMatchObject({ track: { interpolation: { p1: [0.26, -2] } } })
    expect(compositionState.history.past.length).toBe(controlDepth + 1)
    undoComposition()
    await tick()
    expect(JSON.stringify(compositionState.document)).toBe(invalidBezierBefore)
    handle.dispatchEvent(new MouseEvent('pointerdown', { bubbles: true, button: 0, clientX: 30, clientY: 90 }))
    controlGraph.dispatchEvent(new MouseEvent('pointermove', { bubbles: true, clientX: 50, clientY: 50 }))
    await tick()
    setCompositionClipLocked(id, true)
    await tick()
    const lockedDocument = JSON.stringify(compositionState.document)
    const lockedDepth = compositionState.history.past.length
    expect(compositionState.ui.keyframePreviews).toEqual([])
    controlGraph.dispatchEvent(new MouseEvent('pointerup', { bubbles: true, clientX: 50, clientY: 50 }))
    await tick()
    expect(JSON.stringify(compositionState.document)).toBe(lockedDocument)
    expect(compositionState.history.past.length).toBe(lockedDepth)
    undoComposition()
    await tick()
    change(editor.querySelector<HTMLInputElement>('input[aria-label="Bézier p1 X"]')!, '-0.1')
    await tick()
    expect(JSON.stringify(compositionState.document)).toBe(invalidBezierBefore)
    undoComposition(); undoComposition()
    await tick()
    expect(JSON.stringify(compositionState.document)).toBe(bezierBefore)
    const elasticBefore = JSON.stringify(compositionState.document)
    change(editor.querySelector<HTMLSelectElement>('select[aria-label="Интерполяция keyframes"]')!, 'elastic_out')
    await tick()
    expect(twins()).toMatchObject({ track: { interpolation: { kind: 'elastic', dir: 'out', amplitude: 1, period: 0.3 } } })
    change(editor.querySelector<HTMLInputElement>('input[aria-label="Амплитуда Elastic"]')!, '2')
    change(editor.querySelector<HTMLInputElement>('input[aria-label="Период Elastic"]')!, '0.17')
    await tick()
    expect(twins()).toMatchObject({ track: { interpolation: { amplitude: 2, period: 0.17 } } })
    const elasticWire = buildCompositionRenderRequest(compositionState.document).composition.tracks
      .map((track) => track.clips.find((clip) => clip.id === id)).find((clip) => clip !== undefined)
    expect(elasticWire).toMatchObject({ transform: { scaleX: { track: { interpolation: { kind: 'elastic', dir: 'out', amplitude: 2, period: 0.17 } } } } })
    const beforeInvalidElastic = JSON.stringify(compositionState.document)
    change(editor.querySelector<HTMLInputElement>('input[aria-label="Период Elastic"]')!, '0')
    await tick()
    expect(JSON.stringify(compositionState.document)).toBe(beforeInvalidElastic)
    undoComposition(); undoComposition(); undoComposition()
    await tick()
    expect(JSON.stringify(compositionState.document)).toBe(elasticBefore)
    for (const bounce of ['bounce_in', 'bounce_out', 'bounce_in_out']) {
      const beforeBounce = JSON.stringify(compositionState.document)
      const bounceDepth = compositionState.history.past.length
      const interpolation = editor.querySelector<HTMLSelectElement>('select[aria-label="Интерполяция keyframes"]')!
      expect(interpolation.querySelector(`option[value="${bounce}"]`)).not.toBeNull()
      change(interpolation, bounce)
      await tick()
      expect(twins()).toMatchObject({ track: { interpolation: bounce } })
      const wire = buildCompositionRenderRequest(compositionState.document).composition.tracks
        .map((track) => track.clips.find((clip) => clip.id === id)).find((clip) => clip !== undefined)
      expect(wire).toMatchObject({ transform: { scaleX: { track: { interpolation: bounce } }, scaleY: { track: { interpolation: bounce } } } })
      expect(compositionState.history.past.length).toBe(bounceDepth + 1)
      undoComposition()
      await tick()
      expect(JSON.stringify(compositionState.document)).toBe(beforeBounce)
    }
    change(editor.querySelector<HTMLInputElement>('input[aria-label="Время keyframe Scale X"]')!, '1')
    await tick()
    expect(twins()).toMatchObject({ track: { keyframes: [{ tick: COMPOSITION_TIME_BASE }] } })
    const beforeClear = JSON.stringify(compositionState.document)
    Array.from(editor.querySelectorAll('button')).find((button) => button.textContent?.includes('Сделать статическим'))!.click()
    await tick()
    expect(twins()).toEqual({ mode: 'constant', value: 2 })
    undoComposition()
    await tick()
    expect(JSON.stringify(compositionState.document)).toBe(beforeClear)
    editor.querySelector<HTMLButtonElement>('button[aria-label^="Удалить keyframe Scale X"]')!.click()
    await tick()
    expect(twins().mode).toBe('constant')
    for (let index = 0; index < 5; index++) undoComposition()
    expect(JSON.stringify(compositionState.document)).toBe(before)
    await unmount(component)
  })

  it('materializes editable animation presets and clears them atomically', async () => {
    const component = mount(CompositionInspector, { target })
    await tick()
    change(control('select[aria-label="Animation preset"]'), 'zoom_in')
    change(control('input[aria-label="Animation preset duration"]'), '0.5')
    await tick()
    button('Применить анимацию').click()
    await tick()

    const clip = compositionState.document.tracks
      .flatMap((track) => track.kind === 'video' ? track.clips : [])
      .find((candidate) => candidate.id === compositionState.ui.selectedClipId)!
    expect(clip.animation?.scaleX).toMatchObject({
      mode: 'keyframes',
      track: { interpolation: 'ease_out', keyframes: [{ tick: 0, value: 0.25 }, { tick: 500_000, value: 1 }] },
    })
    expect(clip.animation?.opacity).toMatchObject({
      mode: 'keyframes', track: { keyframes: [{ tick: 0, value: 0 }, { tick: 500_000, value: 1 }] },
    })
    const wire = buildCompositionRenderRequest(compositionState.document).composition.tracks
      .flatMap((track) => track.kind === 'video' ? track.clips : [])
      .find((candidate) => candidate.id === clip.id)!
    expect(wire.transform.scaleX).toEqual(clip.animation?.scaleX)
    expect(wire.opacity).toEqual(clip.animation?.opacity)

    button('Очистить анимацию').click()
    await tick()
    expect(compositionState.document.tracks
      .flatMap((track) => track.kind === 'video' ? track.clips : [])
      .find((candidate) => candidate.id === clip.id)?.animation).toBeUndefined()
    undoComposition()
    expect(compositionState.document.tracks
      .flatMap((track) => track.kind === 'video' ? track.clips : [])
      .find((candidate) => candidate.id === clip.id)?.animation?.scaleX).toBeDefined()
    await unmount(component)
  })

  it('authors an exact fractional frame rate and clears it on numeric FPS changes with Undo', async () => {
    const component = mount(CompositionInspector, { target })
    await tick()
    change(control('select[aria-label="Canvas FPS"]'), '30000/1001')
    await tick()
    expect(compositionState.document.canvas.fpsRate).toEqual({ numerator: 30000, denominator: 1001 })
    expect(compositionState.document.canvas.fps).toBe(30000 / 1001)
    expect(buildCompositionRenderRequest(compositionState.document).composition.canvas.fpsRate).toEqual({ numerator: 30000, denominator: 1001 })
    change(control('select[aria-label="Canvas FPS"]'), '30')
    await tick()
    expect(compositionState.document.canvas.fpsRate).toBeUndefined()
    expect(compositionState.document.canvas.fps).toBe(30)
    undoComposition()
    await tick()
    expect(compositionState.document.canvas.fpsRate).toEqual({ numerator: 30000, denominator: 1001 })
    expect((control('select[aria-label="Canvas FPS"]') as HTMLSelectElement).value).toBe('30000/1001')
    await unmount(component)
  })

  it('authors social canvas presets, custom background and undo', async () => {
    const component = mount(CompositionInspector, { target })
    await tick()

    change(control('select[aria-label="Canvas preset"]'), '9:16')
    change(control('select[aria-label="Canvas FPS"]'), '59.94')
    change(control('input[aria-label="Canvas background color"]'), '#123456')
    change(control('select[aria-label="Canvas background mode"]'), 'blur')
    await tick()
    change(control('input[aria-label="Canvas background blur"]'), '36')
    await tick()

    expect(compositionState.document.canvas).toMatchObject({
      width: 1080,
      height: 1920,
      fps: 59.94,
      backgroundColor: '#123456',
      backgroundMode: 'blur',
      backgroundBlur: 36,
    })
    expect(buildCompositionRenderRequest(compositionState.document).composition.canvas).toMatchObject({
      width: 1080,
      height: 1920,
      fpsMilli: 59_940,
      background: {
        red: 0x12 / 255,
        green: 0x34 / 255,
        blue: 0x56 / 255,
        alpha: 1,
      },
      backgroundMode: 'blur',
      backgroundBlur: 36,
    })

    undoComposition()
    expect(compositionState.document.canvas.backgroundBlur).toBe(24)
    undoComposition()
    expect(compositionState.document.canvas.backgroundMode).toBe('color')
    undoComposition()
    expect(compositionState.document.canvas.backgroundColor).toBe('#000000')
    await unmount(component)
  })

  it('stacks bounded video effects into the strict render wire and removes them', async () => {
    const component = mount(CompositionInspector, { target })
    await tick()

    change(control('select[aria-label="Новый video effect"]'), 'rgb_split')
    await tick()
    button('+ Эффект').click()
    await tick()
    change(control('select[aria-label="Новый video effect"]'), 'posterize')
    await tick()
    button('+ Эффект').click()
    await tick()
    change(control('input[aria-label="Video effect 2 intensity"]'), '0.8')
    await tick()

    const selected = compositionState.document.tracks
      .flatMap((track) => track.kind === 'video' ? track.clips : [])
      .find((clip) => clip.id === compositionState.ui.selectedClipId)!
    expect(selected.videoEffects).toEqual([
      { preset: 'rgb_split', intensity: 0.5 },
      { preset: 'posterize', intensity: 0.8 },
    ])
    const wire = buildCompositionRenderRequest(compositionState.document).composition.tracks
      .flatMap((track) => track.kind === 'video' ? track.clips : [])
      .find((clip) => clip.id === selected.id)!
    expect(wire.effects.slice(0, 2)).toEqual([
      { kind: 'style', preset: 'rgb_split', intensity: 0.5 },
      { kind: 'style', preset: 'posterize', intensity: 0.8 },
    ])

    target.querySelector<HTMLButtonElement>('button[aria-label="Video effect 2 вверх"]')!.click()
    await tick()
    expect(compositionState.document.tracks
      .flatMap((track) => track.kind === 'video' ? track.clips : [])
      .find((clip) => clip.id === selected.id)?.videoEffects).toEqual([
        { preset: 'posterize', intensity: 0.8 },
        { preset: 'rgb_split', intensity: 0.5 },
      ])

    target.querySelector<HTMLButtonElement>('button[aria-label="Удалить video effect 1"]')!.click()
    await tick()
    expect(compositionState.document.tracks
      .flatMap((track) => track.kind === 'video' ? track.clips : [])
      .find((clip) => clip.id === selected.id)?.videoEffects).toEqual([{ preset: 'rgb_split', intensity: 0.5 }])
    await unmount(component)
  })

  it('detaches embedded video audio atomically with matching timing and undo', async () => {
    const component = mount(CompositionInspector, { target })
    await tick()
    change(control('input[aria-label="Громкость source audio"]'), '1.25')
    change(control('input[aria-label="Панорама source audio"]'), '-0.3')
    change(control('select[aria-label="Режим воспроизведения"]'), 'reverse')
    button('Отделить звук').click()
    await tick()

    const video = compositionState.document.tracks
      .flatMap((track) => track.kind === 'video' ? track.clips : [])
      .find((clip) => clip.timelineStartTicks === 4 * COMPOSITION_TIME_BASE)!
    const detached = compositionState.document.tracks
      .flatMap((track) => track.kind === 'audio' ? track.clips : [])
      .find((clip) => clip.id === compositionState.ui.selectedClipId)
    expect(video.sourceAudioEnabled).toBe(false)
    expect(detached).toMatchObject({
      sourceId: video.sourceId,
      timelineStartTicks: video.timelineStartTicks,
      sourceInTicks: video.sourceInTicks,
      sourceOutTicks: video.sourceOutTicks,
      speed: video.speed,
      gain: 1.25,
      pan: -0.3,
      reversed: true,
    })
    expect(control<HTMLInputElement>('input[aria-label="Reverse audio"]').checked).toBe(true)

    undoComposition()
    expect(compositionState.document.tracks.some((track) => track.kind === 'audio')).toBe(false)
    expect(compositionState.document.tracks.flatMap((track) => track.kind === 'video' ? track.clips : [])
      .find((clip) => clip.id === video.id)?.sourceAudioEnabled).toBe(true)
    await unmount(component)
  })

  it('runs local silence removal for the selected multitrack clip', async () => {
    const waveformCache = {
      load: vi.fn().mockResolvedValue({
        durationSeconds: 10,
        sampleRate: 48_000,
        buckets: [0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0, 0, 0.5, 0.5]
          .map((rms) => ({ min: -rms, max: rms, rms })),
      }),
    }
    const component = mount(CompositionInspector, { target, props: { waveformCache } })
    await tick()

    button('Убрать тишину').click()
    await vi.waitFor(() => expect(compositionState.ui.message).toContain('2 audible фрагм.'))
    expect(waveformCache.load).toHaveBeenCalledWith('/files/sources/inspector.mp4')
    expect(compositionState.document.tracks[0]!.clips).toHaveLength(3)
    expect(compositionState.ui.selectedClipId).toBe(compositionState.document.tracks[0]!.clips[1]!.id)
    await unmount(component)
  })

  it('applies Space brand colors and fonts to an exportable text clip', async () => {
    addTextToComposition('Brand title')
    spacesState.selectedId = 'space-1'
    Object.assign(spaceBrandState, {
      spaceId: 'space-1',
      kit: { colors: [{ name: 'Primary', value: '#3366ff' }], fonts: ['Arial'], logoSourceIds: [] },
      revision: 1,
    })
    const component = mount(CompositionInspector, { target })
    await tick()

    button('Primary').click()
    button('Arial').click()
    await tick()
    const track = compositionState.document.tracks.find((candidate) => candidate.kind === 'text')!
    expect(track.clips[0]?.style).toMatchObject({ color: '#3366ff', fontFamily: 'Arial' })

    await unmount(component)
  })

  it('authors exact-handle transitions plus speed, transform, chroma, and source audio fields', async () => {
    const component = mount(CompositionInspector, { target })
    await tick()

    const opacityEditor = fieldset('Keyframes')
    const opacityProperty = opacityEditor.querySelector<HTMLSelectElement>('select[aria-label="Параметр keyframes"]')!
    expect([...opacityProperty.options].map((option) => option.value)).toContain('opacity')
    change(opacityProperty, 'opacity')
    await tick()
    const addOpacityKey = [...fieldset('Keyframes').querySelectorAll('button')].find((candidate) => candidate.textContent?.includes('+ Ключ'))!
    addOpacityKey.click()
    await tick()
    expect(compositionState.ui.message).toBe('')
    expect(compositionState.document.tracks.find((candidate) => candidate.kind === 'video')?.clips[1]?.animation?.opacity).toBeTruthy()
    change(fieldset('Keyframes').querySelector<HTMLInputElement>('input[aria-label="Значение keyframe Opacity"]')!, '0.4')
    await tick()

    change(control('select[aria-label="Тип перехода"]'), 'slide_left')
    change(control('input[aria-label="Длительность перехода"]'), '0.5')
    await tick()
    button('Добавить переход').click()
    await tick()

    const track = compositionState.document.tracks.find((candidate) => candidate.kind === 'video')!
    expect(track.transitions).toEqual([
      expect.objectContaining({ kind: 'slide_left', durationTicks: 500_000 }),
    ])

    change(control('input[aria-label="Скорость клипа"]'), '1.5')
    change(control('input[aria-label="Позиция X"]'), '42')
    change(control('input[aria-label="Поворот слоя"]'), '12')
    change(control('select[aria-label="Режим смешивания"]'), 'screen')
    change(control('input[aria-label="Панорама source audio"]'), '-0.35')
    control<HTMLInputElement>('input[aria-label="Включить chroma key"]').click()
    button('+ Rectangle').click()
    await tick()

    const selected = track.clips.find((candidate) => candidate.id === compositionState.ui.selectedClipId)
    const currentTrack = compositionState.document.tracks.find((candidate) => candidate.id === track.id)
    const current = currentTrack?.kind === 'video'
      ? currentTrack.clips.find((candidate) => candidate.id === compositionState.ui.selectedClipId)
      : undefined
    expect(selected).toBeTruthy()
    expect(current).toMatchObject({
      speed: 1.5,
      transform: { x: 42 },
      rotationDegrees: 12,
      blendMode: 'screen',
      audioPan: -0.35,
      chromaKey: { enabled: true },
      masks: [expect.objectContaining({ shape: 'rectangle' })],
      animation: { opacity: { mode: 'keyframes', track: { keyframes: [{ tick: 0, value: 0.4 }] } } },
    })
    expect(target.textContent).toContain('Chroma key и despill')

    await unmount(component)
  })

  it('authors audio gain, pan, and overlong fades through labelled controls', async () => {
    addMediaInfoToComposition({
      id: 'inspector-audio',
      url: '/files/sources/inspector.wav',
      filename: 'inspector.wav',
      mediaType: 'audio',
      duration: 8,
      width: 0,
      height: 0,
      acodec: 'pcm_s16le',
    })
    const component = mount(CompositionInspector, { target })
    await tick()

    change(control('input[aria-label="Громкость аудиоклипа"]'), '1.4')
    change(control('input[aria-label="Панорама аудиоклипа"]'), '0.45')
    change(control('input[aria-label="Fade in"]'), '16')
    change(control('input[aria-label="Fade out"]'), '0.75')
    change(control('select[aria-label="Voice effect"]'), 'robot')
    change(control('input[aria-label="Pitch semitones"]'), '7')
    change(control('input[aria-label="Tone dB"]'), '9')
    const ducking = control<HTMLInputElement>('input[aria-label="Auto ducking"]')
    ducking.checked = true
    ducking.dispatchEvent(new Event('change', { bubbles: true }))
    await tick()
    change(control('input[aria-label="Ducking threshold"]'), '-30')
    change(control('input[aria-label="Ducking ratio"]'), '10')
    change(control('input[aria-label="Ducking attack"]'), '15')
    change(control('input[aria-label="Ducking release"]'), '250')
    await tick()

    const audio = compositionState.document.tracks
      .find((candidate) => candidate.kind === 'audio')
      ?.clips.find((candidate) => candidate.id === compositionState.ui.selectedClipId)
    expect(audio).toMatchObject({
      gain: 1.4,
      pan: 0.45,
      fadeInTicks: 16_000_000,
      fadeOutTicks: 750_000,
      voiceEffect: 'robot',
      pitchSemitones: 7,
      toneDb: 9,
      ducking: { thresholdDb: -30, ratio: 10, attackMs: 15, releaseMs: 250 },
    })
    expect(target.textContent).toContain('stereo pan')

    await unmount(component)
  })

  it('keeps selected keys across parameter changes and moves both properties once', async () => {
    setCompositionPlayhead(0)
    const id = addMediaInfoToComposition({ id: 'cross-property-audio', url: '/files/sources/keys.wav', filename: 'keys.wav', mediaType: 'audio', duration: 8, width: 0, height: 0, acodec: 'pcm_s16le' })
    setCompositionAutomationKeyframe({ kind: 'audio', clipId: id }, 'gain', 0.5, 1_000_000)
    setCompositionAutomationKeyframe({ kind: 'audio', clipId: id }, 'pan', 0.2, 2_000_000)
    const before = JSON.stringify(compositionState.document)
    const depth = compositionState.history.past.length
    const component = mount(CompositionInspector, { target })
    await tick()
    const editor = fieldset('Audio keyframes')
    const choose = () => {
      const checkbox = editor.querySelector<HTMLInputElement>('input[type="checkbox"]')!
      checkbox.checked = true
      checkbox.dispatchEvent(new Event('change', { bubbles: true }))
    }
    choose()
    change(editor.querySelector<HTMLSelectElement>('select[aria-label="Параметр audio keyframes"]')!, 'pan')
    await tick()
    choose()
    await tick()
    expect(compositionState.ui.selectedKeyframes.map((subject) => subject.property).sort()).toEqual(['gain', 'pan'])
    editor.querySelector<HTMLButtonElement>('[aria-label="Сдвинуть выбранные ключи вперёд"]')!.click()
    await tick()
    expect(compositionState.history.past).toHaveLength(depth + 1)
    expect(compositionState.ui.selectedKeyframes).toHaveLength(2)
    expect(compositionState.ui.selectedKeyframes.every((subject) => subject.ticks[0]! > (subject.property === 'gain' ? 1_000_000 : 2_000_000))).toBe(true)
    undoComposition()
    expect(JSON.stringify(compositionState.document)).toBe(before)
    await unmount(component)
  })

  it('selects multiple keys, nudges them together and deletes them with one Undo per action', async () => {
    setCompositionPlayhead(0)
    const id = addMediaInfoToComposition({ id: 'selected-key-audio', url: '/files/sources/keys.wav', filename: 'keys.wav', mediaType: 'audio', duration: 8, width: 0, height: 0, acodec: 'pcm_s16le' })
    setCompositionAutomationKeyframe({ kind: 'audio', clipId: id }, 'gain', 0.5, 1_000_000)
    setCompositionAutomationKeyframe({ kind: 'audio', clipId: id }, 'gain', 0.8, 2_000_000)
    const before = JSON.stringify(compositionState.document)
    const depth = compositionState.history.past.length
    const component = mount(CompositionInspector, { target })
    await tick()
    const editor = fieldset('Audio keyframes')
    for (const checkbox of editor.querySelectorAll<HTMLInputElement>('input[type="checkbox"]')) {
      checkbox.checked = true
      checkbox.dispatchEvent(new Event('change', { bubbles: true }))
    }
    await tick()
    const stretch = [...editor.querySelectorAll('button')].find((button) => button.textContent?.includes('Растянуть время'))!
    stretch.click()
    await tick()
    expect(compositionState.history.past).toHaveLength(depth + 1)
    expect(editor.querySelectorAll('input[type="checkbox"]:checked')).toHaveLength(2)
    undoComposition()
    await tick()
    expect(JSON.stringify(compositionState.document)).toBe(before)
    for (const checkbox of editor.querySelectorAll<HTMLInputElement>('input[type="checkbox"]')) if (!checkbox.checked) {
      checkbox.checked = true
      checkbox.dispatchEvent(new Event('change', { bubbles: true }))
    }
    await tick()
    editor.querySelector<HTMLButtonElement>('[aria-label="Сдвинуть выбранные ключи вперёд"]')!.click()
    await tick()
    expect(compositionState.history.past).toHaveLength(depth + 1)
    expect(editor.querySelectorAll('input[type="checkbox"]:checked')).toHaveLength(2)
    const moved = JSON.stringify(compositionState.document)
    const remove = [...editor.querySelectorAll('button')].find((button) => button.textContent?.includes('Удалить выбранные ключи'))!
    remove.click()
    await tick()
    expect(compositionState.history.past).toHaveLength(depth + 2)
    undoComposition()
    expect(JSON.stringify(compositionState.document)).toBe(moved)
    undoComposition()
    expect(JSON.stringify(compositionState.document)).toBe(before)
    await unmount(component)
  })

  it('authors accessible AudioClip keyframes through the reused editor and emits exact wire', async () => {
    setCompositionPlayhead(0)
    const audioId = addMediaInfoToComposition({
      id: 'inspector-automated-audio',
      url: '/files/sources/automated.wav',
      filename: 'automated.wav',
      mediaType: 'audio',
      duration: 8,
      width: 0,
      height: 0,
      acodec: 'pcm_s16le',
    })
    setCompositionPlayhead(2 * COMPOSITION_TIME_BASE)
    const component = mount(CompositionInspector, { target })
    await tick()

    let editor = fieldset('Audio keyframes')
    const add = (): HTMLButtonElement => [...fieldset('Audio keyframes').querySelectorAll('button')]
      .find((candidate) => candidate.textContent?.includes('+ Ключ'))!
    expect(editor.querySelector('select[aria-label="Параметр audio keyframes"]')).toBeTruthy()
    expect(editor.querySelector('svg[role="img"][aria-label="График Gain по времени"]')).toBeTruthy()

    add().click()
    await tick()
    editor = fieldset('Audio keyframes')
    change(editor.querySelector<HTMLInputElement>('input[aria-label="Значение keyframe Gain"]')!, '0.65')
    await tick()
    editor = fieldset('Audio keyframes')
    change(editor.querySelector<HTMLSelectElement>('select[aria-label="Интерполяция keyframes"]')!, 'ease_in')
    await tick()
    editor = fieldset('Audio keyframes')
    change(editor.querySelector<HTMLSelectElement>('select[aria-label="Параметр audio keyframes"]')!, 'pan')
    await tick()
    add().click()
    await tick()
    editor = fieldset('Audio keyframes')
    change(editor.querySelector<HTMLInputElement>('input[aria-label="Значение keyframe Pan"]')!, '-0.4')
    await tick()

    const authored = compositionState.document.tracks
      .flatMap((candidate) => candidate.kind === 'audio' ? candidate.clips : [])
      .find((candidate) => candidate.id === audioId)!
    expect(authored.audioAnimation).toMatchObject({
      gain: { track: { interpolation: 'ease_in', keyframes: [{ tick: 2_000_000, value: 0.65 }] } },
      pan: { track: { keyframes: [{ tick: 2_000_000, value: -0.4 }] } },
    })
    const wireTrack = buildCompositionRenderRequest(compositionState.document).composition.tracks
      .find((candidate) => candidate.kind === 'audio')!
    expect(wireTrack.kind === 'audio' ? wireTrack.clips[0] : null).toMatchObject({
      gain: { mode: 'keyframes', track: { interpolation: 'ease_in', keyframes: [{ tick: 2_000_000, value: 0.65 }] } },
      pan: { mode: 'keyframes', track: { interpolation: 'linear', keyframes: [{ tick: 2_000_000, value: -0.4 }] } },
    })

    await unmount(component)
  })

  it('authors Rectangle mask geometry keyframes without disturbing chroma and emits exact effect wire', async () => {
    addCompositionTrack('video')
    setCompositionPlayhead(COMPOSITION_TIME_BASE)
    const overlayId = addMediaInfoToComposition({
      id: 'inspector-mask-overlay',
      url: '/files/sources/mask-overlay.mp4',
      filename: 'mask-overlay.mp4',
      mediaType: 'video',
      duration: 4,
      width: 640,
      height: 360,
      acodec: null,
    })
    const maskId = addCompositionVideoMask(overlayId, 'rectangle')
    updateCompositionChromaKey(overlayId, {
      enabled: true,
      color: '#00ff00',
      similarity: 0.3,
      softness: 0.1,
      spill: 0.2,
    })
    const component = mount(CompositionInspector, { target })
    await tick()

    let editor = fieldset('Mask keyframes')
    const add = (): HTMLButtonElement => [...fieldset('Mask keyframes').querySelectorAll('button')]
      .find((candidate) => candidate.textContent?.includes('+ Ключ'))!
    add().click()
    await tick()
    editor = fieldset('Mask keyframes')
    change(editor.querySelector<HTMLInputElement>('input[aria-label="Значение keyframe X"]')!, '0.35')
    await tick()
    editor = fieldset('Mask keyframes')
    change(editor.querySelector<HTMLSelectElement>('select[aria-label="Параметр mask keyframes"]')!, 'width')
    await tick()
    add().click()
    await tick()
    editor = fieldset('Mask keyframes')
    change(editor.querySelector<HTMLInputElement>('input[aria-label="Значение keyframe Width"]')!, '1.2')
    await tick()

    const overlayTrack = buildCompositionRenderRequest(compositionState.document).composition.tracks
      .find((candidate) => candidate.kind === 'video' && candidate.id === compositionState.ui.selectedTrackId)!
    const effects = overlayTrack.kind === 'video' ? overlayTrack.clips[0]!.effects : []
    expect(effects[0]).toMatchObject({ kind: 'chroma_key' })
    expect(effects[1]).toMatchObject({
      kind: 'mask',
      shape: 'rectangle',
      x: { mode: 'keyframes', track: { keyframes: [{ tick: 1_000_000, value: 0.35 }] } },
      width: { mode: 'keyframes', track: { keyframes: [{ tick: 1_000_000, value: 1.2 }] } },
    })
    expect(effects[1]).not.toHaveProperty('id')
    expect(compositionState.document.tracks
      .flatMap((candidate) => candidate.kind === 'video' ? candidate.clips : [])
      .find((candidate) => candidate.id === overlayId)?.masks?.[0]?.id).toBe(maskId)

    await unmount(component)
  })

  it('authors an accessible speed point table, curve, interpolation and audio policy', async () => {
    setCompositionPlayhead(5 * COMPOSITION_TIME_BASE)
    const component = mount(CompositionInspector, { target })
    await tick()

    button('Включить speed ramp').click()
    await tick()
    expect(target.querySelector('svg[role="img"][aria-label="Кривая speed ramp"]')).toBeTruthy()
    expect(target.querySelector('table caption')?.textContent).toContain('presentation-order source progress')
    button('+ Point at playhead').click()
    await tick()

    change(control<HTMLSelectElement>('select[aria-label="Интерполяция speed ramp"]'), 'hold')
    change(control<HTMLSelectElement>('select[aria-label="Политика аудио speed ramp"]'), 'mute')
    const speedInputs = target.querySelectorAll<HTMLInputElement>('input[aria-label^="Speed point "][aria-label$=" speed"]')
    expect(speedInputs).toHaveLength(3)
    change(speedInputs[1]!, '0.5')
    await tick()

    const selected = compositionState.document.tracks
      .flatMap((candidate) => candidate.kind === 'video' ? candidate.clips : [])
      .find((candidate) => candidate.id === compositionState.ui.selectedClipId)
    expect(selected?.speedRamp).toMatchObject({
      interpolation: 'hold',
      audioPolicy: 'mute',
      points: [
        { sourceProgressTick: 0, speed: 1 },
        { sourceProgressTick: COMPOSITION_TIME_BASE, speed: 0.5 },
        { sourceProgressTick: 6 * COMPOSITION_TIME_BASE, speed: 1 },
      ],
    })
    expect(button('Freeze at playhead').disabled).toBe(true)
    expect(target.textContent).toContain('browser playbackRate — приближённый preview')

    await unmount(component)
  })

  it('offers optical flow only after the selected visible video clip is slow motion', async () => {
    const component = mount(CompositionInspector, { target })
    await tick()

    const interpolation = control<HTMLSelectElement>('select[aria-label="Интерполяция кадров"]')
    expect(interpolation.querySelector<HTMLOptionElement>('option[value="optical_flow"]')?.disabled).toBe(true)
    change(control('input[aria-label="Скорость клипа"]'), '0.5')
    await tick()
    expect(interpolation.querySelector<HTMLOptionElement>('option[value="optical_flow"]')?.disabled).toBe(false)
    change(interpolation, 'optical_flow')
    await tick()

    const selected = compositionState.document.tracks
      .flatMap((track) => track.kind === 'video' ? track.clips : [])
      .find((candidate) => candidate.id === compositionState.ui.selectedClipId)
    expect(selected).toMatchObject({ speed: 0.5, frameInterpolation: 'optical_flow' })
    expect(target.textContent).toContain('server capability')
    expect(target.textContent).toContain('только в экспорте')

    await unmount(component)
  })

  it('authors reverse and freezes the clip-local source frame while disabling embedded audio', async () => {
    const component = mount(CompositionInspector, { target })
    await tick()

    const playback = control<HTMLSelectElement>('select[aria-label="Режим воспроизведения"]')
    change(playback, 'reverse')
    await tick()
    let selected = compositionState.document.tracks
      .flatMap((candidate) => candidate.kind === 'video' ? candidate.clips : [])
      .find((candidate) => candidate.id === compositionState.ui.selectedClipId)
    expect(selected).toMatchObject({ playbackMode: { mode: 'reverse' }, sourceAudioEnabled: true })

    change(playback, 'forward')
    setCompositionPlayhead(5 * COMPOSITION_TIME_BASE)
    await tick()
    button('Freeze at playhead').click()
    await tick()

    selected = compositionState.document.tracks
      .flatMap((candidate) => candidate.kind === 'video' ? candidate.clips : [])
      .find((candidate) => candidate.id === compositionState.ui.selectedClipId)
    expect(selected).toMatchObject({
      playbackMode: { mode: 'freeze', sourceTick: 5 * COMPOSITION_TIME_BASE },
      sourceAudioEnabled: false,
    })
    expect(control<HTMLInputElement>('input[aria-label="Использовать встроенный звук"]').disabled).toBe(true)
    expect(control<HTMLSelectElement>('select[aria-label="Интерполяция кадров"]')
      .querySelector<HTMLOptionElement>('option[value="optical_flow"]')?.disabled).toBe(true)
    expect(target.textContent).toContain('seek approximation')
    expect(target.textContent).toContain('Source: 5 с')

    await unmount(component)
  })

  it('authors deterministic deshake radii and explains capability/freeze constraints', async () => {
    const component = mount(CompositionInspector, { target })
    await tick()

    const mode = control<HTMLSelectElement>('select[aria-label="Стабилизация"]')
    expect(mode.querySelector<HTMLOptionElement>('option[value="deshake"]')?.disabled).toBe(false)
    change(mode, 'deshake')
    await tick()
    change(control<HTMLSelectElement>('select[aria-label="Радиус стабилизации X"]'), '16')
    change(control<HTMLSelectElement>('select[aria-label="Радиус стабилизации Y"]'), '64')
    await tick()

    const selected = compositionState.document.tracks
      .flatMap((candidate) => candidate.kind === 'video' ? candidate.clips : [])
      .find((candidate) => candidate.id === compositionState.ui.selectedClipId)
    expect(selected).toMatchObject({ stabilization: { mode: 'deshake', radiusX: 16, radiusY: 64 } })
    expect(button('Freeze at playhead').disabled).toBe(true)
    expect(target.textContent).toContain('canvas-preview их не симулирует')

    change(mode, 'disabled')
    legacyState.capabilities = {
      ...legacyState.capabilities!,
      features: [
        { id: 'composition-v1', label: 'Composition v1', available: true },
        { id: 'stabilization', label: 'Stabilization', available: false, reason: 'deshake filter missing' },
      ],
    }
    await tick()
    expect(mode.querySelector<HTMLOptionElement>('option[value="deshake"]')?.disabled).toBe(true)
    expect(target.textContent).toContain('deshake filter missing')

    await unmount(component)
  })

  it('selects all accessible delivery profiles and shows truthful codec/capability state', async () => {
    const component = mount(CompositionInspector, { target })
    await tick()

    const profile = control<HTMLSelectElement>('select[aria-label="Delivery profile"]')
    expect(profile.options).toHaveLength(12)
    expect(profile.value).toBe('mp4-h264')
    expect(target.textContent).toContain('Файл .mp4 · video H.264 · audio AAC')

    change(profile, 'webm-av1')
    await tick()
    expect(compositionState.export.profile).toEqual({ container: 'webm', codec: 'av1' })
    expect(target.textContent).toContain('Файл .webm · video AV1 · audio Opus')
    expect(target.textContent).toContain('composition-webm-av1')
    expect(button('Экспорт .webm').disabled).toBe(true)

    legacyState.capabilities = {
      ...legacyState.capabilities!,
      features: [
        ...legacyState.capabilities!.features!,
        { id: 'composition-webm-av1', label: 'WebM AV1', available: false, reason: 'AV1 unavailable' },
      ],
    }
    await tick()
    expect(target.textContent).toContain('AV1 unavailable')

    legacyState.capabilities = {
      ...legacyState.capabilities,
      features: legacyState.capabilities.features!.map((feature) =>
        feature.id === 'composition-webm-av1' ? { ...feature, available: true, reason: '' } : feature,
      ),
    }
    change(control<HTMLSelectElement>('select[aria-label="Качество delivery"]'), 'compact')
    await tick()
    expect(compositionState.export.qualityTier).toBe('compact')
    expect(button('Экспорт .webm').disabled).toBe(false)
    expect(button('Экспорт .webm').title).toContain('WebM · AV1 / Opus (.webm)')

    change(profile, 'mov-prores-hq')
    await tick()
    expect(compositionState.export.profile).toEqual({ container: 'mov', profile: 'hq' })
    expect(target.textContent).toContain('Файл .mov · video ProRes HQ · audio PCM')

    change(profile, 'audio-wav')
    await tick()
    expect(compositionState.export.profile).toEqual({ container: 'audio', codec: 'wav' })
    expect(target.textContent).toContain('Файл .wav · video None · audio PCM s16le')
    expect(button('Экспорт .wav').disabled).toBe(false)

    change(profile, 'audio-aac')
    await tick()
    expect(compositionState.export.profile).toEqual({ container: 'audio', codec: 'aac' })
    expect(target.textContent).toContain('Файл .aac · video None · audio AAC')
    expect(button('Экспорт .aac').disabled).toBe(false)

    change(profile, 'audio-flac')
    await tick()
    expect(compositionState.export.profile).toEqual({ container: 'audio', codec: 'flac' })
    expect(target.textContent).toContain('Файл .flac · video None · audio FLAC')
    expect(button('Экспорт .flac').disabled).toBe(false)

    await unmount(component)
  })

  it('authors custom MP4/WebM bitrate and disables incompatible quality control', async () => {
    const component = mount(CompositionInspector, { target })
    await tick()

    const toggle = control<HTMLInputElement>('input[aria-label="Custom video bitrate"]')
    toggle.checked = true
    toggle.dispatchEvent(new Event('change', { bubbles: true }))
    await tick()
    const bitrate = control<HTMLInputElement>('input[aria-label="Video bitrate Kbps"]')
    expect(bitrate.value).toBe('12000')
    expect(control<HTMLSelectElement>('select[aria-label="Качество delivery"]').disabled).toBe(true)
    change(bitrate, '18000')
    await tick()
    expect(compositionState.export.videoBitrateKbps).toBe(18_000)

    change(control<HTMLSelectElement>('select[aria-label="Delivery profile"]'), 'mov-prores-hq')
    await tick()
    expect(compositionState.export.videoBitrateKbps).toBeNull()
    expect(target.querySelector('input[aria-label="Custom video bitrate"]')).toBeNull()
    await unmount(component)
  })

  it('authors a target file size with derived bitrate hint and excludes custom bitrate', async () => {
    const component = mount(CompositionInspector, { target })
    await tick()

    const bitrateToggle = control<HTMLInputElement>('input[aria-label="Custom video bitrate"]')
    bitrateToggle.checked = true
    bitrateToggle.dispatchEvent(new Event('change', { bubbles: true }))
    await tick()
    expect(compositionState.export.videoBitrateKbps).toBe(12_000)

    const sizeToggle = control<HTMLInputElement>('input[aria-label="Target file size"]')
    sizeToggle.checked = true
    sizeToggle.dispatchEvent(new Event('change', { bubbles: true }))
    await tick()
    expect(compositionState.export.targetSizeBytes).toBe(25_000_000)
    expect(compositionState.export.videoBitrateKbps).toBeNull()
    expect(sizeToggle.checked).toBe(true)
    expect(bitrateToggle.checked).toBe(false)
    expect(control<HTMLInputElement>('input[aria-label="Target file size megabytes"]').value).toBe('25')
    expect(control<HTMLSelectElement>('select[aria-label="Качество delivery"]').disabled).toBe(true)
    expect(target.textContent).toContain('Видео ≈ 19808 Kbps на 10.0 s')

    change(control<HTMLInputElement>('input[aria-label="Target file size megabytes"]'), '40')
    await tick()
    expect(compositionState.export.targetSizeBytes).toBe(40_000_000)
    expect(compositionRenderOutput()).toEqual({
      profile: { container: 'mp4', codec: 'h264' },
      qualityTier: 'medium',
      targetSizeBytes: 40_000_000,
    })

    bitrateToggle.checked = true
    bitrateToggle.dispatchEvent(new Event('change', { bubbles: true }))
    await tick()
    expect(compositionState.export.videoBitrateKbps).toBe(12_000)
    expect(compositionState.export.targetSizeBytes).toBeNull()
    expect(target.querySelector('input[aria-label="Target file size megabytes"]')).toBeNull()

    change(control<HTMLSelectElement>('select[aria-label="Delivery profile"]'), 'mov-prores-hq')
    await tick()
    expect(compositionState.export.videoBitrateKbps).toBeNull()
    expect(target.querySelector('input[aria-label="Target file size"]')).toBeNull()
    await unmount(component)
  })

  it('authors accessible visual keyframes and Rectangle/Ellipse/Linear masks for an overlay', async () => {
    addCompositionTrack('video')
    const overlayId = addMediaInfoToComposition({
      id: 'inspector-overlay',
      url: '/files/sources/overlay.mp4',
      filename: 'overlay.mp4',
      mediaType: 'video',
      duration: 3,
      width: 640,
      height: 360,
      acodec: null,
    })
    setCompositionPlayhead(COMPOSITION_TIME_BASE)
    const component = mount(CompositionInspector, { target })
    await tick()

    expect(control<HTMLSelectElement>('select[aria-label="Параметр keyframes"]')).toBeTruthy()
    expect(target.querySelector('svg[role="img"]')?.getAttribute('aria-label')).toContain('График X')
    button('+ Ключ на playhead').click()
    await tick()
    setCompositionPlayhead(2 * COMPOSITION_TIME_BASE)
    await tick()
    button('+ Ключ на playhead').click()
    await tick()
    change(control('select[aria-label="Интерполяция keyframes"]'), 'ease_out')
    const values = target.querySelectorAll<HTMLInputElement>('input[aria-label="Значение keyframe X"]')
    expect(values).toHaveLength(2)
    change(values[1]!, '125')
    const deleteButtons = target.querySelectorAll<HTMLButtonElement>('button[aria-label^="Удалить keyframe X"]')
    deleteButtons[0]!.click()
    await tick()

    button('+ Rectangle').click()
    await tick()
    change(control('input[aria-label="Mask 1 X"]'), '0.65')
    change(control('input[aria-label="Mask 1 feather"]'), '0.2')
    control<HTMLInputElement>('input[aria-label="Инвертировать mask 1"]').click()
    await tick()

    const overlay = compositionState.document.tracks
      .flatMap((track) => track.kind === 'video' ? track.clips : [])
      .find((candidate) => candidate.id === overlayId)
    expect(overlay?.animation?.x).toEqual({
      mode: 'keyframes',
      track: {
        timeBase: COMPOSITION_TIME_BASE,
        interpolation: 'ease_out',
        keyframes: [{ tick: 2 * COMPOSITION_TIME_BASE, value: 125 }],
      },
    })
    expect(overlay?.masks?.[0]).toMatchObject({
      shape: 'rectangle',
      x: { mode: 'constant', value: 0.65 },
      feather: 0.2,
      inverted: true,
    })
    change(control<HTMLInputElement>('input[aria-label="Mask 1 angle"]'), '30')
    const rotatedRectangleWire = buildCompositionRenderRequest(compositionState.document).composition.tracks
      .flatMap((candidate) => candidate.kind === 'video' ? candidate.clips : [])
      .find((candidate) => candidate.id === overlayId)?.effects[0]
    expect(rotatedRectangleWire).toMatchObject({ kind: 'mask', shape: 'rectangle', rotationDegrees: { mode: 'constant', value: 30 } })
    expect([...control<HTMLSelectElement>('select[aria-label="Форма mask 1"]').options].some((option) => option.value === 'linear')).toBe(true)
    change(control<HTMLSelectElement>('select[aria-label="Форма mask 1"]'), 'linear')
    await tick()
    expect([...control<HTMLSelectElement>('select[aria-label="Параметр mask keyframes"]').options].map((option) => option.value)).toEqual(['x', 'y', 'rotationDegrees'])
    change(control<HTMLInputElement>('input[aria-label="Mask 1 angle"]'), '-45')
    const linearWire = buildCompositionRenderRequest(compositionState.document).composition.tracks
      .flatMap((candidate) => candidate.kind === 'video' ? candidate.clips : [])
      .find((candidate) => candidate.id === overlayId)?.effects[0]
    expect(linearWire).toMatchObject({ kind: 'linear_mask', rotationDegrees: { mode: 'constant', value: -45 }, feather: 0.2, inverted: true })
    expect(target.textContent).toContain('Feather и inverted-контур показываются точно только в экспорте')

    await unmount(component)
  })
})


it('edits signed and trailing authored key times through the inspector without clamping', async () => {
  const id = compositionState.ui.selectedClipId!
  applyCompositionVisualAnimation(id, { scaleX: { mode: 'keyframes', track: { timeBase: COMPOSITION_TIME_BASE, interpolation: 'linear', keyframes: [{ tick: 0, value: 1 }, { tick: 2_000_000, value: 2 }] } } })
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  const component = mount(CompositionInspector, { target })
  await tick()
  change(control('select[aria-label="Параметр keyframes"]'), 'scaleX')
  await tick()
  let times = [...target.querySelectorAll<HTMLInputElement>('input[aria-label="Время keyframe Scale X"]')]
  expect(times[0]!.hasAttribute('min')).toBe(false)
  expect(times[0]!.hasAttribute('max')).toBe(false)
  change(times[0]!, '-0.25')
  await tick()
  times = [...target.querySelectorAll<HTMLInputElement>('input[aria-label="Время keyframe Scale X"]')]
  change(times[1]!, '8')
  await tick()
  const clip = compositionState.document.tracks.flatMap((track) => [...track.clips]).find((clip) => clip.id === id)!
  if (clip.kind === 'audio') throw new Error('visual required')
  expect(clip.animation?.scaleX).toMatchObject({ track: { keyframes: [{ tick: -250_000 }, { tick: 8_000_000 }] } })
  expect(compositionState.history.past.length).toBe(depth + 2)
  undoComposition(); undoComposition()
  expect(JSON.stringify(compositionState.document)).toBe(before)
  await unmount(component)
})


it('normalizes a linked Scale row collision in one action and restores both keys with Undo', async () => {
  const id = compositionState.ui.selectedClipId!
  applyCompositionVisualAnimation(id, { scaleX: { mode: 'keyframes', track: { timeBase: COMPOSITION_TIME_BASE, interpolation: 'linear', keyframes: [{ tick: 0, value: 1, interpolation: 'ease_in' }, { tick: 1_000_000, value: 2, interpolation: 'hold' }] } } })
  setCompositionScaleLinked(id, true)
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  const component = mount(CompositionInspector, { target })
  await tick()
  change(control('select[aria-label="Параметр keyframes"]'), 'scaleX')
  await tick()
  const times = [...target.querySelectorAll<HTMLInputElement>('input[aria-label="Время keyframe Scale X"]')]
  change(times[1]!, '0')
  await tick()
  const clip = compositionState.document.tracks.flatMap((track) => [...track.clips]).find((clip) => clip.id === id)!
  if (clip.kind === 'audio') throw new Error('visual required')
  expect(clip.animation?.scaleX).toMatchObject({ track: { keyframes: [{ tick: 0, value: 2, interpolation: 'hold' }] } })
  expect(clip.animation?.scaleY).toEqual(clip.animation?.scaleX)
  expect(clip.scaleLinked).toBe(true)
  expect(compositionState.history.past.length).toBe(depth + 1)
  undoComposition()
  expect(JSON.stringify(compositionState.document)).toBe(before)
  await unmount(component)
})


it('authors independent continuation sides for linked Scale and exports the metadata', async () => {
  const id = compositionState.ui.selectedClipId!
  applyCompositionVisualAnimation(id, { scaleX: { mode: 'keyframes', track: { timeBase: COMPOSITION_TIME_BASE, interpolation: 'linear', keyframes: [{ tick: 1_000_000, value: 1 }, { tick: 2_000_000, value: 2 }] } } })
  setCompositionScaleLinked(id, true)
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  const component = mount(CompositionInspector, { target })
  await tick()
  change(control('select[aria-label="Параметр keyframes"]'), 'scaleX')
  await tick()
  change(control('select[aria-label="Продолжение после ключей"]'), 'continue')
  await tick()
  const clip = compositionState.document.tracks.flatMap((track) => [...track.clips]).find((clip) => clip.id === id)!
  if (clip.kind === 'audio') throw new Error('visual required')
  expect(clip.animation?.scaleX).toMatchObject({ track: { extrapolation: { before: 'hold', after: 'continue' } } })
  expect(clip.animation?.scaleY).toEqual(clip.animation?.scaleX)
  expect(clip.scaleLinked).toBe(true)
  const request = buildCompositionRenderRequest(compositionState.document, compositionRenderOutput())
  const wire = request.composition.tracks.flatMap((track) => track.kind === 'video' ? track.clips : []).find((clip) => clip.id === id)!
  expect(wire.transform.scaleX).toMatchObject({ track: { extrapolation: { before: 'hold', after: 'continue' } } })
  expect(compositionState.history.past.length).toBe(depth + 1)
  undoComposition()
  expect(JSON.stringify(compositionState.document)).toBe(before)
  await unmount(component)
})

it.each(['loop', 'ping_pong', 'offset'] as const)('authors %s repetition for linked Scale with exported metadata and exact Undo', async (mode) => {
  const id = compositionState.ui.selectedClipId!
  applyCompositionVisualAnimation(id, { scaleX: { mode: 'keyframes', track: { timeBase: COMPOSITION_TIME_BASE, interpolation: 'linear', keyframes: [{ tick: 1_000_000, value: 1 }, { tick: 2_000_000, value: 2 }] } } })
  setCompositionScaleLinked(id, true)
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  const component = mount(CompositionInspector, { target })
  await tick()
  change(control('select[aria-label="Параметр keyframes"]'), 'scaleX')
  await tick()
  change(control('select[aria-label="Продолжение после ключей"]'), mode)
  await tick()
  const clip = compositionState.document.tracks.flatMap((track) => [...track.clips]).find((clip) => clip.id === id)!
  if (clip.kind === 'audio') throw new Error('visual required')
  expect(clip.animation?.scaleX).toMatchObject({ track: { extrapolation: { before: 'hold', after: mode } } })
  expect(clip.animation?.scaleY).toEqual(clip.animation?.scaleX)
  expect(clip.scaleLinked).toBe(true)
  const request = buildCompositionRenderRequest(compositionState.document, compositionRenderOutput())
  const wire = request.composition.tracks.flatMap((track) => track.kind === 'video' ? track.clips : []).find((clip) => clip.id === id)!
  expect(wire.transform.scaleX).toMatchObject({ track: { extrapolation: { before: 'hold', after: mode } } })
  expect(compositionState.history.past.length).toBe(depth + 1)
  undoComposition()
  expect(JSON.stringify(compositionState.document)).toBe(before)
  await unmount(component)
})

it.each(['music', 'sfx', 'voiceover'] as const)('assigns audio role %s in one action with exact Undo', async role => {
  const id = addMediaInfoToComposition({ id: 'role-audio', url: '/files/sources/role.wav', filename: 'role.wav', mediaType: 'audio', duration: 8, width: 0, height: 0, acodec: 'pcm_s16le' })
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  const component = mount(CompositionInspector, { target })
  await tick()
  const select = control<HTMLSelectElement>('select[aria-label="Роль аудиоклипа"]')
  expect(select.value).toBe('dialogue')
  change(select, role)
  await tick()
  const clip = compositionState.document.tracks.flatMap(track => [...track.clips]).find(clip => clip.id === id)!
  expect(clip).toMatchObject({ role })
  expect(JSON.parse(JSON.stringify(compositionState.document)).tracks.flatMap((track: { clips: { id: string; role?: string }[] }) => track.clips).find((clip: { id: string }) => clip.id === id).role).toBe(role)
  expect(compositionState.history.past.length).toBe(depth + 1)
  undoComposition()
  expect(JSON.stringify(compositionState.document)).toBe(before)
  await unmount(component)
})

it('keeps role mute/solo preferences through gain Undo/Redo without recording flag actions', async () => {
  const component = mount(CompositionInspector, { target })
  await tick()
  const depth = compositionState.history.past.length
  change(control('input[aria-label="Громкость роли music"]'), '-6')
  await tick()
  expect(compositionState.history.past.length).toBe(depth + 1)
  const mute = control<HTMLInputElement>('input[aria-label="Mute роли music"]')
  mute.checked = true
  mute.dispatchEvent(new Event('change', { bubbles: true }))
  const solo = control<HTMLInputElement>('input[aria-label="Solo роли music"]')
  solo.checked = true
  solo.dispatchEvent(new Event('change', { bubbles: true }))
  await tick()
  expect(compositionState.history.past.length).toBe(depth + 1)
  undoComposition()
  expect(compositionState.document.audioRoles?.music).toEqual({ gainDb: 0, muted: true, solo: true })
  await tick()
  const restoredMute = control<HTMLInputElement>('input[aria-label="Mute роли music"]')
  restoredMute.checked = false
  restoredMute.dispatchEvent(new Event('change', { bubbles: true }))
  expect(compositionState.history.future.length).toBe(1)
  redoComposition()
  expect(compositionState.document.audioRoles?.music).toEqual({ gainDb: -6, muted: false, solo: true })
  await unmount(component)
})


it('matches role mixer dB limits and keeps fractional gain with undo', async () => {
  const component = mount(CompositionInspector, { target })
  await tick()
  const gain = () => control<HTMLInputElement>('input[aria-label="Громкость роли music"]')
  expect(gain().min).toBe('-30')
  expect(gain().max).toBe('20')
  expect(gain().step).toBe('0.5')
  change(gain(), '6.5')
  await tick()
  expect(compositionState.document.audioRoles?.music?.gainDb).toBe(6.5)
  change(gain(), '1000')
  await tick()
  expect(compositionState.document.audioRoles?.music?.gainDb).toBe(20)
  change(gain(), '-1000')
  await tick()
  expect(compositionState.document.audioRoles?.music?.gainDb).toBe(-30)
  undoComposition()
  expect(compositionState.document.audioRoles?.music?.gainDb).toBe(20)
  undoComposition()
  expect(compositionState.document.audioRoles?.music?.gainDb).toBe(6.5)
  await unmount(component)
})


it('mutes preview output without changing authored mix or undo history', async () => {
  const component = mount(CompositionInspector, { target })
  await tick()
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  const mute = control<HTMLInputElement>('input[aria-label="Отключить звук предпросмотра"]')
  mute.checked = true
  mute.dispatchEvent(new Event('change', { bubbles: true }))
  await tick()
  expect(mute.checked).toBe(true)
  expect(JSON.stringify(compositionState.document)).toBe(before)
  expect(compositionState.history.past.length).toBe(depth)
  mute.checked = false
  mute.dispatchEvent(new Event('change', { bubbles: true }))
  await tick()
  expect(JSON.stringify(compositionState.document)).toBe(before)
  await unmount(component)
})

it('auditions role fader without writes, commits once, and cancels Escape release', async () => {
  const { previewAudioState, previewAudioRoleGain } = await import('$lib/audio/previewAudio.svelte')
  const component = mount(CompositionInspector, { target })
  await tick()
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  const slider = () => control<HTMLInputElement>('input[aria-label="Прослушать громкость роли music"]')
  slider().value = '6'
  slider().dispatchEvent(new Event('input', { bubbles: true }))
  await tick()
  expect(previewAudioState.roleGains.music).toBe(6)
  expect(previewAudioRoleGain(compositionState.document, 'music')).toBeCloseTo(10 ** (6 / 20))
  const { audioRoleGain } = await import('$lib/composition/audioRoles')
  expect(audioRoleGain(compositionState.document, 'music')).toBe(1)
  expect(previewAudioRoleGain({ ...compositionState.document, audioRoles: { music: { gainDb: 0, muted: true, solo: true } } }, 'music')).toBe(0)
  expect(JSON.stringify(compositionState.document)).toBe(before)
  expect(compositionState.history.past.length).toBe(depth)
  slider().dispatchEvent(new Event('change', { bubbles: true }))
  await tick()
  expect(compositionState.document.audioRoles?.music?.gainDb).toBe(6)
  expect(previewAudioState.roleGains.music).toBeUndefined()
  expect(compositionState.history.past.length).toBe(depth + 1)
  undoComposition()
  expect(JSON.stringify(compositionState.document)).toBe(before)
  await tick()
  slider().value = '10'
  slider().dispatchEvent(new Event('input', { bubbles: true }))
  slider().dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
  slider().dispatchEvent(new Event('change', { bubbles: true }))
  await tick()
  expect(previewAudioState.roleGains.music).toBeUndefined()
  expect(JSON.stringify(compositionState.document)).toBe(before)
  expect(compositionState.history.past.length).toBe(depth)
  slider().value = '12'
  slider().dispatchEvent(new Event('input', { bubbles: true }))
  await unmount(component)
  expect(previewAudioState.roleGains).toEqual({})
})

it('clears audition on numeric commit and resets role gain without changing mute/solo', async () => {
  const { previewAudioState } = await import('$lib/audio/previewAudio.svelte')
  const component = mount(CompositionInspector, { target })
  await tick()
  const depth = compositionState.history.past.length
  const slider = control<HTMLInputElement>('input[aria-label="Прослушать громкость роли music"]')
  slider.value = '6'
  slider.dispatchEvent(new Event('input', { bubbles: true }))
  await tick()
  change(control('input[aria-label="Громкость роли music"]'), '-3')
  await tick()
  expect(previewAudioState.roleGains.music).toBeUndefined()
  expect(compositionState.document.audioRoles?.music?.gainDb).toBe(-3)
  expect(compositionState.history.past.length).toBe(depth + 1)
  const mute = control<HTMLInputElement>('input[aria-label="Mute роли music"]')
  mute.checked = true
  mute.dispatchEvent(new Event('change', { bubbles: true }))
  const solo = control<HTMLInputElement>('input[aria-label="Solo роли music"]')
  solo.checked = true
  solo.dispatchEvent(new Event('change', { bubbles: true }))
  target.querySelector<HTMLButtonElement>('button[aria-label="Сбросить громкость роли music"]')!.click()
  await tick()
  expect(compositionState.document.audioRoles?.music).toEqual({ gainDb: 0, muted: true, solo: true })
  expect(compositionState.history.past.length).toBe(depth + 2)
  undoComposition()
  expect(compositionState.document.audioRoles?.music).toEqual({ gainDb: -3, muted: true, solo: true })
  await unmount(component)
})

it('cancels numeric role edits with Escape and commits Enter once before blur', async () => {
  const component = mount(CompositionInspector, { target })
  await tick()
  const before = JSON.stringify(compositionState.document)
  const depth = compositionState.history.past.length
  const input = control<HTMLInputElement>('input[aria-label="Громкость роли music"]')
  input.value = '9'
  input.dispatchEvent(new Event('input', { bubbles: true }))
  input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
  input.dispatchEvent(new Event('change', { bubbles: true }))
  await tick()
  expect(input.value).toBe('0')
  expect(JSON.stringify(compositionState.document)).toBe(before)
  expect(compositionState.history.past.length).toBe(depth)
  input.value = '-4.5'
  input.dispatchEvent(new Event('input', { bubbles: true }))
  input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
  input.dispatchEvent(new Event('change', { bubbles: true }))
  await tick()
  expect(compositionState.document.audioRoles?.music?.gainDb).toBe(-4.5)
  expect(compositionState.history.past.length).toBe(depth + 1)
  input.value = ''
  input.dispatchEvent(new Event('input', { bubbles: true }))
  input.dispatchEvent(new Event('change', { bubbles: true }))
  await tick()
  expect(compositionState.document.audioRoles?.music?.gainDb).toBe(-4.5)
  expect(compositionState.history.past.length).toBe(depth + 1)
  undoComposition()
  expect(JSON.stringify(compositionState.document)).toBe(before)
  await unmount(component)
})

it('explains implied role mute from another solo without marking explicitly muted roles', async () => {
  const component = mount(CompositionInspector, { target })
  await tick()
  const depth = compositionState.history.past.length
  const solo = control<HTMLInputElement>('input[aria-label="Solo роли music"]')
  solo.checked = true
  solo.dispatchEvent(new Event('change', { bubbles: true }))
  await tick()
  expect(target.querySelector('[aria-label="Роль dialogue заглушена Solo другой роли"]')).not.toBeNull()
  expect(target.querySelector('[aria-label="Роль music заглушена Solo другой роли"]')).toBeNull()
  const dialogueMute = control<HTMLInputElement>('input[aria-label="Mute роли dialogue"]')
  dialogueMute.checked = true
  dialogueMute.dispatchEvent(new Event('change', { bubbles: true }))
  await tick()
  expect(target.querySelector('[aria-label="Роль dialogue заглушена Solo другой роли"]')).toBeNull()
  expect(target.querySelector('[aria-label="Роль sfx заглушена Solo другой роли"]')).not.toBeNull()
  solo.checked = false
  solo.dispatchEvent(new Event('change', { bubbles: true }))
  await tick()
  expect(target.querySelector('[aria-label="Роль sfx заглушена Solo другой роли"]')).toBeNull()
  expect(compositionState.history.past.length).toBe(depth)
  await unmount(component)
})

it('creates and edits a source-free color layer through the inspector with Undo/Redo', async () => {
  const component = mount(CompositionInspector, { target })
  await tick()
  button('+ Цвет').click()
  await tick()
  const track = compositionState.document.tracks.find(candidate => candidate.id === compositionState.ui.selectedTrackId)!
  expect(track.kind).toBe('color')
  const id = compositionState.ui.selectedClipId!
  const current = () => compositionState.document.tracks.flatMap<CompositionClip>(candidate => candidate.clips).find(candidate => candidate.id === id)!
  expect(current()).toMatchObject({ kind: 'color',color: '#000000ff',timelineStartTicks: 4 * COMPOSITION_TIME_BASE })
  expect(current()).not.toHaveProperty('sourceId')
  change(control('input[aria-label="Цвет слоя RGBA"]'), '#12345680')
  await tick()
  expect(current()).toMatchObject({ color: '#12345680' })
  undoComposition()
  await tick()
  expect(current()).toMatchObject({ color: '#000000ff' })
  redoComposition()
  await tick()
  expect(current()).toMatchObject({ color: '#12345680' })
  change(control('input[aria-label="Исходная ширина цвета"]'), '63')
  change(control('input[aria-label="Исходная высота цвета"]'), '31')
  change(control('input[aria-label="Позиция X"]'), '12')
  change(control('input[aria-label="Прозрачность слоя"]'), '0.5')
  await tick()
  expect(current()).toMatchObject({ width: 63,height: 31,transform: { x: 12 },opacity: .5 })
  const beforeInvalid = compositionState.document
  change(control('input[aria-label="Цвет слоя RGBA"]'), '#gg0000')
  await tick()
  expect(compositionState.document).toBe(beforeInvalid)
  change(control('input[aria-label="Цвет нового ключа"]'), '#ff000080')
  button('Ключ цвета на текущем кадре').click()
  await tick()
  expect(current()).toMatchObject({ colorAnimation: { mode: 'keyframes',track: { keyframes: [{ tick: 0,value: '#ff000080' }] } } })
  undoComposition()
  await tick()
  expect(current()).not.toHaveProperty('colorAnimation')
  redoComposition()
  await tick()
  expect(current()).toMatchObject({ colorAnimation: { mode: 'keyframes' } })
  await unmount(component)
})

it('persists audio transition curve selection through undo, redo and render payload', async () => {
  addMediaInfoToComposition({ id: 'curve-audio', url: '/files/sources/curve.wav', filename: 'curve.wav', mediaType: 'audio', duration: 4, width: 0, height: 0, acodec: 'pcm_s16le' })
  const id = compositionState.ui.selectedClipId!
  const current = () => compositionState.document.tracks.flatMap(track => [...track.clips] as CompositionClip[]).find(clip => clip.id === id)!
  const component = mount(CompositionInspector, { target })
  await tick()
  const curve = control<HTMLSelectElement>('select[aria-label="Audio crossfade curve"]')
  expect([...curve.options].map(option => option.value)).toEqual(['constant_gain', 'constant_power', 'exponential_fade'])
  for (const value of ['constant_power', 'exponential_fade'] as const) {
    const before = current()
    change(curve, value)
    await tick()
    expect(current()).toMatchObject({ crossfadeCurve: value })
    const request = buildCompositionRenderRequest(compositionState.document)
    const audio = request.composition.tracks.find(track => track.kind === 'audio')
    if (!audio || audio.kind !== 'audio') throw new Error('Audio track missing')
    expect(audio.clips.find(clip => clip.id === id)?.crossfadeCurve).toBe(value)
    undoComposition()
    await tick()
    expect(current()).toEqual(before)
    redoComposition()
    await tick()
    expect(current()).toMatchObject({ crossfadeCurve: value })
    expect(control<HTMLSelectElement>('select[aria-label="Audio crossfade curve"]').value).toBe(value)
  }
  await unmount(component)
})

it('creates a source-free graphic clip and edits its stable layer stack with undo', async () => {
  const component = mount(CompositionInspector, { target })
  await tick()
  button('+ Графика').click()
  await tick()
  const graphic = () => compositionState.document.tracks.flatMap<CompositionClip>(track => track.clips).find(clip => clip.id === compositionState.ui.selectedClipId)
  expect(graphic()).toMatchObject({ kind: 'graphic', graphic: { layers: [{ uid: 1, content: { kind: 'text' } }] } })
  expect(graphic()).not.toHaveProperty('sourceId')
  button('+ Фигура').click()
  await tick()
  expect(graphic()).toMatchObject({ graphic: { layers: [{ uid: 1 }, { uid: 2, content: { kind: 'shape' } }] } })
  target.querySelector<HTMLButtonElement>('[aria-label="Слой 1 вперёд"]')!.click()
  await tick()
  expect(graphic()).toMatchObject({ graphic: { layers: [{ uid: 2 }, { uid: 1 }] } })
  undoComposition()
  await tick()
  expect(graphic()).toMatchObject({ graphic: { layers: [{ uid: 1 }, { uid: 2 }] } })
  redoComposition()
  await tick()
  expect(graphic()).toMatchObject({ graphic: { layers: [{ uid: 2 }, { uid: 1 }] } })
  const wire = buildCompositionRenderRequest(compositionState.document).composition.tracks.find(track => track.kind === 'graphic')
  expect(wire).toMatchObject({ kind: 'graphic', clips: [{ graphic: { layers: [{ uid: 2 }, { uid: 1 }] } }] })
  await unmount(component)
})

it.each(['film_dissolve','additive_dissolve','fade_white'] as const)('authors independent %s through inspector and undo/redo',async kind=>{
 const component=mount(CompositionInspector,{target});await tick()
 change(control('select[aria-label="Тип перехода"]'),kind)
 change(control('input[aria-label="Длительность перехода"]'),'0.5');await tick()
 expect(button('Добавить переход').disabled).toBe(false)
 button('Добавить переход').click();await tick()
 const transitions=()=>{const track=compositionState.document.tracks.find(track=>track.kind==='video')!;return track.transitions??[]}
 expect(transitions()).toEqual([expect.objectContaining({kind,durationTicks:0,timeline:{startTick:3750000,durationTick:500000}})])
 const snapshot=JSON.stringify(transitions())
 undoComposition();await tick();expect(transitions()).toHaveLength(0)
 redoComposition();await tick();expect(JSON.stringify(transitions())).toBe(snapshot)
 expect(target.textContent).toContain('Независимый интервал перехода')
 change(control('input[aria-label="Начало независимого перехода"]'),'-0.125')
 control<HTMLInputElement>('input[aria-label="Reverse перехода"]').click();await tick()
 button('Обновить переход').click();await tick()
 expect(transitions()).toEqual([expect.objectContaining({kind,reverse:true,timeline:{startTick:-125000,durationTick:500000}})])
 undoComposition();await tick();expect(JSON.stringify(transitions())).toBe(snapshot)
 expect(control<HTMLInputElement>('input[aria-label="Начало независимого перехода"]').value).toBe('3.75')
 expect(control<HTMLInputElement>('input[aria-label="Reverse перехода"]').checked).toBe(false)
 redoComposition();await tick();expect(transitions()[0]?.reverse).toBe(true)
 expect(control<HTMLInputElement>('input[aria-label="Начало независимого перехода"]').value).toBe('-0.125')
 await unmount(component)
})

it('authors procedural kernel controls and restores displayed values through history',async()=>{
 const component=mount(CompositionInspector,{target});await tick()
 change(control('select[aria-label="Тип перехода"]'),'procedural');await tick()
 change(control('select[aria-label="Ядро перехода"]'),'block_motion');await tick()
 change(control('input[aria-label="Параметр перехода blocks"]'),'8');await tick()
 expect(button('Добавить переход').disabled).toBe(false)
 button('Добавить переход').click();await tick()
 const transitions=()=>{const track=compositionState.document.tracks.find(track=>track.kind==='video')!;return track.transitions??[]}
 expect(transitions()[0]).toMatchObject({kind:'procedural',kernel:{effect:'block_motion',params:{blocks:{Float:8}}}})
 change(control('input[aria-label="Параметр перехода blocks"]'),'12');await tick()
 button('Обновить переход').click();await tick()
 expect(transitions()[0]?.kernel?.params?.blocks).toEqual({Float:12})
 undoComposition();await tick()
 expect(transitions()[0]?.kernel?.params?.blocks).toEqual({Float:8})
 expect(control<HTMLInputElement>('input[aria-label="Параметр перехода blocks"]').value).toBe('8')
 redoComposition();await tick()
 expect(control<HTMLInputElement>('input[aria-label="Параметр перехода blocks"]').value).toBe('12')
 await unmount(component)
})

it('rejoins detached audio from the audio inspector and undoes the operation', async () => {
  const id = compositionState.ui.selectedClipId!
  const audioId = detachCompositionVideoAudio(id)
  const before = JSON.stringify(compositionState.document)
  const component = mount(CompositionInspector, { target })
  await tick()
  const join = button('Вернуть звук в видео')
  expect(join.disabled).toBe(false)
  join.click(); await tick()
  expect(compositionState.ui.selectedClipId).toBe(id)
  expect(compositionState.document.tracks.some(t => t.clips.some(c => c.id === audioId))).toBe(false)
  undoComposition(); await tick()
  expect(JSON.stringify(compositionState.document)).toBe(before)
  await unmount(component)
})

it('authors volume keys at the cursor and carries value/custom easing into the native payload',async()=>{
 const id=addMediaInfoToComposition({id:'gain-key-audio',url:'/files/sources/keys.wav',filename:'keys.wav',mediaType:'audio',duration:1,width:0,height:0,acodec:'pcm_s16le'})
 const inserted=compositionState.document.tracks.flatMap<CompositionClip>(track=>track.clips).find(clip=>clip.id===id)!
 setCompositionPlayhead(inserted.timelineStartTicks+500000);const component=mount(CompositionInspector,{target});await tick()
 Array.from(target.querySelectorAll('button')).find(button=>button.textContent?.includes('Добавить ключ громкости в позиции курсора'))!.click();await tick()
 change(control('input[aria-label="Ключ громкости 1: значение"]'),'2');await tick()
 change(control('input[aria-label="Ключ громкости 1: y1"]'),'3');await tick()
 const audio=compositionState.document.tracks.flatMap<CompositionClip>(track=>track.clips).find(clip=>clip.id===id)!;expect(audio).toMatchObject({concatGainKeys:[{at:0.5,value:2,ease:[0,3,1,1]}]})
 expect(buildCompositionRenderRequest(compositionState.document).composition.tracks.find(track=>track.kind==='audio')).toMatchObject({clips:[{concatGainKeys:[{at:0.5,value:2,ease:[0,3,1,1]}]}]})
 if(process.env.CONCAT_VOLUME_EDITOR_KEYS_OUTPUT&&'concatGainKeys'in audio)writeFileSync(process.env.CONCAT_VOLUME_EDITOR_KEYS_OUTPUT,JSON.stringify(audio.concatGainKeys))
 undoComposition();await tick();expect(control('input[aria-label="Ключ громкости 1: y1"]').value).toBe('0');redoComposition();await tick();expect(control('input[aria-label="Ключ громкости 1: y1"]').value).toBe('3')
 await unmount(component)
})

it('authors an absolute source stream and restores it through undo/redo and payload',async()=>{
 const component=mount(CompositionInspector,{target});await tick()
 const selected=compositionState.ui.selectedClipId!
 change(control('input[aria-label="Индекс аудиопотока"]'),'2');await tick()
 const clip=()=>{const found=compositionState.document.tracks.flatMap<CompositionClip>(track=>track.clips).find(clip=>clip.id===selected)!;if(found.kind!=='video')throw new Error('video expected');return found}
 expect('audioStream'in clip()&&clip().audioStream).toBe(2)
 const payload=JSON.stringify(buildCompositionRenderRequest(compositionState.document,compositionRenderOutput()))
 expect(payload).toContain('"audioStream":2')
 undoComposition();await tick();expect('audioStream'in clip()&&clip().audioStream).not.toBe(2);redoComposition();await tick();expect(control('input[aria-label="Индекс аудиопотока"]').value).toBe('2')
 button('Использовать дорожку по умолчанию').click();await tick();expect(Object.hasOwn(clip(),'audioStream')).toBe(false)
 await unmount(component)
})

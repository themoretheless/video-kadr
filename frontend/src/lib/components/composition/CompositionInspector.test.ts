// @ts-expect-error Vitest's SSR condition exposes server-only mount; this test needs Svelte's client entry.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { COMPOSITION_TIME_BASE } from '$lib/composition/types.js'
import { buildCompositionRenderRequest } from '$lib/composition/payload.js'
import {
  addCompositionTrack,
  addTextToComposition,
  addCompositionVideoMask,
  addMediaInfoToComposition,
  compositionState,
  resetCompositionForTests,
  setCompositionPlayhead,
  splitSelectedCompositionClip,
  undoComposition,
  updateCompositionChromaKey,
} from '$lib/state/composition.svelte.js'
import { state as legacyState } from '$lib/state/store.svelte.js'
import CompositionInspector from './CompositionInspector.svelte'
import { clearSpaceBrandKit, spaceBrandState } from '$lib/state/spaceBrand.svelte.js'
import { spacesState } from '$lib/state/spaces.svelte.js'

let target: HTMLDivElement

beforeEach(() => {
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
    control<HTMLInputElement>('input[type="checkbox"]:not(:disabled)').click()
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

  it('authors audio gain, pan, and bounded fades through labelled controls', async () => {
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
    change(control('input[aria-label="Fade in"]'), '0.4')
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
      fadeInTicks: 400_000,
      fadeOutTicks: 750_000,
      voiceEffect: 'robot',
      pitchSemitones: 7,
      toneDb: 9,
      ducking: { thresholdDb: -30, ratio: 10, attackMs: 15, releaseMs: 250 },
    })
    expect(target.textContent).toContain('stereo pan')

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
    expect(target.textContent).toContain('canvas-preview его не симулирует')

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

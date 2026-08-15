// @ts-expect-error Vitest's SSR condition exposes server-only mount; this test needs Svelte's client entry.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import {
  createCompositionPreviewProxyAdapter,
  type CompositionPreviewProxyAdapter,
  type CompositionPreviewProxyControllerPort,
} from '$lib/proxy/compositionPreviewController.js'
import { createProxyController, type ProxyClient } from '$lib/proxy/state.svelte.js'
import type { ProxyList } from '$lib/proxy/types.js'
import {
  addCompositionTrack,
  addCompositionVideoMask,
  addMediaInfoToComposition,
  compositionState,
  resetCompositionForTests,
  updateCompositionBlendMode,
  updateCompositionChromaKey,
  updateCompositionClipOpacity,
  updateCompositionClipSpeed,
  updateCompositionRotation,
  updateCompositionVisualTransform,
  setCompositionPlayhead,
  setCompositionTransition,
  setCompositionVisualKeyframe,
  splitSelectedCompositionClip,
  toggleCompositionPlayback,
  updateCompositionVideoMask,
  updateCompositionFrameInterpolation,
  updateCompositionPlaybackMode,
  updateCompositionStabilization,
  updateCompositionSpeedRamp,
  freezeCompositionClipAtPlayhead,
} from '$lib/state/composition.svelte.js'
import CompositionPreview from './CompositionPreview.svelte'

let target: HTMLDivElement
let proxyAdapter: CompositionPreviewProxyAdapter

function mountPreview() {
  return mount(CompositionPreview, { target, props: { proxyAdapter } })
}

beforeEach(() => {
  resetCompositionForTests()
  addMediaInfoToComposition({
    id: 'preview-primary',
    url: '/files/sources/primary.mp4',
    filename: 'primary.mp4',
    mediaType: 'video',
    duration: 8,
    width: 1280,
    height: 720,
    acodec: 'aac',
  })
  const controller: CompositionPreviewProxyControllerPort = {
    state: { sources: {}, preferences: {} },
    ensure: () => undefined,
    setOriginal: () => undefined,
    setProxy: () => undefined,
    markPlaybackFailed: () => undefined,
  }
  proxyAdapter = createCompositionPreviewProxyAdapter(controller, 'other')
  target = document.createElement('div')
  document.body.append(target)
})

afterEach(() => {
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
  target?.remove()
})

describe('CompositionPreview static approximations', () => {
  it('seeks exact incoming head and outgoing tail handles across a transition boundary', async () => {
    setCompositionPlayhead(4_000_000)
    const incomingId = splitSelectedCompositionClip()
    const track = compositionState.document.tracks.find((candidate) => candidate.kind === 'video')!
    const [outgoing] = track.clips
    expect(incomingId).toBeTruthy()
    setCompositionTransition(track.id, outgoing!.id, incomingId!, 'dissolve', 2_000_000, 'preview-transition')
    setCompositionPlayhead(3_500_000)

    const component = mountPreview()
    await tick()
    let videos = [...target.querySelectorAll<HTMLVideoElement>('video')]
    expect(videos).toHaveLength(2)
    expect(videos[0]!.currentTime).toBeCloseTo(3.5, 4)
    expect(videos[1]!.currentTime).toBeCloseTo(3.5, 4)

    Object.defineProperty(videos[1], 'seekable', {
      configurable: true,
      value: { length: 1, start: () => 4, end: () => 8 },
    })
    videos[1]!.dispatchEvent(new Event('seeked'))
    await tick()
    expect(target.textContent).toContain('Browser seek не даёт показать source handle')
    expect(target.textContent).toContain('экспорт остаётся точным')

    setCompositionPlayhead(4_500_000)
    await tick()
    videos = [...target.querySelectorAll<HTMLVideoElement>('video')]
    expect(videos[0]!.currentTime).toBeCloseTo(4.5, 4)
    expect(videos[1]!.currentTime).toBeCloseTo(4.5, 4)

    await unmount(component)
  })

  it('uses source-sized CSS transform, blend, rotation, opacity, speed, and an export-only chroma note', async () => {
    addCompositionTrack('video')
    const overlayId = addMediaInfoToComposition({
      id: 'preview-overlay',
      url: '/files/sources/overlay.mp4',
      filename: 'overlay.mp4',
      mediaType: 'video',
      duration: 3,
      width: 640,
      height: 360,
      acodec: null,
    })
    updateCompositionVisualTransform(overlayId, { x: 64, y: -36, width: 320, height: 180 })
    updateCompositionRotation(overlayId, 15)
    updateCompositionClipOpacity(overlayId, 0.6)
    updateCompositionBlendMode(overlayId, 'screen')
    updateCompositionClipSpeed(overlayId, 2)
    updateCompositionChromaKey(overlayId, {
      enabled: true,
      color: '#00ff00',
      similarity: 0.25,
      softness: 0.1,
      spill: 0.2,
    })

    const component = mountPreview()
    await tick()

    const overlay = [...target.querySelectorAll<HTMLVideoElement>('video')]
      .find((candidate) => candidate.getAttribute('src')?.includes('overlay.mp4'))
    expect(overlay).toBeTruthy()
    expect(overlay?.style.left).toBe('55%')
    expect(overlay?.style.top).toBe('45%')
    expect(overlay?.style.width).toBe('25%')
    expect(overlay?.style.transform).toContain('rotate(15deg)')
    expect(overlay?.style.opacity).toBe('0.6')
    expect(overlay?.style.mixBlendMode).toBe('screen')
    expect(overlay?.style.objectFit).toBe('fill')
    expect(overlay?.playbackRate).toBe(2)
    expect(target.textContent).toContain('Chroma key виден только в экспорте')
    expect(compositionState.document.tracks[0]).toMatchObject({ kind: 'video' })

    await unmount(component)
  })

  it('samples visual keyframes and approximates an animated mask with CSS clip-path', async () => {
    addCompositionTrack('video')
    const overlayId = addMediaInfoToComposition({
      id: 'preview-animated-overlay',
      url: '/files/sources/animated-overlay.mp4',
      filename: 'animated-overlay.mp4',
      mediaType: 'video',
      duration: 4,
      width: 640,
      height: 360,
      acodec: null,
    })
    setCompositionPlayhead(0)
    setCompositionVisualKeyframe(overlayId, 'x', 0)
    setCompositionVisualKeyframe(overlayId, 'opacity', 0.2)
    setCompositionPlayhead(2_000_000)
    setCompositionVisualKeyframe(overlayId, 'x', 128)
    setCompositionVisualKeyframe(overlayId, 'opacity', 0.8)
    const maskId = addCompositionVideoMask(overlayId, 'ellipse')
    updateCompositionVideoMask(overlayId, maskId, { width: 0.5, height: 0.4, feather: 0.15 })
    updateCompositionClipSpeed(overlayId, 0.5)
    updateCompositionFrameInterpolation(overlayId, 'optical_flow')
    setCompositionPlayhead(1_000_000)

    const component = mountPreview()
    await tick()

    const overlay = [...target.querySelectorAll<HTMLVideoElement>('video')]
      .find((candidate) => candidate.getAttribute('src')?.includes('animated-overlay.mp4'))
    expect(overlay?.style.left).toBe('55%')
    expect(Number(overlay?.style.opacity)).toBeCloseTo(0.5)
    expect(overlay?.style.clipPath).toBe('ellipse(25% 20% at 50% 50%)')
    expect(target.textContent).toContain('Mask preview — clip-path approximation')
    expect(target.textContent).toContain('feather только в экспорте')
    expect(target.textContent).toContain('Optical flow виден точно только в экспорте')

    await unmount(component)
  })

  it('maps reverse source time by seek and holds an exact silent freeze frame', async () => {
    const clipId = compositionState.document.tracks
      .flatMap((track) => track.kind === 'video' ? track.clips : [])
      .at(0)!.id
    updateCompositionPlaybackMode(clipId, { mode: 'reverse' })
    setCompositionPlayhead(2_000_000)

    let component = mountPreview()
    await tick()
    let video = target.querySelector<HTMLVideoElement>('video[src*="primary.mp4"]')!
    expect(video.currentTime).toBeCloseTo(6, 4)
    expect(video.paused).toBe(true)
    expect(target.textContent).toContain('Reverse preview — seek approximation')
    expect(target.textContent).toContain('audio точны в экспорте')
    await unmount(component)

    updateCompositionPlaybackMode(clipId, { mode: 'forward' })
    setCompositionPlayhead(3_000_000)
    freezeCompositionClipAtPlayhead(clipId)
    setCompositionPlayhead(5_000_000)
    component = mountPreview()
    await tick()
    video = target.querySelector<HTMLVideoElement>('video[src*="primary.mp4"]')!
    expect(video.currentTime).toBe(3)
    expect(video.muted).toBe(true)
    expect(target.textContent).toContain('Freeze держит source frame')
    expect(target.textContent).toContain('embedded audio выключен')

    await unmount(component)
  })

  it('labels deshake as export-accurate without simulating it in CSS', async () => {
    const clipId = compositionState.document.tracks
      .flatMap((track) => track.kind === 'video' ? track.clips : [])
      .at(0)!.id
    updateCompositionStabilization(clipId, { mode: 'deshake', radiusX: 32, radiusY: 48 })

    const component = mountPreview()
    await tick()

    const video = target.querySelector<HTMLVideoElement>('video[src*="primary.mp4"]')!
    expect(video.style.transform).toBe('none')
    expect(target.textContent).toContain('Deshake stabilization применяется точно только в экспорте')

    await unmount(component)
  })

  it('maps ramped source time, applies instantaneous rate and honors mute audio policy', async () => {
    const clipId = compositionState.document.tracks
      .flatMap((track) => track.kind === 'video' ? track.clips : [])
      .at(0)!.id
    updateCompositionSpeedRamp(clipId, {
      interpolation: 'hold',
      points: [
        { sourceProgressTick: 0, speed: 1 },
        { sourceProgressTick: 4_000_000, speed: 2 },
        { sourceProgressTick: 8_000_000, speed: 2 },
      ],
      audioPolicy: 'mute',
    })
    setCompositionPlayhead(5_000_000)

    const component = mountPreview()
    await tick()
    const video = target.querySelector<HTMLVideoElement>('video[src*="primary.mp4"]')!
    expect(video.currentTime).toBeCloseTo(6, 4)
    expect(video.playbackRate).toBe(2)
    expect(video.muted).toBe(true)
    expect(target.textContent).toContain('source-time точен')
    expect(target.textContent).toContain('browser playbackRate приблизительный')

    await unmount(component)
  })

  it('maps an audio ramp locally and removes muted-policy audio from preview playback', async () => {
    const audioId = addMediaInfoToComposition({
      id: 'preview-audio-ramp',
      url: '/files/sources/music.wav',
      filename: 'music.wav',
      mediaType: 'audio',
      duration: 8,
      width: 0,
      height: 0,
      acodec: 'pcm_s16le',
    })
    updateCompositionSpeedRamp(audioId, {
      interpolation: 'hold',
      points: [
        { sourceProgressTick: 0, speed: 1 },
        { sourceProgressTick: 4_000_000, speed: 2 },
        { sourceProgressTick: 8_000_000, speed: 2 },
      ],
      audioPolicy: 'preserve_pitch',
    })
    setCompositionPlayhead(5_000_000)

    const component = mountPreview()
    await tick()
    let audio = target.querySelector<HTMLAudioElement>('audio[src*="music.wav"]')!
    expect(audio.currentTime).toBeCloseTo(6, 4)
    expect(audio.playbackRate).toBe(2)

    updateCompositionSpeedRamp(audioId, {
      ...compositionState.document.tracks.flatMap((track) => track.kind === 'audio' ? track.clips : [])
        .find((candidate) => candidate.id === audioId)!.speedRamp!,
      audioPolicy: 'mute',
    })
    await tick()
    audio = target.querySelector<HTMLAudioElement>('audio[src*="music.wav"]')!
    expect(audio).toBeNull()

    await unmount(component)
  })

  it('uses a ready preferred proxy and falls back to original after a media error', async () => {
    const key = 'a'.repeat(64)
    const ready: ProxyList = {
      sourceId: 'preview-primary',
      sourceFingerprint: 'b'.repeat(64),
      status: 'ready',
      proxies: [{
        key,
        profile: { maxWidth: 720, codec: 'h264', quality: 28, includeAudio: true },
        status: 'ready',
        url: `/files/proxies/${key}.mp4`,
        sizeBytes: 1024,
        sha256: 'c'.repeat(64),
      }],
      jobs: [],
    }
    const client: ProxyClient = {
      createLibraryProxy: vi.fn(),
      getLibraryProxies: vi.fn().mockResolvedValue(ready),
      deleteLibraryProxy: vi.fn(),
      pollJob: vi.fn(),
    }
    const controller = createProxyController(client, null)
    await controller.refresh('preview-primary')
    proxyAdapter = createCompositionPreviewProxyAdapter(controller, 'other')
    const clipId = compositionState.document.tracks
      .flatMap((track) => track.kind === 'video' ? track.clips : [])
      .at(0)!.id
    updateCompositionClipSpeed(clipId, 2)
    setCompositionPlayhead(1_500_000)
    const pause = vi.spyOn(HTMLMediaElement.prototype, 'pause')

    const component = mountPreview()
    await tick()
    let video = target.querySelector<HTMLVideoElement>('video')!
    expect(video.getAttribute('src')).toBe('/files/sources/primary.mp4')
    expect(video.currentTime).toBeCloseTo(3, 4)
    expect(video.playbackRate).toBe(2)
    expect(video.paused).toBe(true)

    video.currentTime = 0
    video.playbackRate = 1
    const pausesBeforeProxy = pause.mock.calls.length
    controller.setProxy('preview-primary', key)
    await tick()
    video = target.querySelector<HTMLVideoElement>('video')!
    expect(video.getAttribute('src')).toBe(`/files/proxies/${key}.mp4`)
    expect(video.currentTime).toBeCloseTo(3, 4)
    expect(video.playbackRate).toBe(2)
    expect(pause.mock.calls.length).toBeGreaterThan(pausesBeforeProxy)
    expect(target.textContent).toContain('Proxy · 720px')
    expect(target.textContent).toContain('Экспорт: Original')

    video.currentTime = 0
    video.playbackRate = 1
    const pausesBeforeMetadata = pause.mock.calls.length
    video.dispatchEvent(new Event('loadedmetadata'))
    await tick()
    expect(video.currentTime).toBeCloseTo(3, 4)
    expect(video.playbackRate).toBe(2)
    expect(pause.mock.calls.length).toBeGreaterThan(pausesBeforeMetadata)

    video.currentTime = 0
    video.playbackRate = 1
    video.dispatchEvent(new Event('error'))
    await tick()
    video = target.querySelector<HTMLVideoElement>('video')!
    expect(video.getAttribute('src')).toBe('/files/sources/primary.mp4')
    expect(video.currentTime).toBeCloseTo(3, 4)
    expect(video.playbackRate).toBe(2)
    expect(video.paused).toBe(true)
    expect(controller.state.sources['preview-primary']?.failedKey).toBe(key)
    expect(target.textContent).toContain('Original · fallback')
    expect(compositionState.media['preview-primary']?.url).toBe('/files/sources/primary.mp4')

    await unmount(component)
    pause.mockRestore()
  })

  it('resynchronizes a playing preview after relink and metadata with an unchanged playhead', async () => {
    const clipId = compositionState.document.tracks
      .flatMap((track) => track.kind === 'video' ? track.clips : [])
      .at(0)!.id
    updateCompositionClipSpeed(clipId, 2)
    setCompositionPlayhead(1_250_000)
    const play = vi.spyOn(HTMLMediaElement.prototype, 'play').mockResolvedValue(undefined)
    vi.stubGlobal('requestAnimationFrame', vi.fn(() => 17))
    vi.stubGlobal('cancelAnimationFrame', vi.fn())
    toggleCompositionPlayback()

    const component = mountPreview()
    await tick()
    let video = target.querySelector<HTMLVideoElement>('video')!
    expect(video.currentTime).toBeCloseTo(2.5, 4)
    expect(video.playbackRate).toBe(2)
    expect(play.mock.instances).toContain(video)

    video.currentTime = 0
    video.playbackRate = 1
    const playsBeforeRelink = play.mock.calls.length
    compositionState.media['preview-primary'] = {
      ...compositionState.media['preview-primary']!,
      url: '/files/sources/relinked-primary.mp4',
    }
    await tick()
    video = target.querySelector<HTMLVideoElement>('video')!
    expect(video.getAttribute('src')).toBe('/files/sources/relinked-primary.mp4')
    expect(video.currentTime).toBeCloseTo(2.5, 4)
    expect(video.playbackRate).toBe(2)
    expect(play.mock.calls.length).toBeGreaterThan(playsBeforeRelink)

    video.currentTime = 0
    video.playbackRate = 1
    const playsBeforeMetadata = play.mock.calls.length
    video.dispatchEvent(new Event('loadedmetadata'))
    await tick()
    expect(video.currentTime).toBeCloseTo(2.5, 4)
    expect(video.playbackRate).toBe(2)
    expect(play.mock.calls.length).toBeGreaterThan(playsBeforeMetadata)
    expect(compositionState.document.tracks[0]?.clips[0]).toMatchObject({ sourceId: 'preview-primary' })

    await unmount(component)
    play.mockRestore()
    vi.unstubAllGlobals()
  })

  it('keeps audible embedded source audio on original when the preferred proxy is silent', async () => {
    const key = 'd'.repeat(64)
    const ready: ProxyList = {
      sourceId: 'preview-primary',
      sourceFingerprint: 'e'.repeat(64),
      status: 'ready',
      proxies: [{
        key,
        profile: { maxWidth: 480, codec: 'h264', quality: 28, includeAudio: false },
        status: 'ready',
        url: `/files/proxies/${key}.mp4`,
        sizeBytes: 1024,
        sha256: 'f'.repeat(64),
      }],
      jobs: [],
    }
    const client: ProxyClient = {
      createLibraryProxy: vi.fn(),
      getLibraryProxies: vi.fn().mockResolvedValue(ready),
      deleteLibraryProxy: vi.fn(),
      pollJob: vi.fn(),
    }
    const controller = createProxyController(client, null)
    await controller.refresh('preview-primary')
    controller.setProxy('preview-primary', key)
    proxyAdapter = createCompositionPreviewProxyAdapter(controller, 'other')

    const component = mountPreview()
    await tick()
    expect(target.querySelector<HTMLVideoElement>('video')?.getAttribute('src'))
      .toBe('/files/sources/primary.mp4')
    expect(target.textContent).toContain('Original · fallback')
    expect(target.textContent).toContain('Proxy без звука')
    expect(target.querySelectorAll('option')).toHaveLength(1)

    await unmount(component)
  })
})

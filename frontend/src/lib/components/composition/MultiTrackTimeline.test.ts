import { setTextColorKey } from '$lib/composition/colorCommands.js'
import { pairImportedVideoAudio } from '$lib/composition/importPair.js'
import { importPreferences } from '$lib/state/importPreferences.svelte.js'
// @ts-expect-error Vitest's SSR condition exposes server-only mount; this test needs Svelte's client entry.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { COMPOSITION_TIME_BASE } from '$lib/composition/types.js'
import { LocalWaveformCache } from '$lib/audio/waveformCache.js'
import {
  updateCompositionCanvas,
  addCompositionTrack,
  copyCompositionSelection,
  addColorToComposition,
  addTextToComposition,
  commitDocument,
  selectCompositionClip,
  selectCompositionGap,
  selectCompositionKeyframe,
  setCompositionAutomationKeyframe,
  setCompositionClipLocked,
  addMediaInfoToComposition,
  compositionState,
  undoComposition,
  toggleCompositionTrackFlag,
  moveCompositionClip,
  resetCompositionForTests,
  setCompositionPlayhead,
  setCompositionTransition,
} from '$lib/state/composition.svelte.js'
import MultiTrackTimeline from './MultiTrackTimeline.svelte'

let target: HTMLDivElement

beforeEach(() => {
  // Exercise embedded-audio authoring independently of the default import policy.
  importPreferences.autoPairAudio = false
  vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new Error('decode unavailable in test')))
  resetCompositionForTests()
  addMediaInfoToComposition({
    id: 'timeline-video',
    url: '/files/sources/timeline.mp4',
    filename: 'timeline.mp4',
    mediaType: 'video',
    duration: 8,
    width: 1280,
    height: 720,
    acodec: 'aac',
  })
  target = document.createElement('div')
  document.body.append(target)
})

afterEach(() => {
  vi.unstubAllGlobals()
  target?.remove()
})

function button(text: string): HTMLButtonElement {
  const result = [...target.querySelectorAll('button')].find((candidate) => candidate.textContent?.includes(text))
  if (!result) throw new Error(`Missing button: ${text}`)
  return result
}

function change(control: HTMLInputElement, value: string): void {
  control.value = value
  control.dispatchEvent(new Event('input', { bubbles: true }))
  control.dispatchEvent(new Event('change', { bubbles: true }))
}

describe('MultiTrackTimeline', () => {
  it('scrolls a marquee horizontally at the edge and stops its frame loop on cancellation', async () => {
    let frame: FrameRequestCallback | null = null
    vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => { frame = callback; return 42 })
    const cancel = vi.fn()
    vi.stubGlobal('cancelAnimationFrame', cancel)
    const component = mount(MultiTrackTimeline, { target })
    try {
      await tick()
      const scroll = target.querySelector<HTMLElement>('.composition-timeline-scroll')!
      scroll.getBoundingClientRect = () => new DOMRect(0, 0, 800, 200)
      scroll.setPointerCapture = () => {}
      scroll.hasPointerCapture = () => false
      scroll.scrollTop = 11
      const lane = target.querySelector<HTMLElement>('.composition-track-lane')!
      lane.getBoundingClientRect = () => new DOMRect(-scroll.scrollLeft, 0, 1200, 64)
      const pointer = (name: string, x: number, y: number) => lane.dispatchEvent(new MouseEvent(name, { bubbles: true, button: 0, clientX: x, clientY: y }))
      pointer('pointerdown', 700, 60)
      pointer('pointermove', 790, 10)
      expect(frame).not.toBeNull()
      ;(frame as unknown as FrameRequestCallback)(0)
      expect(scroll.scrollLeft).toBe(14)
      expect(scroll.scrollTop).toBe(11)
      pointer('pointercancel', 790, 10)
      expect(cancel).toHaveBeenCalledWith(42)
      ;(frame as unknown as FrameRequestCallback)(16)
      expect(scroll.scrollLeft).toBe(14)
      await tick()
      expect(target.querySelector('.composition-marquee')).toBeNull()
    } finally { await unmount(component) }
  })
  it.each(['shrink', 'escape', 'cancel'] as const)('selects a live box across lanes and preserves history: %s', async (mode) => {
    const videoId = compositionState.document.tracks[0]!.clips[0]!.id
    compositionState.document = { ...pairImportedVideoAudio(compositionState.document, videoId, { audioTrackId: 'paired-audio', audioClipId: 'paired-sound', linkId: 'pair' }), links: [] }
    selectCompositionClip(compositionState.document.tracks[0]!.id, videoId, { single: true })
    setCompositionAutomationKeyframe({ kind: 'visual', clipId: videoId }, 'opacity', 1, 0)
    selectCompositionKeyframe({ kind: 'visual', clipId: videoId }, 'opacity', 0)
    const previousKeys = JSON.stringify(compositionState.ui.selectedKeyframes)
    const original = JSON.stringify(compositionState.document)
    const depth = compositionState.history.past.length
    const component = mount(MultiTrackTimeline, { target })
    await tick()
    const scroll = target.querySelector<HTMLElement>('.composition-timeline-scroll')!
    scroll.getBoundingClientRect = () => new DOMRect(0, 0, 800, 200)
    scroll.setPointerCapture = () => {}
    scroll.hasPointerCapture = () => false
    const lanes = [...target.querySelectorAll<HTMLElement>('.composition-track-lane')]
    lanes.forEach((lane, index) => { lane.getBoundingClientRect = () => new DOMRect(0, index * 64, 800, 64) })
    const pointer = (name: string, x: number, y: number) => lanes[1]!.dispatchEvent(new MouseEvent(name, { bubbles: true, button: 0, clientX: x, clientY: y }))
    pointer('pointerdown', 550, 120)
    pointer('pointermove', 100, 10)
    await tick()
    expect(compositionState.ui.selectedClipIds).toEqual([videoId, 'paired-sound'])
    expect(compositionState.ui.selectedKeyframes).toEqual([])
    expect(target.querySelector('.composition-marquee')).not.toBeNull()
    if (mode === 'shrink') {
      pointer('pointermove', 100, 80)
      expect(compositionState.ui.selectedClipIds).toEqual(['paired-sound'])
      pointer('pointerup', 100, 80)
      lanes[1]!.dispatchEvent(new MouseEvent('click', { bubbles: true, clientX: 100, clientY: 80 }))
      expect(compositionState.ui.selectedClipIds).toEqual(['paired-sound'])
    } else if (mode === 'escape') window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    else pointer('pointercancel', 100, 10)
    await tick()
    if (mode !== 'shrink') expect(compositionState.ui.selectedClipIds).toEqual([videoId])
    expect(JSON.stringify(compositionState.ui.selectedKeyframes)).toBe(mode === 'shrink' ? '[]' : previousKeys)
    expect(target.querySelector('.composition-marquee')).toBeNull()
    expect(compositionState.transport.playheadTicks).toBe(0)
    expect(JSON.stringify(compositionState.document)).toBe(original)
    expect(compositionState.history.past).toHaveLength(depth)
    await unmount(component)
  })
  it('clears clip selection on a click below the lanes without seeking or changing history', async () => {
    const track = compositionState.document.tracks[0]!
    selectCompositionClip(track.id, track.clips[0]!.id)
    const depth = compositionState.history.past.length
    const component = mount(MultiTrackTimeline, { target })
    await tick()
    target.querySelector<HTMLElement>('.composition-timeline-scroll')!.dispatchEvent(new MouseEvent('pointerup', { bubbles: true, button: 0 }))
    expect(compositionState.ui.selectedClipIds).toEqual([])
    expect(compositionState.ui.selectedClipId).toBeNull()
    expect(compositionState.transport.playheadTicks).toBe(0)
    expect(compositionState.history.past).toHaveLength(depth)
    await unmount(component)
  })

  it('does not arm a gap on a locked lane and drops its stale highlight when the lane locks', () => {
    const track = compositionState.document.tracks[0]!
    compositionState.document = { ...compositionState.document, tracks: compositionState.document.tracks.map((lane) => lane.id === track.id ? { ...lane, clips: lane.clips.map((clip) => ({ ...clip, timelineStartTicks: 2_000_000 })) } : lane) } as typeof compositionState.document
    selectCompositionGap(track.id, 1_000_000)
    expect(compositionState.ui.selectedGap).toMatchObject({ start: 0, end: 2_000_000 })
    toggleCompositionTrackFlag(track.id, 'locked')
    expect(compositionState.ui.selectedGap).toBeNull()
    selectCompositionGap(track.id, 1_000_000)
    expect(compositionState.ui.selectedGap).toBeNull()
  })

  it.each(['move', 'raise', 'cancel'] as const)('projects transition chrome with its complete moving pair: %s', async (mode) => {
    const source = compositionState.document.tracks[0]!
    if (source.kind !== 'video') throw new Error('Expected video fixture')
    const first = source.clips[0]!
    const transition = { id: 'pair-transition', fromClipId: first.id, toClipId: 'second', durationTicks: 1_000_000, kind: 'dissolve' as const }
    compositionState.document = { ...compositionState.document, tracks: [
      { ...source, id: 'pair-lane', clips: [{ ...first, sourceOutTicks: 4_000_000 }, { ...first, id: 'second', timelineStartTicks: 4_000_000, sourceInTicks: 4_000_000 }], transitions: [transition] },
      { ...source, id: 'destination', clips: [] },
    ] }
    selectCompositionClip('pair-lane', first.id, { single: true })
    selectCompositionClip('pair-lane', 'second', { toggle: true })
    compositionState.ui.snapEnabled = false
    const original = JSON.stringify(compositionState.document)
    const depth = compositionState.history.past.length
    const component = mount(MultiTrackTimeline, { target })
    await tick()
    const clip = target.querySelector<HTMLElement>('[data-track-id="pair-lane"] .composition-clip')!
    const destination = target.querySelector<HTMLElement>(mode === 'raise' ? '[data-new-track-drop] .composition-new-track-lane' : '[data-track-id="destination"] .composition-track-lane')!
    clip.setPointerCapture = () => {}
    clip.hasPointerCapture = () => false
    const previous = Object.getOwnPropertyDescriptor(document, 'elementFromPoint')
    Object.defineProperty(document, 'elementFromPoint', { configurable: true, value: () => destination })
    try {
      clip.dispatchEvent(new MouseEvent('pointerdown', { bubbles: true, button: 0, clientX: 0 }))
      clip.dispatchEvent(new MouseEvent('pointermove', { bubbles: true, clientX: compositionState.ui.zoomPxPerSecond }))
      await tick()
      const ghost = destination.querySelector<HTMLElement>('.composition-transition-ghost')!
      expect(ghost).not.toBeNull()
      expect(Number.parseFloat(ghost.style.left)).toBeCloseTo(4.5 * compositionState.ui.zoomPxPerSecond)
      expect(target.querySelector('[data-track-id="pair-lane"] button.composition-transition-handle')).toBeNull()
      expect(JSON.stringify(compositionState.document)).toBe(original)
      clip.dispatchEvent(new MouseEvent(mode === 'cancel' ? 'pointercancel' : 'pointerup', { bubbles: true, clientX: compositionState.ui.zoomPxPerSecond }))
      await tick()
      expect(target.querySelector('.composition-transition-ghost')).toBeNull()
      if (mode !== 'cancel') {
        expect(compositionState.history.past).toHaveLength(depth + 1)
        const landed = compositionState.document.tracks.find((track) => track.clips.some((clip) => clip.id === 'second'))!
        expect(landed).toMatchObject({ transitions: [transition] })
        undoComposition()
      }
      expect(JSON.stringify(compositionState.document)).toBe(original)
    } finally {
      if (previous) Object.defineProperty(document, 'elementFromPoint', previous)
      else Reflect.deleteProperty(document, 'elementFromPoint')
      await unmount(component)
    }
  })

  it('previews and commits a displayed lane offset for the whole selected block as one Undo', async () => {
    const source = compositionState.document.tracks[0]!
    if (source.kind !== 'video') throw new Error('Expected video fixture')
    const first = source.clips[0]!
    compositionState.document = { ...compositionState.document, links: [], tracks: [
      { ...source, id: 'lane-a' },
      { ...source, id: 'lane-b', clips: [{ ...first, id: 'second', timelineStartTicks: 10_000_000 }] },
      { ...source, id: 'lane-c', clips: [] },
    ] }
    selectCompositionClip('lane-a', first.id, { single: true })
    selectCompositionClip('lane-b', 'second', { toggle: true })
    compositionState.ui.snapEnabled = false
    const original = JSON.stringify(compositionState.document)
    const depth = compositionState.history.past.length
    const component = mount(MultiTrackTimeline, { target })
    await tick()
    const clip = target.querySelector<HTMLElement>('[data-track-id="lane-a"] .composition-clip')!
    const destination = target.querySelector<HTMLElement>('[data-track-id="lane-b"] .composition-track-lane')!
    clip.setPointerCapture = () => {}
    clip.hasPointerCapture = () => false
    const previous = Object.getOwnPropertyDescriptor(document, 'elementFromPoint')
    Object.defineProperty(document, 'elementFromPoint', { configurable: true, value: () => destination })
    try {
      clip.dispatchEvent(new MouseEvent('pointerdown', { bubbles: true, button: 0, clientX: 0 }))
      clip.dispatchEvent(new MouseEvent('pointermove', { bubbles: true, clientX: compositionState.ui.zoomPxPerSecond }))
      await tick()
      expect(target.querySelector('[data-track-id="lane-b"] .composition-move-ghost')).not.toBeNull()
      expect(target.querySelector('[data-track-id="lane-c"] .composition-move-ghost')).not.toBeNull()
      expect(target.querySelector('.composition-move-ghost.invalid')).toBeNull()
      expect(JSON.stringify(compositionState.document)).toBe(original)
      clip.dispatchEvent(new MouseEvent('pointerup', { bubbles: true, clientX: compositionState.ui.zoomPxPerSecond }))
      await tick()
      expect(compositionState.document.tracks.map((track) => track.clips.map((clip) => [clip.id, clip.timelineStartTicks]))).toEqual([[], [[first.id, 1_000_000]], [['second', 11_000_000]]])
      expect(compositionState.history.past).toHaveLength(depth + 1)
      undoComposition()
      expect(JSON.stringify(compositionState.document)).toBe(original)
    } finally {
      if (previous) Object.defineProperty(document, 'elementFromPoint', previous)
      else Reflect.deleteProperty(document, 'elementFromPoint')
      await unmount(component)
    }
  })
  it.each(['move', 'unlinked', 'cancel', 'locked-sibling'] as const)('previews every ordinary drag subject on its own grid: %s', async (mode) => {
    const videoId = compositionState.document.tracks[0]!.clips[0]!.id
    compositionState.document = pairImportedVideoAudio(compositionState.document, videoId, { audioTrackId: 'paired-audio', audioClipId: 'paired-sound', linkId: 'pair' })
    if (mode === 'unlinked') compositionState.document = { ...compositionState.document, links: [] }
    selectCompositionClip(compositionState.document.tracks[0]!.id, videoId, { single: mode === 'unlinked' })
    if (mode === 'unlinked') selectCompositionClip('paired-audio', 'paired-sound', { toggle: true })
    updateCompositionCanvas({ fps: 30000 / 1001, fpsRate: { numerator: 30000, denominator: 1001 } })
    compositionState.ui.snapEnabled = false
    if (mode === 'locked-sibling') setCompositionClipLocked('paired-sound', true)
    const component = mount(MultiTrackTimeline, { target })
    await tick()
    const clip = target.querySelector<HTMLElement>('.composition-clip.video')!
    clip.setPointerCapture = () => {}
    clip.hasPointerCapture = () => false
    const original = JSON.stringify(compositionState.document)
    const depth = compositionState.history.past.length
    const pointer = (name: string, x: number) => clip.dispatchEvent(new MouseEvent(name, { bubbles: true, button: 0, clientX: x }))
    const previous = Object.getOwnPropertyDescriptor(document, 'elementFromPoint')
    Object.defineProperty(document, 'elementFromPoint', { configurable: true, value: () => clip })
    try {
      pointer('pointerdown', 0)
      const x = compositionState.ui.zoomPxPerSecond * 33_367 / 1_000_000
      pointer('pointermove', x)
      await tick()
      const ghosts = [...target.querySelectorAll<HTMLElement>('.composition-move-ghost')]
      expect(ghosts).toHaveLength(2)
      expect(ghosts.every((ghost) => ghost.classList.contains('invalid'))).toBe(mode === 'locked-sibling')
      expect(clip.style.left).toBe('0px')
      expect(JSON.stringify(compositionState.document)).toBe(original)
      expect(compositionState.history.past).toHaveLength(depth)
      const positions = ghosts.map((ghost) => Number.parseFloat(ghost.style.left))
      pointer(mode === 'cancel' ? 'pointercancel' : 'pointerup', x)
      await tick()
      expect(target.querySelectorAll('.composition-move-ghost')).toHaveLength(0)
      if (mode === 'move' || mode === 'unlinked') {
        const starts = compositionState.document.tracks.flatMap((track) => track.clips.map((clip) => clip.timelineStartTicks))
        expect(starts).toEqual([33_367, 33_375])
        starts.forEach((start, index) => expect(positions[index]).toBeCloseTo(start / 1_000_000 * compositionState.ui.zoomPxPerSecond, 6))
        expect(compositionState.history.past).toHaveLength(depth + 1)
        undoComposition()
      }
      expect(JSON.stringify(compositionState.document)).toBe(original)
      expect(compositionState.history.past).toHaveLength(depth)
    } finally {
      if (previous) Object.defineProperty(document, 'elementFromPoint', previous)
      else Reflect.deleteProperty(document, 'elementFromPoint')
      await unmount(component)
    }
  })
  it('drops onto one fresh lane with a matching preview and restores the source lane in one Undo', async () => {
    const component = mount(MultiTrackTimeline, { target })
    await tick()
    const clip = target.querySelector<HTMLElement>('.composition-clip')!
    clip.setPointerCapture = () => {}
    clip.hasPointerCapture = () => false
    const previous = Object.getOwnPropertyDescriptor(document, 'elementFromPoint')
    const before = JSON.stringify(compositionState.document)
    const depth = compositionState.history.past.length
    const pointer = (name: string, x: number) => clip.dispatchEvent(new MouseEvent(name, { bubbles: true, button: 0, clientX: x }))
    try {
      pointer('pointerdown', 0)
      await tick()
      const strip = target.querySelector<HTMLElement>('[data-new-track-drop]')!
      expect(strip).not.toBeNull()
      Object.defineProperty(document, 'elementFromPoint', { configurable: true, value: () => strip })
      pointer('pointermove', compositionState.ui.zoomPxPerSecond * 2)
      await tick()
      expect(strip.querySelector('.composition-copy-ghost')).not.toBeNull()
      expect(strip.classList.contains('invalid')).toBe(false)
      expect(compositionState.history.past.length).toBe(depth)
      pointer('pointerup', compositionState.ui.zoomPxPerSecond * 2)
      await tick()
      expect(target.querySelector('[data-new-track-drop] .composition-copy-ghost')).toBeNull()
      expect(compositionState.document.tracks[0]!.clips[0]!.timelineStartTicks).toBe(2_000_000)
      expect(compositionState.document.tracks[0]!.id).not.toBe(JSON.parse(before).tracks[0].id)
      expect(compositionState.history.past.length).toBe(depth + 1)
      undoComposition()
      expect(JSON.stringify(compositionState.document)).toBe(before)
    } finally {
      if (previous) Object.defineProperty(document, 'elementFromPoint', previous)
      else Reflect.deleteProperty(document, 'elementFromPoint')
      await unmount(component)
    }
  })
  it('raises the menu subject in one undo and restores the removed source lane', async () => {
    const before = JSON.stringify(compositionState.document)
    const history = compositionState.history.past.length
    const component = mount(MultiTrackTimeline, { target })
    await tick()
    const clip = target.querySelector<HTMLElement>('.composition-clip')!
    clip.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true }))
    await tick()
    button('На новую дорожку').click()
    await tick()
    expect(compositionState.history.past.length).toBe(history + 1)
    expect(compositionState.document.tracks[0]!.id).not.toBe(JSON.parse(before).tracks[0].id)
    undoComposition()
    expect(JSON.stringify(compositionState.document)).toBe(before)
    await unmount(component)
  })
  it('sets and clears exact export In/Out points with buttons and CapCut keyboard shortcuts', async () => {
    const component = mount(MultiTrackTimeline, { target })
    await tick()

    setCompositionPlayhead(1_250_000)
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'i', bubbles: true }))
    setCompositionPlayhead(4_500_000)
    button('Out · O').click()
    await tick()

    expect(compositionState.export.rangeInTicks).toBe(1_250_000)
    expect(compositionState.export.rangeOutTicks).toBe(4_500_000)
    expect(target.querySelector<HTMLElement>('.composition-export-range')?.style.width).toBe('273px')

    button('Очистить In/Out').click()
    expect(compositionState.export.rangeInTicks).toBeNull()
    expect(compositionState.export.rangeOutTicks).toBeNull()
    await unmount(component)
  })

  it('renders review markers on the ruler and seeks their exact timeline tick', async () => {
    const component = mount(MultiTrackTimeline, {
      target,
      props: {
        reviewThreads: [{
          id: 'review-1',
          projectId: 'project-1',
          comments: [{ id: 'comment-1', author: 'alice', body: 'Check cut', timelineTick: 2_500_000, createdAt: 1 }],
        }],
      },
    })
    await tick()

    const marker = target.querySelector<HTMLButtonElement>('.composition-review-handle')!
    expect(marker.getAttribute('aria-label')).toContain('2.5s')
    marker.click()
    expect(compositionState.transport.playheadTicks).toBe(2_500_000)
    await unmount(component)
  })

  it('Alt-drags a copy and leaves the original span unchanged with one Undo', async () => {
    const component = mount(MultiTrackTimeline, { target })
    await tick()
    const clip = target.querySelector<HTMLElement>('.composition-clip')!
    clip.setPointerCapture = () => {}
    clip.hasPointerCapture = () => false
    const original = JSON.stringify(compositionState.document)
    const depth = compositionState.history.past.length
    const pointer = (name: string, x: number) => clip.dispatchEvent(new MouseEvent(name, { bubbles: true, button: 0, clientX: x, altKey: true }))
    Object.defineProperty(document, 'elementFromPoint', { configurable: true, value: () => clip })
    pointer('pointerdown', 0)
    await tick()
    expect(target.querySelector('.composition-copy-ghost.invalid')).not.toBeNull()
    pointer('pointermove', compositionState.ui.zoomPxPerSecond * 11)
    await tick()
    expect(target.querySelectorAll('.composition-copy-ghost')).toHaveLength(1)
    expect(target.querySelector('.composition-copy-ghost.invalid')).toBeNull()
    expect(compositionState.history.past).toHaveLength(depth)
    pointer('pointerup', compositionState.ui.zoomPxPerSecond * 11)
    await tick()
    expect(compositionState.document.tracks[0]!.clips).toHaveLength(2)
    expect(compositionState.document.tracks[0]!.clips[0]!.timelineStartTicks).toBe(0)
    expect(compositionState.document.tracks[0]!.clips[1]!.timelineStartTicks).toBe(11_000_000)
    expect(compositionState.history.past).toHaveLength(depth + 1)
    undoComposition()
    expect(JSON.stringify(compositionState.document)).toBe(original)
    Reflect.deleteProperty(document, 'elementFromPoint')
    await unmount(component)
  })

  it.each(['linked', 'single', 'cancel', 'collision'] as const)('qualifies Alt-drag %s selection and atomic history', async (mode) => {
    const videoId = compositionState.document.tracks[0]!.clips[0]!.id
    compositionState.document = pairImportedVideoAudio(compositionState.document, videoId, { audioTrackId: 'paired-audio', audioClipId: 'paired-sound', linkId: 'original-pair' })
    // Fresh selection on a member must expand to the link when Alt means drag-copy.
    compositionState.ui.selectedClipIds = []
    compositionState.ui.selectedClipId = null
    const component = mount(MultiTrackTimeline, { target })
    await tick()
    const clip = target.querySelector<HTMLElement>('.composition-clip.video')!
    clip.setPointerCapture = () => {}
    clip.hasPointerCapture = () => false
    const pointer = (name: string, x: number) => clip.dispatchEvent(new MouseEvent(name, { bubbles: true, button: 0, clientX: x, altKey: true }))
    const previousHitTest = Object.getOwnPropertyDescriptor(document, 'elementFromPoint')
    Object.defineProperty(document, 'elementFromPoint', { configurable: true, value: () => clip })
    try {
      if (mode === 'single') {
        const depth = compositionState.history.past.length
        pointer('pointerdown', 0)
        pointer('pointerup', 0)
        await tick()
        expect(compositionState.ui.selectedClipIds).toEqual([videoId])
        expect(compositionState.history.past).toHaveLength(depth)
      }
      const original = JSON.stringify(compositionState.document)
      const depth = compositionState.history.past.length
      pointer('pointerdown', 0)
      const x = compositionState.ui.zoomPxPerSecond * (mode === 'collision' ? 1 : 11)
      pointer('pointermove', x)
      await tick()
      expect(target.querySelectorAll('.composition-copy-ghost')).toHaveLength(mode === 'single' ? 1 : 2)
      expect(!!target.querySelector('.composition-copy-ghost.invalid')).toBe(mode === 'collision')
      expect(compositionState.history.past).toHaveLength(depth)
      pointer(mode === 'cancel' ? 'pointercancel' : 'pointerup', x)
      await tick()
      expect(target.querySelectorAll('.composition-copy-ghost')).toHaveLength(0)
      if (mode === 'cancel' || mode === 'collision') {
        expect(JSON.stringify(compositionState.document)).toBe(original)
        expect(compositionState.history.past).toHaveLength(depth)
      } else {
        expect(compositionState.history.past).toHaveLength(depth + 1)
        expect(compositionState.document.links).toHaveLength(mode === 'single' ? 1 : 2)
        expect(compositionState.ui.selectedClipIds).toHaveLength(mode === 'single' ? 1 : 2)
        if (mode === 'linked') expect(compositionState.document.links![1]!.memberIds.every((id) => !compositionState.document.links![0]!.memberIds.includes(id))).toBe(true)
        undoComposition()
        expect(JSON.stringify(compositionState.document)).toBe(original)
      }
    } finally {
      if (previousHitTest) Object.defineProperty(document, 'elementFromPoint', previousHitTest)
      else Reflect.deleteProperty(document, 'elementFromPoint')
      await unmount(component)
    }
  })

  it.each(['vertical-single', 'cross-track', 'locked-sibling'] as const)('qualifies %s duplicate drops', async (mode) => {
    const sourceTrack = compositionState.document.tracks[0]!
    const videoId = sourceTrack.clips[0]!.id
    compositionState.document = pairImportedVideoAudio(compositionState.document, videoId, { audioTrackId: 'paired-audio', audioClipId: 'paired-sound', linkId: 'original-pair' })
    const destinationId = addCompositionTrack('video')
    if (mode === 'locked-sibling') setCompositionClipLocked('paired-sound', true)
    selectCompositionClip(sourceTrack.id, videoId, { single: mode === 'vertical-single' })
    const before = JSON.stringify(compositionState.document)
    const depth = compositionState.history.past.length
    const component = mount(MultiTrackTimeline, { target })
    await tick()
    const clip = target.querySelector<HTMLElement>('.composition-clip.video')!
    const destination = target.querySelector<HTMLElement>(`[data-track-id="${destinationId}"]`)!
    clip.setPointerCapture = () => {}
    clip.hasPointerCapture = () => false
    const previous = Object.getOwnPropertyDescriptor(document, 'elementFromPoint')
    Object.defineProperty(document, 'elementFromPoint', { configurable: true, value: () => destination })
    const pointer = (name: string, x: number, y: number) => clip.dispatchEvent(new MouseEvent(name, { bubbles: true, button: 0, clientX: x, clientY: y, altKey: true }))
    try {
      pointer('pointerdown', 0, 0)
      const x = mode === 'vertical-single' ? 0 : compositionState.ui.zoomPxPerSecond * 11
      pointer('pointermove', x, 100)
      await tick()
      expect(destination.querySelector('.composition-copy-ghost')).not.toBeNull()
      expect(!!target.querySelector('.composition-copy-ghost.invalid')).toBe(mode === 'locked-sibling')
      pointer('pointerup', x, 100)
      await tick()
      if (mode === 'locked-sibling') {
        expect(JSON.stringify(compositionState.document)).toBe(before)
        expect(compositionState.history.past).toHaveLength(depth)
      } else {
        const copied = compositionState.document.tracks.find((track) => track.id === destinationId)!.clips[0]!
        expect(copied.timelineStartTicks).toBe(mode === 'vertical-single' ? 0 : 11_000_000)
        expect(compositionState.document.tracks.find((track) => track.id === sourceTrack.id)!.clips).toHaveLength(1)
        expect(compositionState.history.past).toHaveLength(depth + 1)
        undoComposition()
        expect(JSON.stringify(compositionState.document)).toBe(before)
      }
    } finally {
      if (previous) Object.defineProperty(document, 'elementFromPoint', previous)
      else Reflect.deleteProperty(document, 'elementFromPoint')
      await unmount(component)
    }
  })

  it('steps keyboard trim boundaries on the exact fractional grid and restores them with Undo', async () => {
    updateCompositionCanvas({ fps: 30000 / 1001, fpsRate: { numerator: 30000, denominator: 1001 } })
    const component = mount(MultiTrackTimeline, { target })
    await tick()
    const handle = target.querySelector<HTMLButtonElement>('.composition-trim-handle.left')!
    const depth = compositionState.history.past.length
    handle.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true }))
    await tick()
    expect(compositionState.document.tracks[0]!.clips[0]!.timelineStartTicks).toBe(33_367)
    handle.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true }))
    await tick()
    expect(compositionState.document.tracks[0]!.clips[0]!.timelineStartTicks).toBe(66_733)
    expect(compositionState.history.past).toHaveLength(depth + 2)
    undoComposition()
    expect(compositionState.document.tracks[0]!.clips[0]!.timelineStartTicks).toBe(33_367)
    undoComposition()
    expect(compositionState.document.tracks[0]!.clips[0]!.timelineStartTicks).toBe(0)
    await unmount(component)
  })

  it('renders authored tracks and invokes split plus track controls', async () => {
    const component = mount(MultiTrackTimeline, { target })
    await tick()

    expect(target.textContent).toContain('timeline.mp4')
    expect(target.querySelectorAll('.composition-clip')).toHaveLength(1)

    setCompositionPlayhead(3 * COMPOSITION_TIME_BASE)
    button('Разрезать').click()
    await tick()
    expect(compositionState.document.tracks[0]!.clips).toHaveLength(2)
    expect(target.querySelectorAll('.composition-clip')).toHaveLength(2)

    const videoTrack = compositionState.document.tracks[0]!
    const [from, to] = videoTrack.clips
    setCompositionTransition(videoTrack.id, from!.id, to!.id, 'dissolve', 500_000)
    await tick()
    expect(target.querySelector<HTMLButtonElement>('.composition-transition-handle')?.getAttribute('aria-label')).toContain('dissolve')

    const sound = target.querySelector<HTMLButtonElement>('button[title="Звук"]')!
    sound.click()
    await tick()
    expect(compositionState.document.tracks[0]).toMatchObject({ kind: 'video', muted: true })

    const firstClip = compositionState.document.tracks[0]!.clips[0]!
    const startBefore = firstClip.timelineStartTicks
    const trimStart = target.querySelector<HTMLButtonElement>('.composition-trim-handle.left')!
    trimStart.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true, cancelable: true }))
    await tick()
    expect(compositionState.document.tracks[0]!.clips[0]!.timelineStartTicks).toBe(
      startBefore + Math.round(COMPOSITION_TIME_BASE / compositionState.document.canvas.fps),
    )

    await unmount(component)
  })

  it('renders an audio waveform tile and degrades gracefully when local decoding fails', async () => {
    addMediaInfoToComposition({
      id: 'timeline-audio',
      url: '/files/sources/timeline.wav',
      filename: 'timeline.wav',
      mediaType: 'audio',
      duration: 6,
      width: 0,
      height: 0,
      acodec: 'pcm_s16le',
    })
    const waveformCache = new LocalWaveformCache(vi.fn().mockRejectedValue(new Error('decode unavailable in test')))
    const component = mount(MultiTrackTimeline, { target, props: { waveformCache } })
    await tick()
    const waveform = target.querySelector<SVGElement>('svg.audio-waveform')
    expect(waveform).not.toBeNull()
    expect(waveform?.getAttribute('aria-label')).toContain('timeline.wav')
    await vi.waitFor(() => {
      expect(waveform?.getAttribute('aria-label')).toContain('недоступна')
    })

    const solo = target.querySelector<HTMLButtonElement>('button[title="Solo"]')!
    solo.click()
    await tick()
    expect(compositionState.document.tracks.find((track) => track.kind === 'audio')).toMatchObject({ solo: true })

    await unmount(component)
  })

  it('renders editable ruler markers and invokes one-shot magnet plus ripple delete', async () => {
    const component = mount(MultiTrackTimeline, { target })
    setCompositionPlayhead(2 * COMPOSITION_TIME_BASE)
    await tick()
    button('+ Marker').click()
    await tick()

    expect(target.querySelectorAll('.composition-marker-handle')).toHaveLength(1)
    const label = target.querySelector<HTMLInputElement>('input[aria-label^="Метка marker"]')!
    change(label, 'Drop')
    const markerTime = target.querySelector<HTMLInputElement>('input[aria-label^="Время marker"]')!
    change(markerTime, '1.5')
    change(target.querySelector<HTMLInputElement>('input[type="color"]')!, '#22c55e')
    await tick()
    expect(target.querySelector('.composition-marker-handle')?.getAttribute('aria-label')).toContain('Drop, 1.5s')
    expect(target.querySelector<HTMLElement>('.composition-marker-handle')?.style.getPropertyValue('--marker-color')).toBe('#22c55e')
    setCompositionPlayhead(0)
    target.querySelector<HTMLButtonElement>('.composition-marker-handle')!.click()
    expect(compositionState.transport.playheadTicks).toBe(1.5 * COMPOSITION_TIME_BASE)

    setCompositionPlayhead(3 * COMPOSITION_TIME_BASE)
    button('Разрезать').click()
    await tick()
    const track = compositionState.document.tracks[0]!
    const right = track.clips[1]!
    moveCompositionClip(right.id, track.id, 5 * COMPOSITION_TIME_BASE, false)
    await tick()
    target.querySelector<HTMLButtonElement>('button[aria-label^="Собрать gaps"]')!.click()
    await tick()
    expect(compositionState.document.tracks[0]!.clips[1]!.timelineStartTicks).toBe(3 * COMPOSITION_TIME_BASE)
    button('Ripple delete').click()
    await tick()
    expect(compositionState.document.tracks[0]!.clips).toHaveLength(1)
    target.querySelector<HTMLButtonElement>('button[aria-label="Удалить marker Drop"]')!.click()
    await tick()
    expect(target.querySelectorAll('.composition-marker-handle')).toHaveLength(0)

    await unmount(component)
  })

  it('closes the playhead gap in one undo step and leaves the playhead absolute', async () => {
    const component = mount(MultiTrackTimeline, { target })
    setCompositionPlayhead(3 * COMPOSITION_TIME_BASE)
    button('Разрезать').click()
    await tick()
    const track = compositionState.document.tracks[0]!
    moveCompositionClip(track.clips[1]!.id, track.id, 5 * COMPOSITION_TIME_BASE, false)
    setCompositionPlayhead(4 * COMPOSITION_TIME_BASE)
    await tick()
    const before = JSON.stringify(compositionState.document)
    const depth = compositionState.history.past.length
    target.querySelector<HTMLButtonElement>('button[aria-label^="Закрыть пробел у плейхеда"]')!.click()
    await tick()
    expect(compositionState.document.tracks[0]!.clips[1]!.timelineStartTicks).toBe(3 * COMPOSITION_TIME_BASE)
    expect(compositionState.document.tracks[0]!.clips).toHaveLength(2)
    expect(compositionState.transport.playheadTicks).toBe(4 * COMPOSITION_TIME_BASE)
    expect(compositionState.history.past).toHaveLength(depth + 1)
    undoComposition()
    expect(JSON.stringify(compositionState.document)).toBe(before)
    await unmount(component)
  })

  it.each([false, true])('opens a gap menu and respects downstream locked=%s', async (locked) => {
    const component = mount(MultiTrackTimeline, { target })
    setCompositionPlayhead(3_000_000)
    button('Разрезать').click()
    await tick()
    const track = compositionState.document.tracks[0]!
    const rightId = track.clips[1]!.id
    moveCompositionClip(rightId, track.id, 5_000_000, false)
    if (locked) setCompositionClipLocked(rightId, true)
    await tick()
    const lane = target.querySelector<HTMLElement>('.composition-track-lane')!
    lane.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, clientX: 4 * compositionState.ui.zoomPxPerSecond }))
    await tick()
    const menu = target.querySelector<HTMLElement>('[role="menu"][aria-label="Действия с выбранным пробелом"]')!
    expect(menu).not.toBeNull()
    const action = menu.querySelector<HTMLButtonElement>('button')!
    expect(action.getAttribute('aria-disabled')).toBe(String(locked))
    const before = JSON.stringify(compositionState.document)
    const depth = compositionState.history.past.length
    action.click()
    await tick()
    if (locked) {
      expect(JSON.stringify(compositionState.document)).toBe(before)
      expect(compositionState.history.past).toHaveLength(depth)
      menu.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
      await tick()
      expect(document.activeElement).toBe(lane)
    } else {
      expect(compositionState.document.tracks[0]!.clips[1]!.timelineStartTicks).toBe(3_000_000)
      expect(compositionState.history.past).toHaveLength(depth + 1)
      undoComposition()
      expect(JSON.stringify(compositionState.document)).toBe(before)
    }
    expect(target.querySelector('.composition-clip-menu')).toBeNull()
    await unmount(component)
  })

  it('selects exact blank space by clicking and deletes it with one undo', async () => {
    const component = mount(MultiTrackTimeline, { target })
    setCompositionPlayhead(3 * COMPOSITION_TIME_BASE)
    button('Разрезать').click()
    await tick()
    const track = compositionState.document.tracks[0]!
    const rightId = track.clips[1]!.id
    moveCompositionClip(rightId, track.id, 5 * COMPOSITION_TIME_BASE, false)
    await tick()
    const lane = target.querySelector<HTMLElement>('.composition-track-lane')!
    lane.dispatchEvent(new MouseEvent('click', { bubbles: true, clientX: 4 * compositionState.ui.zoomPxPerSecond }))
    await tick()
    expect(compositionState.ui.selectedGap).toEqual({ trackId: track.id, start: 3 * COMPOSITION_TIME_BASE, end: 5 * COMPOSITION_TIME_BASE })
    expect(compositionState.ui.selectedClipIds).toEqual([])
    expect(target.querySelector('.composition-selected-gap')).not.toBeNull()
    const depth = compositionState.history.past.length
    lane.dispatchEvent(new KeyboardEvent('keydown', { key: 'Delete', bubbles: true }))
    await tick()
    expect(compositionState.document.tracks[0]!.clips[1]!.timelineStartTicks).toBe(3 * COMPOSITION_TIME_BASE)
    expect(compositionState.history.past).toHaveLength(depth + 1)
    expect(compositionState.ui.selectedGap).toBeNull()
    undoComposition()
    lane.dispatchEvent(new MouseEvent('click', { bubbles: true, clientX: 4 * compositionState.ui.zoomPxPerSecond }))
    moveCompositionClip(rightId, track.id, 6 * COMPOSITION_TIME_BASE, false)
    await tick()
    expect(compositionState.ui.selectedGap).toBeNull()
    expect(target.querySelector('.composition-selected-gap')).toBeNull()
    await unmount(component)
  })

  it('opens a gap menu by keyboard without seeking and closes it when its span becomes stale', async () => {
    const component = mount(MultiTrackTimeline, { target })
    const track = compositionState.document.tracks[0]!
    const clipId = track.clips[0]!.id
    moveCompositionClip(clipId, track.id, 2_000_000, false)
    setCompositionPlayhead(0)
    await tick()
    const lane = target.querySelector<HTMLElement>('.composition-track-lane')!
    lane.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', altKey: true, bubbles: true }))
    const depth = compositionState.history.past.length
    lane.dispatchEvent(new KeyboardEvent('keydown', { key: 'F10', shiftKey: true, bubbles: true }))
    await tick()
    const menu = target.querySelector<HTMLElement>('[role="menu"][aria-label="Действия с выбранным пробелом"]')!
    const action = menu.querySelector('button')!
    expect(document.activeElement).toBe(action)
    expect(compositionState.transport.playheadTicks).toBe(0)
    expect(compositionState.history.past).toHaveLength(depth)
    menu.dispatchEvent(new KeyboardEvent('keydown', { key: 'End', bubbles: true }))
    expect(document.activeElement).toBe(action)
    menu.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    await tick()
    expect(document.activeElement).toBe(lane)
    lane.dispatchEvent(new KeyboardEvent('keydown', { key: 'ContextMenu', bubbles: true }))
    await tick()
    expect(target.querySelector('.composition-clip-menu')).not.toBeNull()
    moveCompositionClip(clipId, track.id, 3_000_000, false)
    await tick()
    expect(target.querySelector('.composition-clip-menu')).toBeNull()
    expect(compositionState.ui.selectedGap).toBeNull()
    lane.dispatchEvent(new KeyboardEvent('keydown', { key: 'ContextMenu', bubbles: true }))
    await tick()
    expect(target.querySelector('.composition-clip-menu')).toBeNull()
    await unmount(component)
  })

  it('selects gaps by keyboard without seeking and clears them with Escape', async () => {
    const component = mount(MultiTrackTimeline, { target })
    const track = compositionState.document.tracks[0]!
    moveCompositionClip(track.clips[0]!.id, track.id, 2 * COMPOSITION_TIME_BASE, false)
    setCompositionPlayhead(0)
    await tick()
    const lane = target.querySelector<HTMLElement>('.composition-track-lane')!
    const revision = compositionState.history.past.length
    lane.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', altKey: true, bubbles: true }))
    await tick()
    expect(compositionState.ui.selectedGap).toEqual({ trackId: track.id, start: 0, end: 2 * COMPOSITION_TIME_BASE })
    expect(compositionState.transport.playheadTicks).toBe(0)
    expect(compositionState.history.past).toHaveLength(revision)
    const close = button('Закрыть пробел')
    expect(close.getAttribute('aria-disabled')).toBe('false')
    toggleCompositionTrackFlag(track.id, 'locked')
    await tick()
    expect(compositionState.ui.selectedGap).toBeNull()
    expect(target.querySelector('.composition-gap-highlight')).toBeNull()
    const blockedDepth = compositionState.history.past.length
    close.click()
    expect(compositionState.history.past).toHaveLength(blockedDepth)
    lane.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    await tick()
    expect(compositionState.ui.selectedGap).toBeNull()
    await unmount(component)
  })

  it('toggles exactly the selected clips and restores their state with Undo', async () => {
    const component = mount(MultiTrackTimeline, { target })
    await tick()
    const depth = compositionState.history.past.length
    button('Выключить клипы').click()
    await tick()
    expect(compositionState.document.tracks[0]!.clips[0]!.enabled).toBe(false)
    expect(target.querySelector('.composition-clip.disabled-clip')).not.toBeNull()
    expect(compositionState.history.past).toHaveLength(depth + 1)
    expect(button('Включить клипы')).not.toBeNull()
    undoComposition()
    await tick()
    expect(compositionState.document.tracks[0]!.clips[0]!.enabled).toBeUndefined()
    await unmount(component)
  })

  it('preserves exact multi-selection in the context menu and toggles it with one Undo', async () => {
    const component = mount(MultiTrackTimeline, { target })
    setCompositionPlayhead(3 * COMPOSITION_TIME_BASE)
    button('Разрезать').click()
    const clips = compositionState.document.tracks[0]!.clips
    compositionState.ui.selectedClipIds = clips.map((clip) => clip.id)
    await tick()
    const item = target.querySelector<HTMLElement>('.composition-clip')!
    item.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: 20, clientY: 20 }))
    await tick()
    const menu = target.querySelector<HTMLElement>('[role="menu"][aria-label="Действия с выделенными клипами"]')!
    expect(menu).not.toBeNull()
    expect(menu.textContent).toContain('Выключить клипы (2)')
    const depth = compositionState.history.past.length
    menu.querySelector<HTMLButtonElement>('button')!.click()
    await tick()
    expect(compositionState.document.tracks[0]!.clips.every((clip) => clip.enabled === false)).toBe(true)
    expect(compositionState.history.past).toHaveLength(depth + 1)
    expect(target.querySelector('.composition-clip-menu')).toBeNull()
    undoComposition()
    expect(compositionState.document.tracks[0]!.clips.every((clip) => clip.enabled === undefined)).toBe(true)
    await unmount(component)
  })

  it('opens a guarded clip menu by keyboard and leaves locked content unchanged', async () => {
    const component = mount(MultiTrackTimeline, { target })
    const track = compositionState.document.tracks[0]!
    toggleCompositionTrackFlag(track.id, 'locked')
    await tick()
    const item = target.querySelector<HTMLElement>('.composition-clip')!
    item.dispatchEvent(new KeyboardEvent('keydown', { key: 'F10', shiftKey: true, bubbles: true, cancelable: true }))
    await tick()
    const menu = target.querySelector<HTMLElement>('.composition-clip-menu')!
    const controls = [...menu.querySelectorAll<HTMLButtonElement>('button')]
    expect(controls).toHaveLength(4)
    expect(controls.every((button) => button.getAttribute('aria-disabled') === 'true')).toBe(true)
    const before = JSON.stringify(compositionState.document)
    controls.forEach((button) => button.click())
    expect(JSON.stringify(compositionState.document)).toBe(before)
    menu.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    await tick()
    expect(target.querySelector('.composition-clip-menu')).toBeNull()
    expect(document.activeElement).toBe(item)
    await unmount(component)
  })

  it('keeps disabled reasons focusable and roves through toolbar and command menu', async () => {
    const component = mount(MultiTrackTimeline, { target })
    await tick()

    const commands = [...target.querySelectorAll<HTMLButtonElement>('[data-timeline-command]')]
    const disabled = commands.find((command) => command.getAttribute('aria-disabled') === 'true')
    expect(disabled).toBeDefined()
    const reasonId = disabled!.getAttribute('aria-describedby')
    expect(reasonId).toBeTruthy()
    expect(target.querySelector(`#${reasonId}`)?.textContent?.trim()).not.toBe('')

    commands[0]!.focus()
    commands[0]!.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true, cancelable: true }))
    await tick()
    expect(document.activeElement).toBe(commands[1])
    expect(commands[1]!.tabIndex).toBe(0)

    button('Команды').click()
    await tick()
    const menuItems = [...target.querySelectorAll<HTMLButtonElement>('[role="menuitem"]')]
    expect(document.activeElement).toBe(menuItems[0])
    menuItems[0]!.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true, cancelable: true }))
    await tick()
    expect(document.activeElement).toBe(menuItems[1])
    expect(menuItems[1]!.tabIndex).toBe(0)

    await unmount(component)
  })
})

it('moves selected text and solid color keys together, retains selection and undoes atomically', async () => {
  setCompositionPlayhead(0)
  const textId = addTextToComposition('Animated text')
  const solidId = addColorToComposition()
  commitDocument(setTextColorKey(compositionState.document,textId,1000000,'#ff000080'))
  commitDocument(setTextColorKey(compositionState.document,solidId,1000000,'#0000ffff'))
  const component = mount(MultiTrackTimeline, { target })
  await tick()
  const key = (id: string,tickValue: number) => {
    const result = target.querySelector<HTMLButtonElement>(`[data-color-key-clip="${id}"] [data-color-key-tick="${tickValue}"]`)
    if (!result) throw new Error(`Missing color key ${id}:${tickValue}`)
    return result
  }
  key(textId,1000000).click()
  key(solidId,1000000).dispatchEvent(new MouseEvent('click',{ bubbles: true,shiftKey: true }))
  await tick()
  expect(compositionState.ui.selectedColorKeys).toHaveLength(2)
  const before = compositionState.document
  const frame = Math.round(COMPOSITION_TIME_BASE/compositionState.document.canvas.fps)
  key(solidId,1000000).dispatchEvent(new KeyboardEvent('keydown',{ key: 'ArrowRight',bubbles: true }))
  await tick()
  expect(key(textId,1000000+frame).getAttribute('aria-pressed')).toBe('true')
  expect(key(solidId,1000000+frame).getAttribute('aria-pressed')).toBe('true')
  expect(compositionState.ui.selectedColorKeys).toEqual([{ clipId: textId,tick: 1000000+frame },{ clipId: solidId,tick: 1000000+frame }])
  undoComposition()
  await tick()
  expect(compositionState.document).toEqual(before)
  expect(key(textId,1000000)).toBeTruthy()
  expect(key(solidId,1000000)).toBeTruthy()
  await unmount(component)
})


it('exposes sync-lock and Paste Insert through the timeline with one Undo',async()=>{
 const component=mount(MultiTrackTimeline,{target})
 try{
  await tick()
  const track=compositionState.document.tracks[0]!
  const sync=target.querySelector<HTMLButtonElement>('button[title="Sync-lock при вставке"]')!
  expect(sync.getAttribute('aria-pressed')).toBe('true')
  sync.click();await tick()
  expect(compositionState.document.tracks[0]!.syncLock).toBe(false)
  expect(sync.getAttribute('aria-pressed')).toBe('false')
  copyCompositionSelection()
  setCompositionPlayhead(2_000_000)
  await tick()
  const before=JSON.stringify(compositionState.document)
  const depth=compositionState.history.past.length
  button('Вставить со сдвигом').click();await tick()
  expect(compositionState.history.past.length).toBe(depth+1)
  expect(compositionState.document.tracks.find(candidate=>candidate.id===track.id)!.clips.map(clip=>clip.timelineStartTicks)).toEqual([0,2_000_000,10_000_000])
  undoComposition();await tick()
  expect(JSON.stringify(compositionState.document)).toBe(before)
 }finally{await unmount(component)}
})


it('moves keyboard focus to a selected clip when pointer defaults are suppressed',async()=>{
 const component=mount(MultiTrackTimeline,{target})
 try{
  await tick()
  const clip=target.querySelector<HTMLElement>('.composition-clip.video')!
  clip.setPointerCapture=()=>{}
  clip.hasPointerCapture=()=>false
  button('Вставить со сдвигом').focus()
  expect(document.activeElement).not.toBe(clip)
  clip.dispatchEvent(new MouseEvent('pointerdown',{bubbles:true,button:0,clientX:10,clientY:10}))
  expect(document.activeElement).toBe(clip)
  clip.dispatchEvent(new MouseEvent('pointercancel',{bubbles:true,button:0}))
 }finally{await unmount(component)}
})

it('shows a single-sided explicit transition at its independent source-clock interval',async()=>{
 const source=compositionState.document.tracks[0]!
 if(source.kind!=='video')throw new Error('Video required')
 const clip=source.clips[0]!
 compositionState.document={...compositionState.document,tracks:[{...source,transitions:[{id:'explicit-fade',fromClipId:clip.id,toClipId:null,durationTicks:0,kind:'dissolve',timeline:{startTick:'508032000001',durationTick:'254016000000'}}]}]}
 const component=mount(MultiTrackTimeline,{target});await tick()
 try {
  const handle=target.querySelector<HTMLElement>('button.composition-transition-handle')!
  expect(handle).not.toBeNull()
  expect(Number.parseFloat(handle.style.left)).toBeCloseTo(2*compositionState.ui.zoomPxPerSecond)
  expect(Number.parseFloat(handle.style.width)).toBeCloseTo(Math.max(18,compositionState.ui.zoomPxPerSecond))
  handle.click();await tick()
  expect(compositionState.ui.selectedClipIds).toContain(clip.id)
  expect(compositionState.document.tracks[0]!.kind==='video'&&compositionState.document.tracks[0]!.transitions?.[0]?.timeline?.startTick).toBe('508032000001')
 }finally{unmount(component)}
})

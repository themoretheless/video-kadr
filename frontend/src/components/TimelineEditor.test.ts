import { createApp, nextTick } from 'vue'
import { afterEach, describe, expect, it } from 'vitest'

import { ensureCreatorTrackLayout, migrateProjectDocument } from '../project-schema'
import { executeTimelineCommand, timelineState } from '../store'
import TimelineEditor from './TimelineEditor.vue'

let host: HTMLElement | null = null

afterEach(() => {
  host?.remove()
  host = null
})

function mountTimeline(): HTMLElement {
  const document = ensureCreatorTrackLayout(
    migrateProjectDocument({
      videoId: 'media-1',
      name: 'Timeline UI',
      video: { id: 'media-1', duration: 10 },
      edit: {},
    }),
  )
  timelineState.document = document
  timelineState.selectedClipId = 'clip-main'
  host = window.document.createElement('div')
  window.document.body.append(host)
  createApp(TimelineEditor).mount(host)
  return host
}

describe('TimelineEditor', () => {
  it('renders 4 video and 4 audio tracks with accessible controls', () => {
    const element = mountTimeline()
    expect(element.querySelectorAll('.timeline-track')).toHaveLength(8)
    expect(element.querySelectorAll('.track-controls')).toHaveLength(8)
    expect(element.querySelector('[aria-label="Временная шкала проекта"]')).not.toBeNull()
    expect(element.textContent).toContain('8 дорожек')
  })

  it('selects a stable clip and toggles a track lock through a structural command', async () => {
    const element = mountTimeline()
    const clip = element.querySelector<HTMLButtonElement>('.timeline-clip')!
    clip.click()
    expect(timelineState.selectedClipId).toBe('clip-main')

    const lock = element.querySelector<HTMLButtonElement>('.track-controls button[title="Lock"]')!
    lock.click()
    await nextTick()
    expect(timelineState.document?.sequences[0]?.tracks[0]?.locked).toBe(true)
    expect(lock.getAttribute('aria-pressed')).toBe('true')
  })

  it('adds a marker, decorates it from the panel and removes it', async () => {
    const element = mountTimeline()
    timelineState.playheadTick = 3_000_000
    await nextTick()
    const add = element.querySelector<HTMLButtonElement>('button[title*="Добавить маркер"]')!
    add.click()
    await nextTick()
    expect(timelineState.selectedMarkerId).toBeTruthy()
    expect(element.querySelectorAll('.timeline-marker')).toHaveLength(1)
    const flag = element.querySelector<HTMLButtonElement>('.timeline-marker')!
    flag.click()
    await nextTick()
    expect(timelineState.selectedClipId).toBeNull()
    const panel = element.querySelector('.timeline-marker-controls')!
    const label = panel.querySelector<HTMLInputElement>('input[type="text"]')!
    label.value = 'Хук'
    label.dispatchEvent(new Event('change'))
    await nextTick()
    expect(timelineState.document!.sequences[0]!.markers![0]!.label).toBe('Хук')
    const remove = panel.querySelector<HTMLButtonElement>('button[title*="Удалить маркер"]')!
    remove.click()
    await nextTick()
    expect(timelineState.document!.sequences[0]!.markers).toEqual([])
    expect(element.querySelector('.timeline-marker')).toBeNull()
    expect(timelineState.selectedMarkerId).toBeNull()
    timelineState.playheadTick = 0
  })

  it('renders a gap block, selects it and closes it from the toolbar', async () => {
    const element = mountTimeline()
    const track = timelineState.document!.sequences[0]!.tracks[0]!
    const main = track.clips[0]!
    const seam = main.timelineStartTick + main.durationTicks
    executeTimelineCommand({
      kind: 'insert_clip',
      sequenceId: timelineState.document!.activeSequenceId,
      trackId: track.id,
      index: 1,
      clip: {
        id: 'clip-later', mediaId: main.mediaId, timelineStartTick: seam + 2_000_000,
        durationTicks: 1_000_000, sourceInTick: 0, sourceOutTick: 1_000_000, effects: [],
      },
    })
    await nextTick()
    const gap = element.querySelector<HTMLButtonElement>('.timeline-gap')!
    expect(gap).toBeTruthy()
    gap.click()
    await nextTick()
    expect(timelineState.selectedGap).toEqual({ trackId: track.id, startTick: seam })
    const close = element.querySelector<HTMLButtonElement>('button[title*="Закрыть выбранный пропуск"]')!
    close.click()
    await nextTick()
    expect(timelineState.selectedGap).toBeNull()
    const closed = timelineState.document!.sequences[0]!.tracks[0]!.clips.find((clip) => clip.id === 'clip-later')!
    expect(closed.timelineStartTick).toBe(seam)
    expect(element.querySelector('.timeline-gap')).toBeNull()
  })

  it('dims the selected clip through the opacity slider', async () => {
    const element = mountTimeline()
    const clip = element.querySelector<HTMLButtonElement>('.timeline-clip')!
    clip.click()
    await nextTick()
    const slider = element.querySelector<HTMLInputElement>('input[type="range"][max="100"]')!
    slider.value = '40'
    slider.dispatchEvent(new Event('input'))
    await nextTick()
    const findClip = () => timelineState.document!.sequences[0]!.tracks[0]!.clips.find((item) => item.id === 'clip-main')!
    expect(findClip().opacity).toBe(0.4)
    slider.value = '100'
    slider.dispatchEvent(new Event('input'))
    await nextTick()
    expect(findClip().opacity).toBeUndefined()
  })

  it('renders a scrubbing ruler and splits the selected clip at the playhead', async () => {
    const element = mountTimeline()
    timelineState.playheadTick = 5_000_000
    await nextTick()
    expect(element.querySelector('.timeline-ruler-lane')).not.toBeNull()
    expect(element.querySelectorAll('.timeline-ruler-mark').length).toBeGreaterThan(1)
    expect(element.querySelector('.timeline-playhead-line')).not.toBeNull()

    const clip = element.querySelector<HTMLButtonElement>('.timeline-clip')!
    clip.click()
    const split = element.querySelector<HTMLButtonElement>('button[title*="Разделить"]')!
    split.click()
    await nextTick()
    const clips = timelineState.document?.sequences[0]?.tracks[0]?.clips ?? []
    expect(clips.map((item) => item.id)).toEqual(['clip-main', 'clip-main-right'])
    expect(clips[1]?.timelineStartTick).toBe(5_000_000)
  })
})

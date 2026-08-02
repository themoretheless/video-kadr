import { createApp, nextTick } from 'vue'
import { afterEach, describe, expect, it } from 'vitest'

import { ensureCreatorTrackLayout, migrateProjectDocument } from '../project-schema'
import { timelineState } from '../store'
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
})
